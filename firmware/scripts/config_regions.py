"""Internal ownership and interface facts for authored finite regions.

These records are not user input or evidence learned from the current ELF.
A closed machine/source family must establish every allocation, phase, view
and scalar domain before using them. These checks do not establish layout,
instruction coverage, initialization, register transport or a source domain.
They prevent composition from widening facts supplied by those recognizers.
"""
from dataclasses import dataclass
import re
from validation import require


@dataclass(frozen=True)
class Storage:
    space:str
    identity:str
    start:int
    extent:int
    alignment:int
    phase:str
    writable:bool

    def __post_init__(self):
        require(isinstance(self.space,str) and (self.space=='address' or self.space.startswith('stack:')),
                'unreviewed storage coordinate space')
        require(isinstance(self.identity,str) and bool(self.identity) and isinstance(self.phase,str) and bool(self.phase),
                'missing object identity or lifetime phase')
        require(type(self.start) is int and type(self.extent) is int and self.extent>0 and
                (0 if self.space=='address' else -0x100000000)<=self.start<self.start+self.extent<=0x100000000,
                'object interval exceeds its checked coordinate space')
        require(type(self.alignment) is int and 0<self.alignment<=0x100000000 and
                self.alignment&(self.alignment-1)==0 and self.start%self.alignment==0,
                'object alignment differs')
        require(type(self.writable) is bool,'object mutability is not explicit')


@dataclass(frozen=True)
class View:
    storage:Storage
    offset:int
    extent:int
    writable:bool=False

    def __post_init__(self):
        require(isinstance(self.storage,Storage) and type(self.offset) is int and type(self.extent) is int and
                0<=self.offset<self.offset+self.extent<=self.storage.extent,'borrow exceeds its owned object')
        require(type(self.writable) is bool and (not self.writable or self.storage.writable),
                'mutable borrow lacks a mutable owner')

    @property
    def start(self):return self.storage.start+self.offset


def borrow(storage,offset,extent,phase,writable=False):
    require(storage.phase==phase,'borrow belongs to another lifetime phase')
    return View(storage,offset,extent,writable)


def _intersects(space,start,end,other):
    return space==other.space and start<other.start+other.extent and other.start<end


def require_disjoint(storages):
    # Keep one-shot facts for both validation and every pairwise comparison.
    storages=tuple(storages)
    require(all(isinstance(v,Storage) for v in storages),'untyped allocation')
    identities=set()
    for i,storage in enumerate(storages):
        key=(storage.space,storage.identity)
        require(key not in identities,'two active allocations share an object identity');identities.add(key)
        require(all(not _intersects(storage.space,storage.start,storage.start+storage.extent,other)
                    for other in storages[:i]),'distinct live owned allocations overlap')


def require_writes(available,writes,protected):
    """Check declared child writes against exact borrowed output intervals."""
    available=tuple(available);protected=tuple(protected)
    require(all(isinstance(v,View) for v in available) and all(isinstance(v,Storage) for v in protected),
            'untyped output or protected storage')
    for view,offset,extent in writes:
        require(view in available and view.writable,'write lacks the current mutable output borrow')
        require(type(offset) is int and type(extent) is int and 0<=offset<offset+extent<=view.extent,
                'helper write exceeds its exact output borrow')
        start=view.start+offset
        require(all(not _intersects(view.storage.space,start,start+extent,v) for v in protected),
                'helper output intersects protected live storage')


@dataclass(frozen=True)
class Argument:
    kind:str
    view:View|None
    levels:int
    bits:int=0
    domain:frozenset|None=None

    def __post_init__(self):
        require(isinstance(self.kind,str) and bool(self.kind) and type(self.levels) is int and 0<=self.levels<=2,
                'untyped argument/reference level')
        if self.view is not None:
            require(isinstance(self.view,View) and self.bits==0 and self.domain is None,'mixed scalar and borrowed argument')
        else:
            require(self.levels==0 and type(self.bits) is int and 1<=self.bits<=32 and
                    isinstance(self.domain,frozenset) and bool(self.domain) and
                    all(type(v) is int and 0<=v<1<<self.bits for v in self.domain),'unbound scalar domain')


def require_arguments(actual,expected):
    """No receiver-level, pointer/length cross-product or domain widening."""
    slots=re.compile(r'(?:r(?:[0-9]|1[0-2])|lr|S\+(?:0|[1-9][0-9]*))')
    require(isinstance(actual,dict) and isinstance(expected,dict),'missing finite call state')
    require(all(isinstance(k,str) and slots.fullmatch(k) and isinstance(v,Argument)
                for state in (actual,expected) for k,v in state.items()),'untyped call argument location')
    for slot,needed in expected.items():
        require(slot in actual,'required call argument is missing');provided=actual[slot]
        require((provided.kind,provided.levels,provided.bits)==(needed.kind,needed.levels,needed.bits),
                'callee receiver/value/reference ABI differs')
        if needed.view is None:
            require(provided.view is None and provided.domain<=needed.domain,'callee scalar domain is not established')
        else:
            require(provided.view is not None and provided.view.storage==needed.view.storage and
                    provided.view.offset==needed.view.offset and provided.view.extent>=needed.view.extent,
                    'callee argument names another object, phase or insufficient extent')
            require(not needed.view.writable or provided.view.writable,'callee mutable input has only a readonly borrow')


def require_join(alternatives,expected):
    require(isinstance(alternatives,(tuple,list)) and bool(alternatives),'join has no incoming source state')
    for predecessor in alternatives:require_arguments(predecessor,expected)
