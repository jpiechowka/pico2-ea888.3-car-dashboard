"""Complete raw Arguments adapters; caller roots and formatting closure remain."""
import re
import struct
import hashlib
from config_root_contracts import ROOT_WRAPPERS,ROOT_WRAPPER_ROLES
from config_finite import _complete_body
from config_stack import canonical_symbol,typed_function
from validation import require

LOCATION_ROLES = {
    'spawn_core0': ('pico2/src/main.rs',155,1),
    'spawn_dashboard': ('pico2/src/main.rs',161,58),
    'initialize_taken': ('static_cell-2.1.1/src/lib.rs',188,13),
    'view_capacity': ('crates/dashboard-config/src/defaults.rs',118,41),
    'label_capacity': ('crates/dashboard-config/src/defaults.rs',25,43),
    'warning_capacity': ('crates/dashboard-config/src/defaults.rs',52,49),
    'widget_count': ('crates/dashboard-config/src/defaults.rs',152,13),
    'reviewed_defaults': ('crates/dashboard-config/src/slot.rs',61,63),
    'identity_first': ('crates/dashboard-config/src/cache_identity.rs',29,67),
    'first_invalid': ('crates/dashboard-config/src/validate.rs',398,24),
    'views_prefix': ('crates/dashboard-config/src/validate.rs',482,24),
    'widgets_prefix': ('crates/dashboard-config/src/validate.rs',514,26),
    'occupied_view': ('crates/dashboard-config/src/validate.rs',528,16),
    'prior_band': ('crates/dashboard-config/src/validate.rs',594,29),
    'current_band': ('crates/dashboard-config/src/validate.rs',595,20),
    'band_label_slice': ('crates/dashboard-config/src/advanced.rs',102,47),
    'band_label_utf8': ('crates/dashboard-config/src/advanced.rs',102,80),
    'bands_slice': ('crates/dashboard-config/src/advanced.rs',194,49),
}


def root_location(objects,address,role):
    """Typed readonly Location, with portable host-prefix provenance.

    Only the actual checkout/registry prefix and slash spelling may differ.
    The source suffix, line and column are closed roles. Return exact current
    filename bytes and hashes; never use a remembered Windows prefix as a key.
    Caller instruction/source binding remains a separate required proof.
    """
    require(role in LOCATION_ROLES and address%4==0,'unreviewed or unaligned root Location')
    suffix,expected_line,expected_column=LOCATION_ROLES[role]
    raw=objects.read_only(address,16);pointer,length,line,column=struct.unpack('<4I',raw)
    require((line,column)==(expected_line,expected_column) and length>=len(suffix),'root Location line/column/filename extent differs')
    filename=objects.read_only(pointer,length)
    try:path=filename.decode('utf-8').replace('\\','/')
    except UnicodeDecodeError:require(False,'root Location filename is not UTF-8')
    require('\0' not in path and not any(v in ('.','..') for v in path.split('/')) and
            (path==suffix or path.endswith('/'+suffix)),'root Location source role/path differs')
    return dict(role=role,address=address,filename_address=pointer,filename_bytes=filename,
                filename_sha256=hashlib.sha256(filename).hexdigest(),location_sha256=hashlib.sha256(raw).hexdigest(),
                source_suffix=suffix,line=line,column=column,requires_caller_source_binding=True)


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
