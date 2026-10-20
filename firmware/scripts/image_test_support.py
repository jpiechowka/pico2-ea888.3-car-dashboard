"""Explicit mutable image records for host-only negative admission fixtures."""

from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass, field
from types import SimpleNamespace, UnionType
from typing import (
    Literal,
    NotRequired,
    TypeVar,
    Union,
    cast,
    get_args,
    get_origin,
    get_type_hints,
    is_typeddict,
)

from elf_image import Section, SectionEntry, Segment, Symbol
from config_build_source import SourceContext
from config_stack import DiagnosticObjects
from script_values import array_items, object_fields


def fixture_fields(value: object, message: str = "fixture object") -> dict[str, object]:
    """Checked mutable view for deliberate retained-record corruption tests."""
    object_fields(value, message)
    return cast(dict[str, object], value)


def fixture_items(value: object, message: str = "fixture array") -> list[object]:
    """Keep the original array identity when a negative test mutates it."""
    array_items(value, message)
    return cast(list[object], value)


@dataclass
class ImageFixture:
    data: bytes = b""
    segments: list[Segment] = field(default_factory=list)
    sections: list[Section] = field(default_factory=list)
    section_entries: list[SectionEntry] = field(default_factory=list)
    symbol_entries: list[Symbol] = field(default_factory=list)
    named: dict[str, Section] = field(default_factory=dict)
    symbols: dict[str, int] = field(default_factory=dict)
    functions: dict[str, tuple[int, int, int]] = field(default_factory=dict)
    mapping_symbols: list[tuple[str, int, int]] = field(default_factory=list)
    section_data: dict[str, bytes] = field(default_factory=dict)

    def section_bytes(self, name: str) -> bytes:
        if name in self.section_data:
            return self.section_data[name]
        value = self.named[name]
        return self.data[value[4] : value[4] + value[5]]

    def function_bytes(self, name: str) -> bytes:
        address, size, index = self.functions[name]
        section = self.sections[index]
        offset = section[4] + (address & ~1) - section[3]
        return self.data[offset : offset + size]


@dataclass
class QualifiedImageFixture(ImageFixture):
    # Assigned by the source/receipt fixture producer before it is returned.
    fixture_source_context: SourceContext = field(init=False)


class DiagnosticFixture(DiagnosticObjects):
    """Annotation-only mutable metadata view; never instantiate this class.

    Use diagnostic_fixture: production deliberately distinguishes exact provider
    classes. This view adds no runtime behavior, and its image is used only for
    metadata access, never ImageFixture's byte-reader methods.
    """

    elf: ImageFixture
    sizes: dict[int, int]


def diagnostic_fixture(elf: ImageFixture, sizes: dict[int, int]) -> DiagnosticFixture:
    """Preserve the original exact providers and all mutable field identities."""
    if type(elf) is not ImageFixture or elf.section_data:
        raise AssertionError("diagnostic fixture requires plain image metadata")
    namespace = SimpleNamespace(
        data=elf.data,
        segments=elf.segments,
        sections=elf.sections,
        section_entries=elf.section_entries,
        symbol_entries=elf.symbol_entries,
        named=elf.named,
        symbols=elf.symbols,
        functions=elf.functions,
        mapping_symbols=elf.mapping_symbols,
    )
    # All field values come from the checked fixture schema above. SimpleNamespace
    # and DiagnosticObjects must remain exact runtime classes for detachment.
    return cast(DiagnosticFixture, DiagnosticObjects(namespace, sizes))


def section_fields(values: Sequence[int]) -> Section:
    if len(values) != 10:
        raise ValueError("fixture section needs ten ELF fields")
    return (
        values[0],
        values[1],
        values[2],
        values[3],
        values[4],
        values[5],
        values[6],
        values[7],
        values[8],
        values[9],
    )


def segment_fields(values: Sequence[int]) -> Segment:
    if len(values) != 8:
        raise ValueError("fixture segment needs eight ELF fields")
    return (
        values[0],
        values[1],
        values[2],
        values[3],
        values[4],
        values[5],
        values[6],
        values[7],
    )


FixtureT = TypeVar("FixtureT")


def dynamic_call(function: Callable[..., object], *args: object, **kwargs: object) -> object:
    """Invoke deliberate invalid arguments without changing a production signature."""
    return function(*args, **kwargs)


def fixture_mapping(value: object) -> dict[object, object]:
    """Expose a test dictionary's backing storage for deliberate invalid mutations."""
    if type(value) is not dict:
        raise AssertionError("fixture mapping requires a plain dictionary")
    # Tests intentionally violate the original key/value schema. Keep the same
    # backing dictionary so isolation and malformed-input checks exercise it.
    return cast(dict[object, object], value)


def matches_fixture(value: object, shape: object) -> bool:
    """Check fixture shape only; production still owns all semantic admission."""
    if shape is object:
        return True
    if shape in (int, str, bool, bytes, float, type(None)):
        return type(value) is shape
    origin: object = get_origin(shape)
    args: tuple[object, ...] = get_args(shape)
    if origin is NotRequired:
        return matches_fixture(value, args[0])
    if origin in (Union, UnionType):
        return any(matches_fixture(value, option) for option in args)
    if origin is Literal:
        return any(type(value) is type(option) and value == option for option in args)
    if origin is list:
        if type(value) is not list:
            return False
        members: list[object] = value
        return all(matches_fixture(member, args[0]) for member in members)
    if origin is tuple:
        if type(value) is not tuple:
            return False
        cells: tuple[object, ...] = value
        return len(cells) == len(args) and all(matches_fixture(v, t) for v, t in zip(cells, args))
    if origin is dict:
        if type(value) is not dict:
            return False
        pairs: dict[object, object] = value
        return all(
            matches_fixture(k, args[0]) and matches_fixture(v, args[1]) for k, v in pairs.items()
        )
    if is_typeddict(shape):
        if type(value) is not dict:
            return False
        fields: dict[str, object] = get_type_hints(shape, include_extras=True)
        record: dict[object, object] = value
        return all(
            (key in record and matches_fixture(record[key], field))
            or (key not in record and get_origin(field) is NotRequired)
            for key, field in fields.items()
        )
    raise AssertionError(f"unsupported fixture schema {shape!r}")


def checked_fixture(value: object, shape: type[FixtureT]) -> FixtureT:
    if not matches_fixture(value, shape):
        raise AssertionError(f"fixture does not match {shape.__name__}")
    # The recursive checker above validates every declared field, including
    # discriminated unions; retain extra provenance without projecting it away.
    return cast(FixtureT, value)


def required_fixture(value: FixtureT | None) -> FixtureT:
    """Require a fixture branch while preserving its original identity under -O."""
    if value is None:
        raise AssertionError("required fixture branch is absent")
    return value


def fixture_record(value: object) -> Mapping[str, object]:
    """Read a retained fixture mapping, including an immutable description."""
    if not isinstance(value, Mapping) or not all(type(key) is str for key in value):
        raise AssertionError("expected fixture string-keyed mapping")
    return cast(Mapping[str, object], value)


def fixture_records(value: object) -> tuple[Mapping[str, object], ...]:
    """Check description elements without copying their shared tuple identity."""
    if type(value) is not tuple:
        raise AssertionError("expected fixture record tuple")
    for item in value:
        fixture_record(item)
    return cast(tuple[Mapping[str, object], ...], value)
