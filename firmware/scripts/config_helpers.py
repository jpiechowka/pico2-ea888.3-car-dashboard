"""Complete small helper bodies; source/owned effects and callers stay required.

Stable literal contracts retain every loop, save/return path and status flag.
Typed target metadata is a boundary requirement, never its transitive proof.
Specialized varint input, initialized Result fields and actual target alignment
policy must be established by contextual composition before gate admission.
"""
import re
from config_finite import _complete_body
from config_helper_contracts import HELPER_GRAMMARS,HELPER_ROLES
from config_logger import _logger_function
from config_roots import root_location
from config_stack import canonical_symbol,transfer_family
from validation import require


def _helper_function(objects,address,role):
    extent,frame,name=_logger_function(objects,address,role)
    if role in HELPER_ROLES:
        expected=HELPER_GRAMMARS[HELPER_ROLES[role]]
        require((extent,frame)==(expected['extent'],expected['frame']),'helper child current extent/frame differs')
    return extent,frame,name


def _helper_object(objects,address,descriptor):
    require(descriptor['kind']=='source_location' and descriptor['role'] in
            ('postcard_serialize_u16','postcard_varint_usize','postcard_varint_u32_small'),
            'unreviewed helper readonly object')
    root_location(objects,address,descriptor['role']);return ()


def helper_body(objects,name,instructions,outline_role=None,_ancestry=()):
    require(name in objects.elf.functions,'missing small helper body')
    start=objects.elf.functions[name][0]&~1;require(start not in _ancestry,'small helper outline cycle')
    role=outline_role or canonical_symbol(name)
    require(not outline_role or re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',name),'small helper outline origin differs')
    require(role in HELPER_ROLES,'unreviewed exact small helper receiver/outline')
    semantic=HELPER_ROLES[role]
    proof=_complete_body(objects,name,instructions,role,semantic,HELPER_GRAMMARS[semantic],
                         _helper_object,helper_body,_ancestry,function_consumer=_helper_function)
    proof.update(semantic_role=semantic,
                 transfers=tuple((r.address-start,r.mnemonic,r.operands.split('@',1)[0].strip())
                     for a,r in sorted(instructions.items()) if start<=a<start+proof['extent'] and
                     (transfer_family(r.mnemonic.removesuffix('.w')) or re.search(r'\bpc\b',r.operands))),
                 requires_source_inputs_owned_effects_and_child_closure=True,
                 requires_parent_frame_and_abi=semantic if outline_role else None,
                 requires_u32_domain_maximum=127 if semantic=='varint_u32_small' else None,
                 requires_hard_float_s0=semantic=='serialize_f32')
    return proof
