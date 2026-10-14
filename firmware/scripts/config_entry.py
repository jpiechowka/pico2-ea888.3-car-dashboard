"""Complete startup bodies and producer-selected task storage.

The callback store/consume relationship applies to the first detached boot
batch only. These bodies retain every claim retry, publication and retirement
path; caller memory, source ownership and diagnostic closure remain required.
No callback target is authorized merely because it occurs elsewhere in an ELF.
"""
import re
from config_entry_contracts import ENTRY_GRAMMARS,ENTRY_ROLES
from config_finite import _complete_body
from config_logger import _object_symbol,_initialized
from config_roots import _window_object
from config_stack import canonical_symbol,typed_function
from validation import require

BOOT_POOL='_RNvNvCs_5pico214___embassy_main4POOL'
DASHBOARD_POOL='_RNvNvCs_5pico214dashboard_task4POOL'
CONFIG='_RNvNtCs_5pico211view_config13CONFIGURATION'
ISSUER='_RNvNtCs_16dashboard_config14cache_identity6ISSUER'
STATICS={BOOT_POOL:(48,'.bss'),DASHBOARD_POOL:(10840,'.bss'),CONFIG:(14604,'.data'),ISSUER:(4,'.data')}


def static_role(name):
    return re.sub(r'Cs[A-Za-z0-9]+_','Cs_',name)


def entry_object(objects,address,descriptor):
    kind=descriptor['kind']
    if kind=='entry_static':
        role=descriptor['role'];require(role in STATICS and address%4==0,'unreviewed or unaligned entry static')
        extent,section=STATICS[role];_object_symbol(objects,address,role,extent,section)
        if role==CONFIG:
            raw=_initialized(objects,address,extent,role)
            require(raw[14600]==0,'Config ConstStaticCell is initially taken')
        elif role==ISSUER:require(_initialized(objects,address,extent,role)==b'\x01\0\0\0','Issuer initialization differs')
        return ()
    if kind=='entry_callback':
        # Descriptor interfaces are fixed in the complete producer grammars.
        allowed={(v['role'],v['extent'],v['frame']) for g in ENTRY_GRAMMARS.values() for v in g['objects'] if v['kind']==kind}
        require((descriptor['role'],descriptor['extent'],descriptor['frame']) in allowed and address&1,
                'unreviewed entry callback interface or non-Thumb pointer')
        extent,frame,_=typed_function(objects,address&~1,descriptor['role'])
        require((extent,frame)==(descriptor['extent'],descriptor['frame']),'entry callback extent/frame differs')
        return ((address&~1,descriptor['role'],extent,frame),)
    return _window_object(objects,address,descriptor)


def entry_body(objects,name,instructions,outline_role=None,_ancestry=()):
    """Bind the entire emitted entry body; do not grant later-batch authority."""
    require(name in objects.elf.functions,'missing entry body')
    start=objects.elf.functions[name][0]&~1;require(start not in _ancestry,'entry outline activation cycle')
    role=outline_role or canonical_symbol(name)
    require(not outline_role or re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',name),'entry outline origin differs')
    require(role in ENTRY_ROLES,'unreviewed exact entry role');results=[]
    for shape in ENTRY_ROLES[role]:
        try:
            proof=_complete_body(objects,name,instructions,role,shape,ENTRY_GRAMMARS[shape],entry_object,entry_body,_ancestry)
        except AssertionError:continue
        proof.update(requires_first_batch_source_and_task_ownership=True,
                     requires_parent_alias_lifetime_and_diagnostic_closure=True,
                     callback_scope='first detached boot batch, never subsequent executor iterations')
        results.append(proof)
    require(len(results)==1,'missing, changed or ambiguous complete entry shape')
    return results[0]
