"""Invocation-local immutable representation of freshly checked copy bodies.

This does not cache copy calls or dynamic states. Only complete equal child
descriptions from the same detached provider/map may share after each existing
copy API succeeds. Unsupported providers/values keep the checked unshared path.
"""

from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Literal, Protocol, TypeAlias, TypedDict, cast, overload
from types import MappingProxyType
from config_body_types import InstructionMap, Callback, OrdinaryEdge, IndirectSite
from config_refill_children import OpaqueCopyResult, Writes

if TYPE_CHECKING:
    from config_decoded_sources import PhaseGraph
from config_structural import _FrozenObjects
from validation import require


FrozenValue: TypeAlias = (
    str
    | bytes
    | int
    | bool
    | None
    | tuple["FrozenValue", ...]
    | Mapping["FrozenValue", "FrozenValue"]
)


class LoggerDescription(Protocol):
    """Read-only field view of a current logger body after recursive freezing."""

    @overload
    def __getitem__(self, key: Literal["function", "extent", "frame", "caller_extent"]) -> int: ...
    @overload
    def __getitem__(self, key: Literal["shape", "role"]) -> str: ...
    @overload
    def __getitem__(self, key: Literal["objects"]) -> Mapping[int, int]: ...
    @overload
    def __getitem__(self, key: Literal["callbacks"]) -> tuple[Callback, ...]: ...
    @overload
    def __getitem__(self, key: Literal["ordinary_edges"]) -> tuple[OrdinaryEdge, ...]: ...
    @overload
    def __getitem__(self, key: Literal["indirect_sites"]) -> tuple[IndirectSite, ...]: ...
    @overload
    def __getitem__(self, key: Literal["transfers"]) -> tuple[tuple[int, str, str], ...]: ...
    @overload
    def __getitem__(
        self, key: Literal["requires_parent_outline_abi", "manual_frame_requires_complete_builtin"]
    ) -> str | None: ...
    @overload
    def __getitem__(
        self,
        key: Literal[
            "requires_runtime_ownership_entry_and_guard_closure",
            "requires_typed_inputs_live_sink_and_default_options",
            "reference_only",
        ],
    ) -> bool: ...
    @overload
    def __getitem__(self, key: str) -> object: ...


class SharedCopyResult(TypedDict):
    children: tuple[LoggerDescription, ...]
    possible_writes: Writes
    conservative_activation_frames: tuple[int, ...]
    physical_activation_paths: tuple[tuple[int, ...], ...]
    restored_tail_storage: bool
    normal_preserves: tuple[str, ...]
    initialized_semantic_bytes_not_inferred: bool
    requires_actual_parent_pointer_count_transport: bool


def _immutable_description(value: object) -> FrozenValue:
    memo: dict[int, FrozenValue] = {}
    active: set[int] = set()

    def visit(item: object) -> FrozenValue:
        kind = type(item)
        if kind in (str, bytes, int, bool, type(None)):
            return cast(str | bytes | int | bool | None, item)
        if kind not in (dict, tuple):
            raise TypeError("unsupported immutable copy description")
        identity = id(item)
        if identity in active:
            raise TypeError("cyclic immutable copy description")
        if identity in memo:
            return memo[identity]
        active.add(identity)
        result: FrozenValue
        if kind is tuple:
            result = tuple(visit(v) for v in cast(tuple[object, ...], item))
        else:
            result = MappingProxyType(
                {visit(k): visit(v) for k, v in cast(dict[object, object], item).items()}
            )
        active.remove(identity)
        memo[identity] = result
        return result

    return visit(value)


class _CopyDescriptions:
    def __init__(self, objects: object, rows: InstructionMap) -> None:
        self.objects = objects
        self.rows = rows
        self._values: dict[tuple[int, PhaseGraph], FrozenValue] = {}

    @overload
    def checked(
        self, effect: OpaqueCopyResult, objects: object, rows: InstructionMap, target: int
    ) -> SharedCopyResult: ...

    @overload
    def checked(
        self, effect: Mapping[str, object], objects: object, rows: InstructionMap, target: int
    ) -> Mapping[str, object]: ...

    def checked(
        self, effect: Mapping[str, object], objects: object, rows: InstructionMap, target: int
    ) -> Mapping[str, object]:
        require(
            objects is self.objects and rows is self.rows,
            "Sources copy description changes its invocation input context",
        )
        require(type(target) is int, "Sources copy description target is not a typed address")
        # Unsupported readers still execute every original copy/body/provider
        # check. They acquire no new structural reuse or frozen-input claim.
        if type(objects) is not _FrozenObjects:
            return effect
        if not isinstance(objects, _FrozenObjects):
            raise AssertionError("Sources copy description changes its detached map")
        require(rows is objects._rows, "Sources copy description changes its detached map")
        require(
            type(effect) is dict and type(effect.get("children")) is tuple,
            "Sources copy description is not a complete checked child tuple",
        )
        from config_decoded_sources import _phase_graph

        try:
            frozen = _immutable_description(effect["children"])
        except TypeError:
            return effect
        # The complete typed values, internal alias edges, contextual target
        # and every additional supported field participate, never a digest.
        key = (target, _phase_graph(effect["children"]))
        if key not in self._values:
            self._values[key] = frozen
        return dict(effect, children=self._values[key])
