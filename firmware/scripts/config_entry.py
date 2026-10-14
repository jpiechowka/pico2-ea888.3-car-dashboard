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
from config_roots import _window_object,root_adapter
from config_stack import canonical_symbol,typed_function
from validation import require

BOOT_POOL='_RNvNvCs_5pico214___embassy_main4POOL'
DASHBOARD_POOL='_RNvNvCs_5pico214dashboard_task4POOL'
CONFIG='_RNvNtCs_5pico211view_config13CONFIGURATION'
ISSUER='_RNvNtCs_16dashboard_config14cache_identity6ISSUER'
STATICS={BOOT_POOL:(48,'.bss'),DASHBOARD_POOL:(10840,'.bss'),CONFIG:(14604,'.data'),ISSUER:(4,'.data')}
PHASES={key:phase for phase,keys in {
    'boot_initializer':('speed2_0','sizes_0','sizez_0'),
    'dashboard_initializer':('speed2_1','sizes_1','sizez_1'),
    'run':('speed2_2','sizes_2','sizez_2'),
    'cortex_main':('speed2_3','sizes_3','sizez_3'),
    'boot_poll':('speed2_4','sizes_4','sizez_4'),
    'main':('speed2_5',),'queue_push':('sizez_6',),'raw_poll':('sizez_7',),
    'raw_spawn':('sizez_8',),'callback_install':('sizez_9',),
    'configuration_initialize':('speed2_6','sizes_6','sizez_10')}.items() for key in keys}
INITIALIZE='_RNvNtCs_5pico211view_config10initialize'


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
            proof=_complete_body(objects,name,instructions,role,shape,ENTRY_GRAMMARS[shape],entry_object,_entry_outline,_ancestry)
        except AssertionError:continue
        proof.update(requires_first_batch_source_and_task_ownership=True,
                     requires_parent_alias_lifetime_and_diagnostic_closure=True,
                     callback_scope='first detached boot batch, never subsequent executor iterations')
        results.append(proof)
    require(len(results)==1,'missing, changed or ambiguous complete entry shape')
    return results[0]


def _entry_outline(objects,name,instructions,role,ancestry):
    if role=='@outline:initialize_clrex_panic':return root_adapter(objects,name,instructions,role,ancestry)
    return entry_body(objects,name,instructions,role,ancestry)


def boot_entry(objects,instructions):
    """Compose the finite first-batch entry interface.

    Full bodies prove the reviewed local queue/claim and register effects.
    Frozen source ownership supplies the single-publisher, initially empty
    executor and detached-batch premises. Configuration memory/helper closure
    must preserve these live objects before this interface is admitted by the
    final graph. Later executor iterations receive no boot callback binding.
    """
    proofs={};external=[]

    def visit(target,role):
        names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target and
               (role.startswith('@outline:') or canonical_symbol(n)==role)]
        require(len(names)==1,'ambiguous complete entry target')
        name=names[0]
        if not role.startswith('@outline:'):
            definitions=[v for v in objects.elf.symbol_entries if canonical_symbol(v[0])==role]
            require(len(definitions)==1 and definitions[0][0]==name and definitions[0][1]==target|1,
                    'entry phase has duplicate/conflicting raw definition')
        proof=entry_body(objects,name,instructions,role if role.startswith('@outline:') else None)
        phase=PHASES[proof['shape']]
        if phase in proofs:
            require(proofs[phase]['function']==target,'distinct functions implement the same entry phase')
            return proofs[phase]
        proofs[phase]=proof
        for offset,operation,child,child_role,frame in proof['ordinary_edges']:
            if child_role in ENTRY_ROLES:visit(child,child_role)
            else:external.append((phase,offset,child,child_role,frame))
        if phase=='run':
            contract=ENTRY_GRAMMARS[proof['shape']]
            callbacks=[(proof['objects'][i],v) for i,v in enumerate(contract['objects']) if v['kind']=='entry_callback']
            require(len(callbacks)==1,'run has ambiguous boot callback producer')
            pointer,descriptor=callbacks[0];boot=visit(pointer&~1,descriptor['role'])
            require(PHASES[boot['shape']]=='boot_poll','first run callback is not the boot poll')
        return proof

    mains=[(n,e) for n,e in objects.elf.functions.items() if canonical_symbol(n)=='main']
    require(len(mains)==1,'missing or ambiguous main entry')
    visit(mains[0][1][0]&~1,'main')
    required={'main','cortex_main','run','boot_poll','boot_initializer','dashboard_initializer','configuration_initialize'}
    sizez=proofs['run']['shape']=='sizez_2'
    if sizez:required|={'raw_poll','raw_spawn','queue_push','callback_install'}
    require(set(proofs)==required,'incomplete or mixed entry phase closure')
    consumer=proofs['raw_poll' if sizez else 'run']
    require(len(consumer['indirect_sites'])==1,'entry callback consumption is missing or ambiguous')
    offset,operation,register=consumer['indirect_sites'][0]
    require(operation=='blx' and register=='r1','changed entry callback call ABI')
    require(all(not p['indirect_sites'] for phase,p in proofs.items() if phase not in ('run' if not sizez else 'raw_poll',)),
            'unbound callback in another entry phase')
    boot=proofs['boot_poll'];contract=ENTRY_GRAMMARS[boot['shape']]
    later=[(boot['objects'][i],v) for i,v in enumerate(contract['objects']) if v['kind']=='entry_callback']
    require(len(later)==2 and later[1][1]['role']=='_RNvNtCs_16embassy_executor3raw11poll_exited',
            'Dashboard/retirement callback interface differs')
    initialize=[v for v in proofs['boot_poll']['ordinary_edges'] if v[3]==INITIALIZE]
    require(len(initialize)==1 and initialize[0][4]==8,'missing synchronous configuration initialization boundary')
    chain=[proofs[p] for p in ('main','cortex_main','run')+ (('raw_poll',) if sizez else ())]
    return dict(entry_frame=sum(p['frame'] for p in chain),entry_chain=tuple((p['function'],p['frame']) for p in chain),
                boot_poll=boot['function'],boot_poll_frame=boot['frame'],initialize=initialize[0][2],initialize_frame=initialize[0][4],
                callback_edge=dict(caller=consumer['function'],offset=offset,target=boot['function'],scope='first detached boot batch'),
                later_dashboard_callback=later[0][0]&~1,retired_callback=later[1][0]&~1,
                phases=proofs,external_edges=tuple(external),requires_frozen_executor_source_ownership=True,
                requires_configuration_and_diagnostic_memory_closure=True)
