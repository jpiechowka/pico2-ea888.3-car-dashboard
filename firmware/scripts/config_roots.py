"""Complete raw Arguments adapters; caller roots and formatting closure remain."""
import re
from config_root_contracts import ROOT_WRAPPERS,ROOT_WRAPPER_ROLES
from config_finite import _complete_body
from config_stack import canonical_symbol,typed_function
from validation import require


def root_object(objects,address,descriptor):
    kind=descriptor['kind']
    if kind=='raw_arguments_callback':
        require(address&1,'root Arguments callback is not Thumb')
        role=descriptor['role'];extent,frame,_=typed_function(objects,address&~1,role)
        require((extent,frame)==(descriptor['extent'],descriptor['frame']),'root raw callback extent/frame differs')
        return ((address&~1,role,extent,frame),)
    if kind=='default_template':
        objects.default_template(address,descriptor['role'])
        return ()
    require(kind=='tagged_literal','unreviewed root adapter object')
    value=bytes.fromhex(descriptor['hex']);word=descriptor['word']
    require(word&1 and word>>1==len(value) and objects.read_only(address,len(value))==value,
            'root tagged literal bytes/length differ')
    return ()


def root_adapter(objects,name,instructions,outline_role=None,_ancestry=()):
    require(name in objects.elf.functions,'missing raw Arguments adapter')
    start=objects.elf.functions[name][0]&~1;require(start not in _ancestry,'root adapter activation cycle')
    require(not outline_role or re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',name),'root adapter outline origin differs')
    role=outline_role or canonical_symbol(name)
    require(role in ROOT_WRAPPER_ROLES,'unreviewed raw Arguments adapter interface')
    shape=ROOT_WRAPPER_ROLES[role]
    proof=_complete_body(objects,name,instructions,role,shape,ROOT_WRAPPERS[shape],root_object,root_adapter,_ancestry)
    proof.update(requires_typed_caller_arguments_and_location=True,
                 requires_callback_handler_defaults_and_sink_closure=True,
                 reference_only=role=='_RNvNtCs_4core9panicking5panic')
    return proof
