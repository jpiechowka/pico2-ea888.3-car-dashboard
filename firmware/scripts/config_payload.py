"""Complete finite typed payloads; caller/source/default/sink closure is separate."""
import re
import struct
from config_payload_contracts import PAYLOAD_GRAMMARS,PAYLOAD_ROLES
from config_stack import canonical_symbol,typed_function
from config_finite import _complete_body
from validation import require


def payload_object(objects,address,descriptor):
    kind=descriptor['kind']
    if kind in ('literal','enum_lengths'):
        value=bytes.fromhex(descriptor['bytes']) if kind=='literal' else bytes(descriptor['lengths'])
        require(kind!='literal' or len(value)==descriptor['length'],'inconsistent payload literal contract')
        require(objects.read_only(address,len(value))==value,'payload label/ordered enum lengths differ')
        return ()
    if kind=='enum_pointers':
        require(address%4==0,'unaligned payload enum pointer table')
        labels=[v.encode() for v in descriptor['labels']]
        words=struct.unpack('<'+'I'*len(labels),objects.read_only(address,4*len(labels)))
        for pointer,label in zip(words,labels):
            require(objects.read_only(pointer,len(label))==label,'payload ordered enum label differs')
        return ()
    require(kind in ('debug_vtable','raw_debug_callback') and len(descriptor['roles'])==1,'unreviewed payload object')
    if kind=='debug_vtable':
        require(address%4==0,'unaligned payload Debug table')
        words=struct.unpack('<4I',objects.read_only(address,16))
        require(words[:3]==tuple(descriptor['layout']),'payload Debug table layout differs')
        pointer=words[3]
    else:pointer=address
    require(pointer&1,'payload callback is not Thumb')
    role=descriptor['roles'][0];extent,frame,_=typed_function(objects,pointer&~1,role)
    return ((pointer&~1,role,extent,frame),)


def _payload_shape(objects,name,instructions,role,shape,ancestry):
    proof=_complete_body(objects,name,instructions,role,shape,PAYLOAD_GRAMMARS[shape],
                         payload_object,payload_body,ancestry)
    proof.update(requires_value_source_and_live_sink=True,requires_options=(0x60000020,0,0),
                 requires_pretty_inventory_exclusion=True)
    return proof


def payload_body(objects,name,instructions,outline_role=None,_ancestry=()):
    """Select exactly one full typed body; never widen clones by demangled name."""
    require(name in objects.elf.functions,'missing payload body')
    start=objects.elf.functions[name][0]&~1;require(start not in _ancestry,'payload outline activation cycle')
    role=outline_role or canonical_symbol(name)
    require(not outline_role or re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',name),'payload outline origin differs')
    require(role in PAYLOAD_ROLES,'unreviewed exact payload receiver/context')
    results=[]
    for shape in PAYLOAD_ROLES[role]:
        try:results.append(_payload_shape(objects,name,instructions,role,shape,_ancestry))
        except AssertionError:continue
    require(len(results)==1,'missing, changed or ambiguous complete typed payload shape')
    return results[0]
