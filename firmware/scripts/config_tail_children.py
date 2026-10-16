"""Closed Tail body and ingress witnesses, separate from source/effect checks.

No structural description issues a completed aggregate or continuation.
Every dynamic adapter must bind its current source, frames and returns.
"""
import re
from config_tail_contracts import GRAMMARS,ROLES
from config_widget_children import _outline as _widget_outline,_function as _widget_function
from config_finite import _complete_body,_literal_body
from config_regions import Region,Transfer,audit_coverage,_control
from config_owner_entry import _direct_target
from config_stack import canonical_symbol
from validation import require
from config_structural import _structural


_OUTLINES={role:shape for shape,role in ROLES.items() if role.startswith('@outline:')}
_ORDINARY={role:shape for shape,role in ROLES.items() if not role.startswith('@outline:')}
ACTION_TARGET_OFFSETS=(72,138,114,126,96,150,162,132,180,108,174,90,102,156,84,120,78,144,168)


def _function(objects,address,role):
    extent,frame,name=_widget_function(objects,address,role)
    if role in _ORDINARY:
        contract=GRAMMARS[_ORDINARY[role]]
        require((extent,frame)==(contract['extent'],contract['frame']),'Tail aggregate extent/frame differs')
    return extent,frame,name


def _outline(objects,name,rows,role,ancestry=()):
    if role in _OUTLINES:return tail_body(objects,name,rows,role,ancestry)
    return _widget_outline(objects,name,rows,role,ancestry)


def tail_body(objects,name,rows,outline_role=None,ancestry=()):
    role=outline_role or canonical_symbol(name)
    if outline_role is not None:
        require(role in _OUTLINES and re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',name),
            'Tail outline borrows an ordinary or unreviewed identity')
        shape=_OUTLINES[role]
    else:
        require(role in _ORDINARY,'unreviewed Tail ordinary body');shape=_ORDINARY[role]
    def no_object(*args):require(False,'unexpected Tail child readonly object')
    proof=_complete_body(objects,name,rows,role,shape,GRAMMARS[shape],no_object,_outline,ancestry,function_consumer=_function)
    require(not proof['indirect_sites'],'Tail child contains an unreviewed executable callback')
    if shape=='ControlAction':
        start=proof['function'];table=objects.read_only(start+52,20)
        targets=tuple(start+52+2*v for v in table[:19])
        require(targets==tuple(start+off for off in ACTION_TARGET_OFFSETS) and table[19]==0 and
            all(at in rows for at in targets),'Tail ControlAction table/typed targets differ')
        proof.update(action_targets=targets,guarded_action_index_domain=(0,18),table_padding=(start+71,1),
            requires_same_source_successful_varint_argument=True)
    return proof


@_structural(2)
def tail_coverage(objects,name,rows,outline_role=None):
    """Complete child coverage or the explicitly limited parent literal bundle."""
    from config_headers import VISITOR
    if canonical_symbol(name)==VISITOR:
        require(outline_role is None,'Tail parent literals cannot borrow an outline identity')
        return _parent_literals(objects,name,rows)
    proof=tail_body(objects,name,rows,outline_role);start=proof['function'];contract=GRAMMARS[proof['shape']]
    calls={start+off:(operation,target,role,frame) for off,operation,target,role,frame in proof['ordinary_edges']}
    transfers=[]
    for off,width,text in contract['instructions']:
        at=start+off;row=rows[at];family,conditional=_control(row)
        if family is None:continue
        if at in calls:
            op,target,role,frame=calls[at]
            transfers.append(Transfer(at,row.mnemonic,'call' if family=='bl' else 'tail',target,
                'complete Tail child; actual caller source/effects remain required',role))
        elif family=='tbb':
            require(proof['shape']=='ControlAction' and off==48,'unclassified Tail computed transfer')
            transfers.append(Transfer(at,row.mnemonic,'table',proof['action_targets'],
                'nineteen guarded Action targets; same-source selector remains required'))
        elif family=='return' or family=='bx' and row.operands=='lr':
            transfers.append(Transfer(at,row.mnemonic,'conditional_return' if conditional else 'return',None,
                'actual original-save/return relation remains required'))
        else:
            require(family in ('b','cbz','cbnz'),'unclassified Tail implicit PC effect')
            target=_direct_target(row,family)
            transfers.append(Transfer(at,row.mnemonic,'loop' if target<=at else 'branch',target,
                'Tail guarded convergence; dynamic source and saved return remain required'))
    context=(start,'decoded Tail literal coverage');regions=[]
    for low,high,kind in contract['mapping']:
        if kind=='d':regions.append(Region(context,'data',start+low,start+high,(),
            ((start+low,objects.read_only(start+low,high-low)),),()))
        else:regions.append(Region(context,'helper',start+low,start+high,
            tuple(start+off for off,width,text in contract['instructions'] if low<=off<high),(),
            tuple(t for t in transfers if start+low<=t.site<start+high)))
    def child(objects,address,role):
        if not role.startswith('@outline:'):return _function(objects,address,role)
        names=[n for n,e in objects.elf.functions.items() if e[0]&~1==address]
        require(len(names)==1,'ambiguous Tail outline');p=_outline(objects,names[0],rows,role)
        return p['extent'],p['frame'],names[0]
    proof['coverage']=audit_coverage(objects,name,rows,context,regions,_function_consumer=child)
    return proof


