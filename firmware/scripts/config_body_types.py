"""Typed literal-body contracts and their structural evidence records."""

from collections.abc import Callable, Mapping, Sequence
from typing import Generic, Literal, NotRequired, TypeVar, TypedDict

from config_stack import DiagnosticObjects
from llvm_stack import Instruction

Descriptor = TypeVar("Descriptor")
Callback = tuple[int, str, int, int]
OrdinaryEdge = tuple[int, str, int, str, int]
IndirectSite = tuple[int, str, str]
InstructionMap = Mapping[int, Instruction]
FunctionConsumer = Callable[[DiagnosticObjects, int, str], tuple[int, int, str]]
ObjectConsumer = Callable[[DiagnosticObjects, int, Descriptor], Sequence[Callback]]


class FrameProof(TypedDict):
    frame: int


OutlineConsumer = Callable[
    [DiagnosticObjects, str, InstructionMap, str, tuple[int, ...]], FrameProof
]


class SourceLocationObject(TypedDict):
    kind: Literal["source_location"]
    role: str


class BodyContract(TypedDict, Generic[Descriptor]):
    extent: int
    frame: NotRequired[int]
    mapping: NotRequired[Sequence[tuple[int, int, str]]]
    instructions: Sequence[tuple[int, int, str]]
    objects: Sequence[Descriptor]
    data: NotRequired[Sequence[tuple[int, int, str]]]


class BodyRecord(FrameProof):
    shape: str
    role: str
    function: int
    extent: int
    objects: dict[int, int]
    callbacks: tuple[Callback, ...]
    ordinary_edges: tuple[OrdinaryEdge, ...]
    indirect_sites: tuple[IndirectSite, ...]
    requires_parent_outline_abi: str | None
    caller_extent: NotRequired[int]
