"""Complete pinned logger/builtin bodies and typed state objects.

Object suffixes .0 through .4 are physical state fields, never clone labels.
The immutable Rust NAME deliberately occupies writable ELF .data. Preserve its
actual section policy. Header initialization does not prove runtime stability;
full emitted stores, source ownership, entry and panic guard proof are required.
"""
import json
import re
import struct
from config_panic import panic_object,PANIC_CALLBACKS
from config_roots import root_location
from config_stack import DEFMT_IDENTITIES,canonical_symbol,typed_function,_typed_function_extent,transfer_family
from config_finite import _complete_body
from config_logger_contracts import LOGGER_GRAMMARS,LOGGER_ROLES,MANUAL_BUILTINS
from validation import require
from config_structural import _structural

ENCODER='_RNvCs_9defmt_rtt11RTT_ENCODER.'
LOCK_OWNER='_RNvNtCs_10embassy_rp21critical_section_impl10LOCK_OWNER.0'
ANCHOR='_RNvNtNtCs_5pico25tasks8recovery15DISPLAY_PENDING.0'
NAME='_RNvCs_9defmt_rtt4NAME'
BUFFER='_RNvCs_9defmt_rtt6BUFFER'

TIMESTAMP_ALIASES={'_defmt_timestamp':0,'__defmt_default_timestamp':6,'DefaultPreInit':6}
PANIC_ALIASES=('_defmt_panic','__defmt_default_panic')


def _logger_function(objects,target,role):
    """Only reviewed zero-size aliases and complete assembly have exceptions."""
    if role in TIMESTAMP_ALIASES or role=='@logger_timestamp':
        expected=TIMESTAMP_ALIASES;extent=6;semantic='@logger_timestamp'
    elif role in PANIC_ALIASES or role=='@logger_default_panic':
        aliases=[e for n,e in objects.elf.functions.items() if n=='__defmt_default_panic' and e[0]&~1==target]
        require(len(aliases)==1 and aliases[0][1] in (24,26),'default defmt panic alias differs')
        extent=aliases[0][1];expected={'_defmt_panic':0,'__defmt_default_panic':extent};semantic='@logger_default_panic'
    else:
        if role not in MANUAL_BUILTINS:return typed_function(objects,target,role)
        extent,name=_typed_function_extent(objects,target,role);expected_extent,frame=MANUAL_BUILTINS[role]
        definitions=[v for v in objects.elf.symbol_entries if canonical_symbol(v[0])==role]
        require(definitions==[(name,target|1,extent,2,objects.elf.functions[name][2])],
                'reviewed builtin definition is missing, conflicting or not local FUNC')
        require(extent==expected_extent and (target not in objects.sizes or objects.sizes[target]==frame),
                'reviewed builtin extent/current frame differs')
        return extent,frame,name
    entries=[v for v in objects.elf.symbol_entries if v[0] in expected or v[1]&~1==target and v[3]&15==2]
    require(len(entries)==len(expected) and {v[0] for v in entries}==set(expected),'missing/duplicate/unreviewed logger function alias')
    index=entries[0][4]
    require(all(v== (v[0],target|1,expected[v[0]],18,index) for v in entries) and 0<=index<len(objects.elf.sections),
            'logger alias extent/Thumb/global type/section differs')
    actual={n:e for n,e in objects.elf.functions.items() if e[0]&~1==target}
    require(actual=={n:(target|1,size,index) for n,size in expected.items() if size},'logger sized alias view differs')
    section=objects.elf.sections[index]
    require(section[1]==1 and section[2]&7==6 and section[3]<=target<target+extent<=section[3]+section[5],
            'logger alias exceeds declared read-only executable section')
    objects.read_only(target,extent);require(objects.sizes.get(target)==8,'logger alias current frame differs')
    return extent,8,semantic


