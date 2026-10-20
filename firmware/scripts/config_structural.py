"""Private invocation-local reuse of detached literal input evidence.

Only the closed APIs below produce reusable structural results. Mutable source
states, effect outcomes, frames, guards and completion never enter this store.
Ordinary callers and unsupported input providers retain their existing checks.
"""

from functools import wraps
from bisect import bisect_left
from types import MappingProxyType, SimpleNamespace
import copy
import hashlib
import inspect
from collections.abc import Callable, Iterable, Iterator, Mapping, Sequence
from typing import ParamSpec, Protocol, TypeVar, TypedDict, cast
from config_stack import DiagnosticObjects
from elf_image import ElfImage, MappedImage, Section, SectionEntry, Segment, Symbol
from llvm_stack import Instruction
from validation import require


_APIS = frozenset(
    (
        ("config_helpers", "helper_body"),
        ("config_helpers", "helper_coverage"),
        ("config_logger", "logger_body"),
        ("config_refill_children", "child_body"),
        ("config_refill_children", "child_coverage"),
        ("config_utf8", "utf8_body"),
        ("config_widget_children", "widget_body"),
        ("config_source_children", "source_coverage"),
        ("config_tail_children", "tail_coverage"),
    )
)

P = ParamSpec("P")
R = TypeVar("R")
Api = tuple[str, str]
MemoKey = tuple[Api, tuple[tuple[str, object], ...]]
FrozenValue = (
    str
    | bytes
    | int
    | bool
    | None
    | Instruction
    | tuple["FrozenValue", ...]
    | Mapping["FrozenValue", "FrozenValue"]
)


class ElfAttributes(TypedDict, total=False):
    """Fields of an already typed image, before recursive immutable copying.

    Partial synthetic fixtures retain missing-attribute errors at consumption.
    This schema is internal; it does not admit an ELF from unvalidated JSON.
    """

    data: bytes
    sections: Sequence[Section]
    segments: Sequence[Segment]
    section_entries: Sequence[SectionEntry]
    symbol_entries: Sequence[Symbol]
    named: Mapping[str, Section]
    symbols: Mapping[str, int]
    functions: Mapping[str, tuple[int, int, int]]
    mapping_symbols: Sequence[tuple[str, int, int]]


def _freeze(value: object) -> FrozenValue:
    if (
        type(value) is str
        or type(value) is bytes
        or type(value) is int
        or type(value) is bool
        or value is None
    ):
        return value
    if type(value) is list or type(value) is tuple:
        return tuple(_freeze(v) for v in value)
    if type(value) is dict:
        return MappingProxyType({_freeze(k): _freeze(v) for k, v in value.items()})
    if type(value) is Instruction:
        require(
            type(value.address) is int
            and type(value.raw) is bytes
            and type(value.mnemonic) is str
            and type(value.operands) is str,
            "untyped detached instruction",
        )
        return Instruction(value.address, value.raw, value.mnemonic, value.operands)
    raise TypeError("unsupported detached structural input")


class _FrozenElf:
    __slots__ = ("_attributes",)

    _attributes: Mapping[FrozenValue, FrozenValue]
    # __getattr__ serves these same typed fields from the detached mapping.
    data: bytes
    sections: Sequence[Section]
    segments: Sequence[Segment]
    section_entries: Sequence[SectionEntry]
    symbol_entries: Sequence[Symbol]
    named: Mapping[str, Section]
    symbols: Mapping[str, int]
    functions: Mapping[str, tuple[int, int, int]]
    mapping_symbols: Sequence[tuple[str, int, int]]

    def __init__(self, attributes: ElfAttributes) -> None:
        object.__setattr__(self, "_attributes", _freeze(attributes))

    def __getattr__(self, name: str) -> FrozenValue:
        try:
            return self._attributes[name]
        except KeyError:
            raise AttributeError(name) from None

    def __setattr__(self, name: str, value: object) -> None:
        raise AttributeError("immutable detached ELF")

    function_bytes = ElfImage.function_bytes
    section_bytes = ElfImage.section_bytes


