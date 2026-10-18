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
from config_boot_source import first_boot_source_contract
from validation import require

SOURCE_PATHS = ("firmware/pico2/src/main.rs", "firmware/pico2/src/view_config.rs")
SOURCE_CAP = 65536
INPUT_CAP = 262144
KIND = "captured_first_boot_sources"
_HASH = re.compile("[0-9a-f]{64}")


def _sha(raw):
    return hashlib.sha256(raw).hexdigest()


def _unique(pairs):
    result = {}
    for name, value in pairs:
        require(
            type(name) is str and name not in result, "duplicate or nonstring source-context key"
        )
        result[name] = value
    return result


def _invalid_constant(value):
    raise AssertionError("nonfinite source-context JSON scalar")


def _json(raw):
    try:
        return json.loads(
            raw.decode("utf-8"), object_pairs_hook=_unique, parse_constant=_invalid_constant
        )
    except (UnicodeDecodeError, json.JSONDecodeError):
        raise AssertionError("invalid UTF-8/JSON source context") from None


def _text(raw, limit):
    require(
        type(raw) is bytes and 0 < len(raw) <= limit,
        "missing or oversized source-context byte member",
    )
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        raise AssertionError("source-context member is not UTF-8") from None


def _member(raw):
    return dict(utf8=_text(raw, INPUT_CAP), length=len(raw), sha256=_sha(raw))


def _read_member(value, limit):
    require(
        type(value) is dict
        and set(value) == {"utf8", "length", "sha256"}
        and type(value["utf8"]) is str
        and type(value["length"]) is int
        and type(value["sha256"]) is str
        and _HASH.fullmatch(value["sha256"]),
        "source-context member shape/type differs",
    )
    try:
        raw = value["utf8"].encode("utf-8")
    except UnicodeEncodeError:
        raise AssertionError("source-context member is not scalar UTF-8") from None
    _text(raw, limit)
    require(
        len(raw) == value["length"] and _sha(raw) == value["sha256"],
        "source-context member length/hash differs",
    )
    return raw


@dataclass(frozen=True)
class SourceContext:
    elf_sha256: str
    attempt: str
    build_options: bytes
    sources: tuple

    def __post_init__(self):
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

    def to_record(self):
        return dict(
            schema=1,
            kind=KIND,
            elf_sha256=self.elf_sha256,
            attempt=self.attempt,
            build_options=_member(self.build_options),
            sources=[dict(path=name, **_member(raw)) for name, raw in self.sources],
        )

    def to_bytes(self):
        return (
            json.dumps(self.to_record(), sort_keys=True, separators=(",", ":"), ensure_ascii=False)
            + "\n"
        ).encode("utf-8")

    def documents(self):
        return {Path(name).name: raw for name, raw in self.sources}

    def binding(self):
        return dict(
            kind=KIND,
            attempt=self.attempt,
            elf_sha256=self.elf_sha256,
            build_options_sha256=_sha(self.build_options),
            sources={name: dict(length=len(raw), sha256=_sha(raw)) for name, raw in self.sources},
        )


def make_source_context(elf_bytes, attempt, build_options, before):
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


def validate_source_context(value, elf, attempt=None, build_options=None):
    require(
        type(value) is SourceContext and type(elf.data) is bytes,
        "missing explicit typed source context",
    )
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


def parse_source_context(raw, elf, attempt=None, build_options=None):
    require(
        type(raw) is bytes and 0 < len(raw) <= INPUT_CAP,
        "missing or oversized serialized source context",
    )
    record = _json(raw)
    require(
        type(record) is dict
        and set(record) == {"schema", "kind", "elf_sha256", "attempt", "build_options", "sources"}
        and type(record["schema"]) is int
        and record["schema"] == 1
        and type(record["kind"]) is str
        and record["kind"] == KIND
        and type(record["sources"]) is list
        and len(record["sources"]) == 2,
        "source-context record shape/version differs",
    )
    sources = []
    for value in record["sources"]:
        require(
            type(value) is dict
            and set(value) == {"path", "utf8", "length", "sha256"}
            and type(value["path"]) is str,
            "source-context source entry has an unknown shape",
        )
        sources.append(
            (
                value["path"],
                _read_member({k: v for k, v in value.items() if k != "path"}, SOURCE_CAP),
            )
        )
    context = SourceContext(
        record["elf_sha256"],
        record["attempt"],
        _read_member(record["build_options"], INPUT_CAP),
        tuple(sources),
    )
    return validate_source_context(context, elf, attempt, build_options)


def load_source_context(path, elf, attempt=None, build_options=None):
    with Path(path).open("rb") as stream:
        raw = stream.read(INPUT_CAP + 1)
    return parse_source_context(raw, elf, attempt, build_options)


def write_source_context(path, value):
    require(type(value) is SourceContext, "source sidecar has no typed context")
    value = SourceContext(value.elf_sha256, value.attempt, value.build_options, value.sources)
    with Path(path).open("xb") as stream:
        stream.write(value.to_bytes())


def inspect_source_context(value, elf):
    value = validate_source_context(value, elf)
    # Recognition proves only authored source semantics. The application
    # history/machine/reference/alias/loader join must still be composed.
    return first_boot_source_contract(value.documents())
