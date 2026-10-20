"""Explicit captured source pairing for the finite first-Boot recognizer.

This bounded input is separate from the 65536-byte memory-result format.
It binds recorded bytes/attempt/options to an ELF, not compiler-consumed-input
attestation (H8), ownership or diagnostic success. No adjacent-file search or
current-checkout fallback is implemented. Source semantics are recomputed.
"""

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
from typing import NoReturn, TypedDict
from config_boot_source import BootSourceContract, first_boot_source_contract
from elf_image import ImageBytes
from script_values import array_items, integer_value, object_fields, string_value
from validation import require

SOURCE_PATHS = ("firmware/pico2/src/main.rs", "firmware/pico2/src/view_config.rs")
SOURCE_CAP = 65536
INPUT_CAP = 262144
KIND = "captured_first_boot_sources"
_HASH = re.compile("[0-9a-f]{64}")


class SourceMemberRecord(TypedDict):
    utf8: str
    length: int
    sha256: str


class NamedSourceMember(SourceMemberRecord):
    path: str


class SourceContextRecord(TypedDict):
    schema: int
    kind: str
    elf_sha256: str
    attempt: str
    build_options: SourceMemberRecord
    sources: list[NamedSourceMember]


class SourceIdentity(TypedDict):
    length: int
    sha256: str


class SourceBinding(TypedDict):
    kind: str
    attempt: str
    elf_sha256: str
    build_options_sha256: str
    sources: dict[str, SourceIdentity]