class _FrozenObjects(DiagnosticObjects):
    # Properties keep the inherited __dict__ from replacing any consumed input.
    __slots__ = (
        "_image",
        "_sizes",
        "_rows",
        "_memo",
        "_counts",
        "_hashes",
        "_typed_names",
        "_instruction_index",
    )

    _image: _FrozenElf
    _sizes: Mapping[int, int]
    _rows: Mapping[int, Instruction]
    _memo: dict[MemoKey, object]
    _counts: dict[Api, tuple[int, int]]
    _hashes: dict[str | tuple[int, int], str]
    _typed_names: (
        tuple[
            Mapping[str, tuple[int, int, int]],
            Mapping[str, tuple[tuple[str, int], ...]],
        ]
        | None
    )
    _instruction_index: tuple[Mapping[int, Instruction], tuple[int, ...]] | None

    def __init__(
        self, elf: _FrozenElf, sizes: Mapping[int, int], rows: Mapping[int, Instruction]
    ) -> None:
        object.__setattr__(self, "_image", elf)
        object.__setattr__(self, "_sizes", _freeze(sizes))
        object.__setattr__(self, "_rows", _freeze(rows))
        object.__setattr__(self, "_memo", {})
        object.__setattr__(self, "_counts", {})
        object.__setattr__(self, "_hashes", {})
        object.__setattr__(self, "_typed_names", None)
        object.__setattr__(self, "_instruction_index", None)

    @property
    def elf(self) -> MappedImage:
        return self._image

    @elf.setter
    def elf(self, value: MappedImage) -> None:
        raise AttributeError("immutable detached objects")

    @property
    def sizes(self) -> Mapping[int, int]:
        return self._sizes

    @sizes.setter
    def sizes(self, value: Mapping[int, int]) -> None:
        raise AttributeError("immutable detached objects")

    def __setattr__(self, name: str, value: object) -> None:
        raise AttributeError("immutable detached objects")


def _detached_inputs(
    objects: DiagnosticObjects, rows: Mapping[int, Instruction]
) -> tuple[DiagnosticObjects, Mapping[int, Instruction]]:
    """Detach supported providers; custom readers keep the original rechecks.

    The complete raw image, all metadata and all rows are copied. No mutable
    input container or provider method survives into a reused invocation.
    SourceContext is independently typed/revalidated by the existing producer.
    """
    if type(objects) is _FrozenObjects:
        require(rows is objects._rows, "structural proof uses another immutable input package")
        return objects, rows
    if (
        type(objects) is not DiagnosticObjects
        or type(objects.elf) not in (ElfImage, SimpleNamespace)
        or type(rows) is not dict
    ):
        return objects, rows
    try:
        # The input image already has the read-only typed image contract.
        # vars() erases that schema; freezing preserves every field value and
        # only replaces mutable containers with their immutable equivalents.
        elf = _FrozenElf(cast(ElfAttributes, vars(objects.elf)))
        frozen = _FrozenObjects(elf, objects.sizes, rows)
    except TypeError:
        return objects, rows
    require(
        all(
            type(at) is int and type(row) is Instruction and at == row.address
            for at, row in frozen._rows.items()
        ),
        "detached instruction map keys differ from actual records",
    )
    return frozen, frozen._rows


def _raw_composition_inputs(objects: object, instructions: object, source_context: object) -> bool:
    """Select canonical raw inputs, never a supplied checked-state flag.

    Actual fresh detachment is checked separately after the real producer
    and its first full incoming validation. Custom/supplied providers keep
    their independently validating retained path.
    """
    from config_build_source import SourceContext

    return (
        type(objects) is DiagnosticObjects
        and set(vars(objects)) == {"elf", "sizes"}
        and type(objects.elf) in (ElfImage, SimpleNamespace)
        and type(instructions) is dict
        and type(source_context) is SourceContext
        and set(vars(source_context)) == {"elf_sha256", "attempt", "build_options", "sources"}
    )


def _fresh_composition_inputs(
    objects: object,
    instructions: object,
    source_context: object,
    owned: object,
    rows: object,
    current_context: object,
) -> bool:
    """Compare only the original and actually derived input triples.

    No private state, approval token or evaluated result enters this predicate.
    Its caller owns the fixed nonescaping construction and all semantic checks.
    """
    return (
        type(owned) is _FrozenObjects
        and type(owned.elf) is _FrozenElf
        and owned is not objects
        and not vars(owned)
        and rows is owned._rows
        and rows is not instructions
        and current_context is not source_context
    )