@_structural(2)
def logger_body(objects,name,instructions,outline_role=None,_ancestry=()):
    """Complete finite bodies; every caller/state/default/guard edge stays open."""
    require(name in objects.elf.functions,'missing logger body')
    start=objects.elf.functions[name][0]&~1;require(start not in _ancestry,'logger activation cycle')
    raw_role=canonical_symbol(name)
    role=outline_role or ('@logger_timestamp' if raw_role in TIMESTAMP_ALIASES else
                         '@logger_default_panic' if raw_role in PANIC_ALIASES else raw_role)
    require(not outline_role or re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',name),'logger outline origin differs')
    require(role in LOGGER_ROLES,'unreviewed exact logger/builtin role');results=[]
    for shape in LOGGER_ROLES[role]:
        try:
            proof=_complete_body(objects,name,instructions,role,shape,LOGGER_GRAMMARS[shape],logger_object,logger_body,
                                 _ancestry,function_consumer=_logger_function)
        except AssertionError:continue
        rows=[r for a,r in sorted(instructions.items()) if start<=a<start+proof['extent']]
        proof.update(transfers=tuple((r.address-start,r.mnemonic,r.operands.split('@',1)[0].strip()) for r in rows
                         if transfer_family(r.mnemonic.removesuffix('.w')) or re.search(r'\bpc\b',r.operands)),
                     requires_runtime_ownership_entry_and_guard_closure=True,
                     requires_typed_inputs_live_sink_and_default_options=True,
                     manual_frame_requires_complete_builtin=role if role in MANUAL_BUILTINS else None,
                     reference_only=role=='@logger_default_panic')
        results.append(proof)
    require(len(results)==1,'missing, changed or ambiguous complete logger/builtin shape')
    return results[0]


def _object_symbol(objects,address,role,size,section_name):
    # Preserve the numeric object ordinal. canonical_symbol strips function
    # clone suffixes and is deliberately not used for these object identities.
    require(isinstance(address,int) and 0<=address<address+size<=0x100000000,'logger object exceeds 32-bit address space')
    entries=[v for v in objects.elf.symbol_entries if re.sub(r'Cs[A-Za-z0-9]+_','Cs_',v[0])==role]
    require(len(entries)==1,'missing or ambiguous logger object identity')
    _,value,extent,info,index=entries[0];expected_info=17 if role=='_SEGGER_RTT' else 1
    require(value==address and extent==size and info==expected_info and 0<=index<len(objects.elf.sections),
            'logger object pointer/size/binding/type differs')
    section=objects.elf.sections[index]
    require(section==objects.elf.named.get(section_name) and section[1]==(1 if section_name=='.data' else 8) and
            section[2]&7==(7 if section_name=='.data' else 3) and
            0<=section[3]<section[3]+section[5]<=0x100000000 and
            section[3]<=address<address+size<=section[3]+section[5],
            'logger object declared allocation/section differs')
    backing=[]
    for i,other in enumerate(objects.elf.sections):
        if not other[2]&2 or not other[5] or not (other[3]<address+size and address<other[3]+other[5]):continue
        require(0<=other[3]<other[3]+other[5]<=0x100000000,'intersecting logger allocation exceeds 32-bit extent')
        backing.append(i)
    require(backing==[index],'logger object allocation is not uniquely owned')
    for other in objects.elf.symbol_entries:
        if other==entries[0] or other[3]&15!=1 or not other[2] or not 0<=other[4]<len(objects.elf.sections):continue
        if objects.elf.sections[other[4]][2]&2:
            require(not (other[1]<address+size and address<other[1]+other[2]),'distinct allocated OBJECT intersects logger storage')
    return section


def _initialized(objects,address,size,role):
    section=_object_symbol(objects,address,role,size,'.data');offset=section[4]+address-section[3]
    raw=objects.elf.data[offset:offset+size]
    require(len(raw)==size and section[4]+section[5]<=len(objects.elf.data),'truncated logger initialized object')
    return raw


def logger_object(objects,address,descriptor):
    """Bind one producer-selected object. No nearby-object or role union."""
    kind=descriptor['kind']
    if kind=='defmt_metadata_id':
        require(descriptor['raw_names'] and all(json.loads(n)==DEFMT_IDENTITIES['usize_encoding_id']
                for n in descriptor['raw_names']),'unreviewed logger interned identity')
        objects.defmt_id(address,'usize_encoding_id');return ()
    if kind=='fmt_write_vtable':
        require(descriptor['layout']==[0,0,1] and descriptor['slots']==[[PANIC_CALLBACKS[k][0]] for k in ('sink_str','sink_char','sink_fmt')],
                'unreviewed logger Write table interface')
        return panic_object(objects,address,'sink_vtable')
    if kind=='literal':
        raw=bytes.fromhex(descriptor['hex'])
        require(len(raw)==descriptor['length'] and objects.read_only(address,len(raw))==raw,'logger literal bytes/length differ')
        return ()
    if kind=='location':
        roles={(173,13):'logger_acquire',(226,13):'logger_release',(385,5):'defmt_default_panic'}
        role=roles.get((descriptor['line'],descriptor['column']));require(role is not None,'unreviewed logger source Location')
        root_location(objects,address,role);return ()
    if kind=='writable_symbol':
        require(len(descriptor['roles'])==1,'ambiguous logger state role');role=descriptor['roles'][0]
        require(role==LOCK_OWNER or role in {ENCODER+str(i) for i in range(5)},'unreviewed logger mutable state')
        _object_symbol(objects,address,role,1,'.bss');return ()
    if kind=='split_state_anchor':
        require(descriptor['anchor_is_encoder'] is False and descriptor['offsets']==dict(taken=1,cs_restore=2,started=3,run=4,zeros=5) and
                descriptor['roles']==[[ENCODER+str(i)] for i in range(5)],'unreviewed logger split state layout')
        _object_symbol(objects,address,ANCHOR,1,'.bss')
        for i in range(5):_object_symbol(objects,address+i+1,ENCODER+str(i),1,'.bss')
        return ()
    require(kind=='rtt_header' and descriptor['roles']==['_SEGGER_RTT'] and descriptor['extent']==48 and address%4==0 and
            descriptor['name_bytes']=='6465666d7400' and descriptor['buffer_extent']==1024 and
            descriptor['fixed_words']=={'4':1,'5':0,'8':1024,'9':0,'10':0,'11':1},'unreviewed RTT header layout')
    raw=_initialized(objects,address,48,'_SEGGER_RTT');words=struct.unpack('<12I',raw)
    require(raw[:16]==b'SEGGER RTT\0\0\0\0\0\0' and all(words[int(k)]==v for k,v in descriptor['fixed_words'].items()),
            'RTT signature/initial cursor/flags/size differs')
    require(_initialized(objects,words[6],6,NAME)==b'defmt\0','RTT typed writable NAME differs')
    _object_symbol(objects,words[7],BUFFER,1024,'.uninit')
    return ()
