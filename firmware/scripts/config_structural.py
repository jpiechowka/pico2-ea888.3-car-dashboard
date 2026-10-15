"""Private invocation-local reuse of detached literal input evidence.

Only the closed APIs below produce reusable structural results. Mutable source
states, effect outcomes, frames, guards and completion never enter this store.
Ordinary callers and unsupported input providers retain their existing checks.
"""
from functools import wraps
from types import MappingProxyType,SimpleNamespace
import copy
import hashlib
import inspect
from config_stack import DiagnosticObjects
from elf_image import ElfImage
from llvm_stack import Instruction
from validation import require


_APIS=frozenset((
    ('config_helpers','helper_body'),('config_helpers','helper_coverage'),
    ('config_logger','logger_body'),('config_refill_children','child_body'),
    ('config_refill_children','child_coverage'),('config_utf8','utf8_body'),
    ('config_widget_children','widget_body'),
))


def _freeze(value):
    if type(value) in (str,bytes,int,bool,type(None)):return value
    if type(value) in (list,tuple):return tuple(_freeze(v) for v in value)
    if type(value) is dict:return MappingProxyType({_freeze(k):_freeze(v) for k,v in value.items()})
    if type(value) is Instruction:
        require(type(value.address) is int and type(value.raw) is bytes and
            type(value.mnemonic) is str and type(value.operands) is str,'untyped detached instruction')
        return Instruction(value.address,value.raw,value.mnemonic,value.operands)
    raise TypeError('unsupported detached structural input')


class _FrozenElf:
    __slots__=('_attributes',)

    def __init__(self,attributes):object.__setattr__(self,'_attributes',_freeze(attributes))
    def __getattr__(self,name):
        try:return self._attributes[name]
        except KeyError:raise AttributeError(name) from None
    def __setattr__(self,name,value):raise AttributeError('immutable detached ELF')
    function_bytes=ElfImage.function_bytes
    section_bytes=ElfImage.section_bytes


class _FrozenObjects(DiagnosticObjects):
    # Properties keep the inherited __dict__ from replacing any consumed input.
    __slots__=('_image','_sizes','_rows','_memo','_counts','_hashes')

    def __init__(self,elf,sizes,rows):
        object.__setattr__(self,'_image',elf)
        object.__setattr__(self,'_sizes',_freeze(sizes))
        object.__setattr__(self,'_rows',_freeze(rows))
        object.__setattr__(self,'_memo',{})
        object.__setattr__(self,'_counts',{})
        object.__setattr__(self,'_hashes',{})
    @property
    def elf(self):return self._image
    @property
    def sizes(self):return self._sizes
    def __setattr__(self,name,value):raise AttributeError('immutable detached objects')


def _detached_inputs(objects,rows):
    """Detach supported providers; custom readers keep the original rechecks.

    The complete raw image, all metadata and all rows are copied. No mutable
    input container or provider method survives into a reused invocation.
    SourceContext is independently typed/revalidated by the existing producer.
    """
    if type(objects) is _FrozenObjects:
        require(rows is objects._rows,'structural proof uses another immutable input package')
        return objects,rows
    if type(objects) is not DiagnosticObjects or type(objects.elf) not in (ElfImage,SimpleNamespace) or type(rows) is not dict:
        return objects,rows
    try:
        elf=_FrozenElf(vars(objects.elf))
        frozen=_FrozenObjects(elf,objects.sizes,rows)
    except TypeError:return objects,rows
    require(all(type(at) is int and type(row) is Instruction and at==row.address for at,row in frozen._rows.items()),
        'detached instruction map keys differ from actual records')
    return frozen,frozen._rows


def _structural(rows_index):
    """Wrap only an explicitly listed structural API, preserving its signature."""
    def decorate(function):
        api=(function.__module__,function.__name__)
        require(api in _APIS,'unreviewed structural reuse API')
        signature=inspect.signature(function);parameters=tuple(signature.parameters)
        row_parameter=parameters[rows_index]
        @wraps(function)
        def checked(*args,**kwargs):
            bound=signature.bind(*args,**kwargs);bound.apply_defaults()
            objects=bound.arguments['objects']
            if type(objects) is not _FrozenObjects:return function(*args,**kwargs)
            require(bound.arguments[row_parameter] is objects._rows,'structural proof uses another immutable input package')
            # These APIs take only names, roles, ancestry and contextual scalar
            # tuples in addition to their exact objects/map. Preserve all keys.
            key=(api,tuple((name,value) for name,value in bound.arguments.items() if name not in ('objects',row_parameter)))
            try:hash(key)
            except TypeError:return function(*args,**kwargs)
            misses,hits=objects._counts.get(api,(0,0))
            if key in objects._memo:
                objects._counts[api]=(misses,hits+1)
                return copy.deepcopy(objects._memo[key])
            result=function(*args,**kwargs)
            objects._memo[key]=copy.deepcopy(result)
            misses,hits=objects._counts.get(api,(0,0))
            objects._counts[api]=(misses+1,hits)
            return result
        return checked
    return decorate


def _image_hash(objects):
    if type(objects) is not _FrozenObjects:return hashlib.sha256(objects.elf.data).hexdigest()
    if 'image' not in objects._hashes:objects._hashes['image']=hashlib.sha256(objects.elf.data).hexdigest()
    return objects._hashes['image']


def _readonly_hash(objects,start,extent):
    if type(objects) is not _FrozenObjects:return hashlib.sha256(objects.read_only(start,extent)).hexdigest()
    key=(start,extent)
    if key not in objects._hashes:objects._hashes[key]=hashlib.sha256(objects.read_only(start,extent)).hexdigest()
    return objects._hashes[key]