def _structural(rows_index: int) -> Callable[[Callable[P, R]], Callable[P, R]]:
    """Wrap only an explicitly listed structural API, preserving its signature."""

    def decorate(function: Callable[P, R]) -> Callable[P, R]:
        api = (function.__module__, function.__name__)
        require(api in _APIS, "unreviewed structural reuse API")
        signature = inspect.signature(function)
        parameters = tuple(signature.parameters)
        row_parameter = parameters[rows_index]

        @wraps(function)
        def checked(*args: P.args, **kwargs: P.kwargs) -> R:
            bound = signature.bind(*args, **kwargs)
            bound.apply_defaults()
            objects: object = bound.arguments["objects"]
            if type(objects) is not _FrozenObjects:
                return function(*args, **kwargs)
            require(
                bound.arguments[row_parameter] is objects._rows,
                "structural proof uses another immutable input package",
            )
            # These APIs take only names, roles, ancestry and contextual scalar
            # tuples in addition to their exact objects/map. Preserve all keys.
            key: MemoKey = (
                api,
                tuple(
                    (name, value)
                    for name, value in bound.arguments.items()
                    if name not in ("objects", row_parameter)
                ),
            )
            try:
                hash(key)
            except TypeError:
                return function(*args, **kwargs)
            misses, hits = objects._counts.get(api, (0, 0))
            if key in objects._memo:
                objects._counts[api] = (misses, hits + 1)
                # Only this exact API writes its return value under this key;
                # the store lives on the detached invocation, not the wrapper.
                return copy.deepcopy(cast(R, objects._memo[key]))
            result = function(*args, **kwargs)
            objects._memo[key] = copy.deepcopy(result)
            misses, hits = objects._counts.get(api, (0, 0))
            objects._counts[api] = (misses + 1, hits)
            return result

        return checked

    return decorate


class InstructionItems(Protocol):
    def items(self) -> Iterable[tuple[int, Instruction]]: ...


def _instruction_addresses(
    objects: DiagnosticObjects | None, rows: InstructionItems
) -> tuple[int, ...] | None:
    """Only exact immutable row inputs share an address index, never a proof.

    Direct private construction does not grant the factory's key invariant.
    Unsupported/malformed maps retain each caller's eager items behavior.
    """
    if (
        type(objects) is not _FrozenObjects
        or rows is not objects._rows
        or type(rows) is not MappingProxyType
    ):
        return None
    cached = objects._instruction_index
    if type(cached) is tuple and len(cached) == 2 and cached[0] is rows:
        return cached[1]
    if not all(
        type(at) is int and type(row) is Instruction and at == row.address
        for at, row in rows.items()
    ):
        return None
    addresses = tuple(sorted(objects._rows))
    object.__setattr__(objects, "_instruction_index", (rows, addresses))
    return addresses


def _instruction_items(
    objects: DiagnosticObjects | None, rows: InstructionItems, start: float, extent: float
) -> Iterator[tuple[int, Instruction]]:
    """Fresh complete span selection, including odd/interior instruction keys."""
    addresses = _instruction_addresses(objects, rows)
    if (
        addresses is not None
        and type(objects) is _FrozenObjects
        and type(start) is type(extent) is int
        and 0 <= start < start + extent <= 2**32
    ):
        end = start + extent
        selected = addresses[bisect_left(addresses, start) : bisect_left(addresses, end)]
        indexed_rows = objects._rows
        return ((at, indexed_rows[at]) for at in selected)
    return ((at, row) for at, row in rows.items() if start <= at < start + extent)


def _image_hash(objects: DiagnosticObjects) -> str:
    if type(objects) is not _FrozenObjects:
        return hashlib.sha256(objects.elf.data).hexdigest()
    if "image" not in objects._hashes:
        objects._hashes["image"] = hashlib.sha256(objects.elf.data).hexdigest()
    return objects._hashes["image"]


def _readonly_hash(objects: DiagnosticObjects, start: int, extent: int) -> str:
    if type(objects) is not _FrozenObjects:
        return hashlib.sha256(objects.read_only(start, extent)).hexdigest()
    key = (start, extent)
    if key not in objects._hashes:
        objects._hashes[key] = hashlib.sha256(objects.read_only(start, extent)).hexdigest()
    return objects._hashes[key]