def _parent_literals(objects,name,rows):
    """Fixed literal bundle only, explicitly not whole-Visitor coverage.

    Actual Config pointer VALUES are immutable input facts. The private
    consumer must freshly compare them with its own same-B/source relation.
    No owner or evaluated source/return state is accepted or retained here.
    """
    from config_headers import VISITOR
    from config_tail_parent_contracts import GRAMMARS as PARENTS
    offsets=(170,5630,5922,6018,6412,6450,7000,56)
    require(name in objects.elf.functions and canonical_symbol(name)==VISITOR,'unreviewed Tail parent identity')
    start=objects.elf.functions[name][0]&~1;extent,frame,_=_function(objects,start,VISITOR)
    require((extent,frame)==(7128,1608),'Tail parent literal extent/frame differs')
    def pointer_value(objects,value,descriptor):
        require(descriptor==dict(kind='same_Config') and type(value) is int and 0<=value<=0xffffffff,
            'Tail parent has another readonly literal descriptor')
        return ()
    windows=tuple(_literal_body(objects,name,rows,VISITOR,'decoded_tail_'+str(off),PARENTS['visitor_'+str(off)],
        pointer_value,_outline,window=start+off,function_consumer=_function) for off in offsets)
    return dict(kind='tail_parent_literal_windows_only',role=VISITOR,function=start,extent=extent,frame=frame,
        windows=windows,pool_values=tuple((p['function']-start,ordinal,value) for p in windows for ordinal,value in p['objects'].items()))


def tail_children(objects,rows,parents):
    """Current transitive closed children; no dynamic effects are memoized."""
    from config_helpers import HELPER_SEMANTIC_ROLES,helper_coverage
    from config_widget_children import _name,widget_body
    from config_widget_contracts import ROLES as WIDGET
    from config_logger import logger_body
    proofs={};pending=[e for p in parents for e in p['ordinary_edges']]
    while pending:
        off,operation,target,role,frame=pending.pop();key=(target,role)
        if key in proofs:continue
        if role.startswith('@outline:'):
            names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target]
            require(len(names)==1,'ambiguous Tail child outline');proof=_outline(objects,names[0],rows,role)
        elif role in _ORDINARY:proof=tail_coverage(objects,_name(objects,role,target),rows)
        elif role in HELPER_SEMANTIC_ROLES.values():
            proof=helper_coverage(objects,_name(objects,role,target),rows,(target,'decoded Tail primitive'))
        elif role in (WIDGET['color'],WIDGET['warning_colors']):proof=widget_body(objects,_name(objects,role,target),rows)
        elif role in ('__aeabi_memcpy','_RNvNtCs_17compiler_builtins3mem6memcpy'):
            proof=logger_body(objects,_name(objects,role,target),rows)
        else:require(False,'unclassified Tail child')
        require(proof['frame']==frame,'Tail child changes its current activation frame')
        proofs[key]=proof;pending.extend(proof['ordinary_edges'])
    return proofs


def tail_return_profiles():
    """Fixed current SP sites; table/source/effects are separate obligations."""
    from config_source_children import source_return_profiles
    profiles=source_return_profiles()
    profiles.update({ROLES['ControlAction']:{4,34,76,82,88,94,100,106,112,118,124,130,136,142,148,154,160,166,172,178,184},
        ROLES['ButtonBinding']:{4,74},ROLES['ScreenBindings']:{0,4,248},ROLES['LedPattern']:{0,4,170,174},
        ROLES['OUTLINED_FUNCTION_338']:set(),ROLES['OUTLINED_FUNCTION_21']:set(),
        ROLES['OUTLINED_FUNCTION_75']:{0,4},ROLES['OUTLINED_FUNCTION_155']:{0,4},
        'seq_bool':{4,14}})
    return profiles