def _sha(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def _unique(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for name, value in pairs:
        require(
            type(name) is str and name not in result, "duplicate or nonstring source-context key"
        )
        result[name] = value
    return result


def _invalid_constant(value: str) -> NoReturn:
    raise AssertionError("nonfinite source-context JSON scalar")


def _json(raw: bytes) -> object:
    try:
        return json.loads(
            raw.decode("utf-8"), object_pairs_hook=_unique, parse_constant=_invalid_constant
        )
    except (UnicodeDecodeError, json.JSONDecodeError):
        raise AssertionError("invalid UTF-8/JSON source context") from None


def _text(raw: bytes, limit: int) -> str:
    require(
        type(raw) is bytes and 0 < len(raw) <= limit,
        "missing or oversized source-context byte member",
    )
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        raise AssertionError("source-context member is not UTF-8") from None


def _member(raw: bytes) -> SourceMemberRecord:
    return dict(utf8=_text(raw, INPUT_CAP), length=len(raw), sha256=_sha(raw))


def _read_member(value: object, limit: int) -> bytes:
    message = "source-context member shape/type differs"
    fields = object_fields(value, message)
    require(set(fields) == {"utf8", "length", "sha256"}, message)
    text = string_value(fields["utf8"], message)
    length = integer_value(fields["length"], message)
    digest = string_value(fields["sha256"], message)
    require(
        _HASH.fullmatch(digest),
        message,
    )
    try:
        raw = text.encode("utf-8")
    except UnicodeEncodeError:
        raise AssertionError("source-context member is not scalar UTF-8") from None
    _text(raw, limit)
    require(
        len(raw) == length and _sha(raw) == digest,
        "source-context member length/hash differs",
    )
    return raw


@dataclass(frozen=True)
class SourceContext:
    elf_sha256: str
    attempt: str
    build_options: bytes
    sources: tuple[tuple[str, bytes], ...]

    def __post_init__(self) -> None:
        require(
            type(self.elf_sha256) is str
            and _HASH.fullmatch(self.elf_sha256)
            and type(self.attempt) is str
            and re.fullmatch("[A-Za-z0-9][A-Za-z0-9_-]*", self.attempt),
            "source-context ELF identity or attempt differs",
        )
        _text(self.build_options, INPUT_CAP)
        require(
            type(_json(self.build_options)) is dict,
            "source-context options are not JSON object bytes",
        )
        require(
            type(self.sources) is tuple
            and len(self.sources) == 2
            and all(type(v) is tuple and len(v) == 2 and type(v[0]) is str for v in self.sources)
            and tuple(v[0] for v in self.sources) == SOURCE_PATHS,
            "source context loses exact ordered source members",
        )
        for name, raw in self.sources:
            _text(raw, SOURCE_CAP)
        require(
            len(self.to_bytes()) <= INPUT_CAP,
            "serialized source context exceeds its separate input cap",
        )

    def to_record(self) -> SourceContextRecord:
        return dict(
            schema=1,
            kind=KIND,
            elf_sha256=self.elf_sha256,
            attempt=self.attempt,
            build_options=_member(self.build_options),
            sources=[dict(path=name, **_member(raw)) for name, raw in self.sources],
        )

    def to_bytes(self) -> bytes:
        return (
            json.dumps(self.to_record(), sort_keys=True, separators=(",", ":"), ensure_ascii=False)
            + "\n"
        ).encode("utf-8")

    def documents(self) -> dict[str, bytes]:
        return {Path(name).name: raw for name, raw in self.sources}

    def binding(self) -> SourceBinding:
        return dict(
            kind=KIND,
            attempt=self.attempt,
            elf_sha256=self.elf_sha256,
            build_options_sha256=_sha(self.build_options),
            sources={name: dict(length=len(raw), sha256=_sha(raw)) for name, raw in self.sources},
        )


def make_source_context(
    elf_bytes: bytes, attempt: str, build_options: bytes, before: dict[str, bytes]
) -> SourceContext:
    require(
        type(elf_bytes) is bytes
        and elf_bytes
        and type(before) is dict
        and all(name in before for name in SOURCE_PATHS),
        "captured source snapshot or ELF is missing",
    )
    return SourceContext(
        _sha(elf_bytes),
        attempt,
        build_options,
        tuple((name, before[name]) for name in SOURCE_PATHS),
    )


def validate_source_context(
    value: object,
    elf: ImageBytes,
    attempt: str | None = None,
    build_options: bytes | None = None,
) -> SourceContext:
    if type(value) is not SourceContext or type(elf.data) is not bytes:
        raise AssertionError("missing explicit typed source context")
    # Revalidate even a forged/mutated Python instance. No semantic verdict or
    # caller-owned source flag can stand in for these raw members.
    fresh = SourceContext(value.elf_sha256, value.attempt, value.build_options, value.sources)
    require(
        fresh == value and value.elf_sha256 == _sha(elf.data),
        "source context names a different current ELF",
    )
    if attempt is not None:
        require(type(attempt) is str and value.attempt == attempt, "source-context attempt changed")
    if build_options is not None:
        require(
            type(build_options) is bytes and value.build_options == build_options,
            "immutable source-context options changed",
        )
    return fresh


def parse_source_context(
    raw: object,
    elf: ImageBytes,
    attempt: str | None = None,
    build_options: bytes | None = None,
) -> SourceContext:
    if not (type(raw) is bytes and 0 < len(raw) <= INPUT_CAP):
        raise AssertionError("missing or oversized serialized source context")
    shape_message = "source-context record shape/version differs"
    record = object_fields(_json(raw), shape_message)
    require(
        set(record) == {"schema", "kind", "elf_sha256", "attempt", "build_options", "sources"}
        and type(record["schema"]) is int
        and record["schema"] == 1
        and type(record["kind"]) is str
        and record["kind"] == KIND,
        shape_message,
    )
    source_records = array_items(record["sources"], shape_message)
    require(len(source_records) == 2, shape_message)
    sources = []
    for source_record in source_records:
        member_message = "source-context source entry has an unknown shape"
        value = object_fields(source_record, member_message)
        require(
            set(value) == {"path", "utf8", "length", "sha256"},
            member_message,
        )
        sources.append(
            (
                string_value(value["path"], member_message),
                _read_member({k: v for k, v in value.items() if k != "path"}, SOURCE_CAP),
            )
        )
    context = SourceContext(
        string_value(record["elf_sha256"], "source-context ELF identity or attempt differs"),
        string_value(record["attempt"], "source-context ELF identity or attempt differs"),
        _read_member(record["build_options"], INPUT_CAP),
        tuple(sources),
    )
    return validate_source_context(context, elf, attempt, build_options)


def load_source_context(
    path: str | Path,
    elf: ImageBytes,
    attempt: str | None = None,
    build_options: bytes | None = None,
) -> SourceContext:
    with Path(path).open("rb") as stream:
        raw = stream.read(INPUT_CAP + 1)
    return parse_source_context(raw, elf, attempt, build_options)


def write_source_context(path: str | Path, value: object) -> None:
    if type(value) is not SourceContext:
        raise AssertionError("source sidecar has no typed context")
    value = SourceContext(value.elf_sha256, value.attempt, value.build_options, value.sources)
    with Path(path).open("xb") as stream:
        stream.write(value.to_bytes())


def inspect_source_context(value: object, elf: ImageBytes) -> BootSourceContract:
    value = validate_source_context(value, elf)
    # Recognition proves only authored source semantics. The application
    # history/machine/reference/alias/loader join must still be composed.
    return first_boot_source_contract(value.documents())
