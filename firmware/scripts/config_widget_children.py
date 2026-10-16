"""Closed Widget scalar/Color/outline families and their owned effects.

Every effect consumes a current complete literal body and actual typed Q/D,
output, frame and protected interval. No name, fixture or returned summary
grants Widget/source/exclusive-owner or whole-graph permission.
"""
from dataclasses import replace
import re
from config_abstract import Span,SequenceInterface,decoder_interface
from config_decode_effects import _object
from config_abstract_compare import InitializedPrefix
from config_finite import _complete_body
from config_helpers import HELPER_SEMANTIC_ROLES,helper_coverage,_helper_function
from config_regions import Storage,View,borrow,require_disjoint,require_writes
from config_stack import canonical_symbol
from config_widget_contracts import GRAMMARS,ROLES
from config_decoded_views import _outline as _views_outline
from config_refill_children import child_body as _refill_body
from validation import require
from config_structural import _structural
from config_logger import logger_body

_CUSTOM={v:k for k,v in ROLES.items() if v.startswith('@outline:')}


def _name(objects,role,address=None):
    names=[n for n,e in objects.elf.functions.items() if canonical_symbol(n)==role and
        (address is None or e[0]&~1==address)]
    require(len(names)==1,'Widget child has no unique current typed role');return names[0]


def _function(objects,address,role):
    extent,frame,name=_helper_function(objects,address,role)
    if role in (ROLES['color'],ROLES['warning_colors']):
        shape='color' if role==ROLES['color'] else 'warning_colors'
        require((extent,frame)==(GRAMMARS[shape]['extent'],GRAMMARS[shape]['frame']),
            'Widget Color child extent/frame differs')
    return extent,frame,name


def _outline(objects,name,rows,role,ancestry=()):
    if role.startswith('@outline:decoded_'):return _views_outline(objects,name,rows,role,ancestry)
    if role=='@outline:advanced_copy_bands48':return _refill_body(objects,name,rows,role,ancestry)
    if role.startswith('@outline:') and role in HELPER_SEMANTIC_ROLES.values():
        return helper_coverage(objects,name,rows,(objects.elf.functions[name][0]&~1,'Widget saved return'),role)
    require(role in _CUSTOM,'Widget has an unreviewed contextual outline')
    return widget_body(objects,name,rows,role,ancestry)


@_structural(2)
def widget_body(objects,name,rows,outline_role=None,ancestry=()):
    role=outline_role or canonical_symbol(name)
    if outline_role is not None:
        require(role in _CUSTOM and re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',name),
            'Widget outline borrows an ordinary or unreviewed role')
        shape=_CUSTOM[role]
    else:
        require(role in (ROLES['color'],ROLES['warning_colors']),'unreviewed ordinary Widget adapter')
        shape='color' if role==ROLES['color'] else 'warning_colors'
    def no_object(*args):require(False,'unexpected Widget child readonly object')
    proof=_complete_body(objects,name,rows,role,shape,GRAMMARS[shape],no_object,_outline,ancestry,function_consumer=_function)
    require(not proof['indirect_sites'],'Widget adapter contains an indirect callback')
    return proof


def _current(objects,rows,proof,shape):
    require(isinstance(proof,dict) and proof.get('shape')==shape,'Widget effect uses another complete adapter')
    role=ROLES[shape];name=_name(objects,role,proof['function'])
    current=widget_body(objects,name,rows)
    require(current==proof,'Widget effect has stale child/body facts');return current


def _layout(frame,size,protected):
    protected=tuple(protected)
    require(isinstance(frame,Storage) and frame.space.startswith('stack:') and frame.writable and
        frame.alignment>=8 and frame.extent==size,'Widget adapter activation has another SP/frame')
    require_disjoint((frame,)+protected)
    return protected


def _child_frame(frame,size,label):
    return Storage(frame.space,frame.identity+':'+label,frame.start-size,size,8,frame.phase,True)


def color_interface(objects,rows,proof,state,frame,ancestor_protected):
    proof=_current(objects,rows,proof,'color');guards=_layout(frame,8,ancestor_protected)
    require(isinstance(state,SequenceInterface) and state.header.storage.phase==frame.phase and
        state.header.storage.space==frame.space,'Color has another sequence/decoder lifetime')
    q=_object(state.header,'Color Q');d=_object(state.decoder.header,'Color D')
    require_disjoint((frame,q,d))
    require_writes((state.header,),((state.header,4,4),),guards+(d,))
    saved=Storage(frame.space,frame.identity+':saved',frame.start,8,4,frame.phase,False)
    outcomes=[];children=[]
    if state.count.lower==0:
        outcomes.append(dict(kind='none',tag=0,count=Span(0,0),cursor=state.decoder.cursor,
            state=replace(state,count=Span(0,0)),possible_writes=()))
    if state.count.upper:
        edge=proof['ordinary_edges'];require(len(edge)==1 and edge[0][0]==8 and edge[0][3]==ROLES['OUTLINED_FUNCTION_259'],
            'Color loses its positive decrement/decoder transport')
        name=next((n for n,e in objects.elf.functions.items() if e[0]&~1==edge[0][2]),None)
        transport=_outline(objects,name,rows,ROLES['OUTLINED_FUNCTION_259'])
        target=transport['ordinary_edges'];role=HELPER_SEMANTIC_ROLES['deserialize_u16']
        require(len(target)==1 and target[0][3]==role,'Color tail selects another scalar decoder')
        scalar=helper_coverage(objects,_name(objects,role,target[0][2]),rows,(target[0][2],'Color u16'))
        child=decoder_interface(objects,rows,scalar,state.decoder,_child_frame(frame,24,'u16'),guards+(saved,q))
        children.append(child);count=Span(max(1,state.count.lower)-1,state.count.upper-1)
        for option in child['outcomes']:
            success=option['kind']=='success';after=replace(state,decoder=option['state'],count=count)
            outcomes.append(dict(kind='some' if success else 'error',tag=1 if success else 2,
                count=count,cursor=after.decoder.cursor,state=after,error=None if success else option['error'],
                value_domain=(0,65535) if success else None,
                child_origin=(state.decoder,tuple(scalar.get(k) for k in
                    ('function','role','semantic_role','extent','frame')),option),
                possible_writes=((state.header,4,4),)+option['possible_writes']))
    return dict(proof=proof,outcomes=tuple(outcomes),children=tuple(children),
        physical_child_paths=((8,24,16),),normal_preserves=tuple('r'+str(i) for i in range(4,12))+('sp',),
        requires_actual_Color_source_Q_D_and_original_return=True)


def warning_colors_interface(objects,rows,proof,state,output,frame,ancestor_protected):
    proof=_current(objects,rows,proof,'warning_colors');guards=_layout(frame,24,ancestor_protected)
    require(isinstance(state,SequenceInterface) and isinstance(output,View) and output.writable and output.extent==6 and
        output.start%2==0 and output.storage.alignment>=2 and output.storage.phase==frame.phase and
        state.header.storage.phase==frame.phase and output.storage.space==frame.space,
        'WarningColors loses its six-byte output/Q/D ABI')
    q=_object(state.header,'WarningColors caller Q');d=_object(state.decoder.header,'WarningColors D');out=_object(output,'WarningColors Result6')
    require_disjoint((frame,q,d,out))
    saved=Storage(frame.space,frame.identity+':saved',frame.start+8,16,4,frame.phase,False)
    protected=guards+(saved,q,d)
    require_writes((output,),((output,0,6),),protected)
    require_writes((state.header,),((state.header,4,4),),guards+(d,out))
    local=borrow(frame,0,8,frame.phase,True)
    require_writes((local,),((local,0,8),),guards+(saved,q,d,out))
    color_targets={e[2] for e in proof['ordinary_edges'] if e[3]==ROLES['color']}
    require(len(color_targets)==1,'WarningColors does not use the same Color decoder twice')
    color=widget_body(objects,_name(objects,ROLES['color'],next(iter(color_targets))),rows)
    outcomes=[];children=[]
    if state.count.lower==0:
        outcomes.append(dict(kind='none',tag=0,state=replace(state,count=Span(0,0)),cursor=state.decoder.cursor,
            count=Span(0,0),possible_writes=((output,0,2),)))
    if state.count.upper:
        count=Span(max(1,state.count.lower)-1,state.count.upper-1)
        pending=[SequenceInterface(local,state.decoder,Span(2,2))]
        for ordinal in range(2):
            following=[]
            for current in pending:
                child=color_interface(objects,rows,color,current,_child_frame(frame,8,'Color'),guards+(saved,q,out))
                children.append(child)
                for option in child['outcomes']:
                    after=replace(state,count=count,decoder=option['state'].decoder)
                    if option['kind']=='some':following.append(option['state'])
                    else:
                        error=14 if option['kind']=='none' else option['error']
                        outcomes.append(dict(kind='error',tag=2,error=error,state=after,cursor=after.decoder.cursor,
                            count=count,possible_writes=((output,0,3),),nested_origin=(ordinal,current,
                                tuple(color.get(k) for k in ('function','role','extent','frame')),option)))
            pending=following
        for complete in pending:
            require(complete.count==Span(0,0),'WarningColors omits one source color')
            after=replace(state,count=count,decoder=complete.decoder)
            outcomes.append(dict(kind='some',tag=1,value_domains=((0,65535),(0,65535)),state=after,
                count=count,cursor=after.decoder.cursor,possible_writes=((output,0,6),)))
    return dict(proof=proof,outcomes=tuple(outcomes),children=tuple(children),
        physical_child_paths=((24,8,24,16),),scratch_pop_registers=('r2','r3'),
        local_writes=((local,0,8),),caller_count_writes=((state.header,4,4),) if state.count.upper else (),
        normal_preserves=tuple('r'+str(i) for i in range(4,12))+('sp',),
        requires_actual_WarningColors_source_Q_D_and_original_return=True)


def _visitor(frame):
    require(isinstance(frame,Storage) and frame.space.startswith('stack:') and frame.writable and
        frame.alignment>=8 and frame.extent==1608,'Widget copy/clear loses its actual visitor activation')


def band_tail_clear_interface(objects,rows,target,output,length,frame,ancestor_protected):
    """Only normalized Band label[0,5), clearing exactly[length,5)."""
    _visitor(frame)
    require(isinstance(output,View) and output.storage==frame and output.writable and output.offset==1440 and output.extent==5 and
        isinstance(length,Span) and length.upper<=5,'Band clear has another normalized label/length epoch')
    proof=logger_body(objects,_name(objects,'__aeabi_memclr',target),rows)
    require(proof['frame']==16 and not proof['ordinary_edges'],'Band clear changes its complete zero-capable helper')
    guards=tuple(ancestor_protected);child=_child_frame(frame,16,'memclr')
    _layout(child,16,guards);require_disjoint((child,_object(output,'normalized label')))
    writes=((output,length.lower,5-length.lower),) if length.lower<5 else ()
    require_writes((output,),writes,guards+(Storage(child.space,child.identity+':saved',child.start,16,4,frame.phase,False),))
    return dict(proof=proof,count_domain=Span(5-length.upper,5-length.lower),possible_writes=writes,
        zero_tail_relative=(length.lower,5) if length.lower==length.upper<5 else None,
        dependent_zero_tail='same decoded length <= byte <5; never the independent interval product',
        physical_child_paths=((16,),),requires_actual_ASCII_nonempty_spaces_and_label_source=True)


def _bounded_copy(objects,rows,target,source,output,count,frame,ancestor_protected,aligned):
    role='__aeabi_memcpy4' if aligned else '__aeabi_memcpy'
    proof=logger_body(objects,_name(objects,role,target),rows)
    expected=((62,'b.w'),(160,'b.w')) if aligned else ((8,'b.w'),)
    edges=proof['ordinary_edges'];builtin='_RNvNtCs_17compiler_builtins3mem6memcpy'
    require(tuple((e[0],e[1]) for e in edges)==expected and all(e[3]==builtin and e[4]==72 for e in edges) and
        len({e[2] for e in edges})==1,'Widget copy loses its exact restored-tail alternatives')
    child=logger_body(objects,_name(objects,builtin,edges[0][2]),rows)
    require(child['frame']==72,'Widget copy loses complete builtin child frame')
    guards=tuple(ancestor_protected);source_owner=_object(source.view,'actual initialized copy prefix')
    require_disjoint((source_owner,_object(output,'actual copy output')))
    for n in (16 if aligned else 8,72):
        activation=_child_frame(frame,n,'copy'+str(n));_layout(activation,n,guards)
        require_disjoint((activation,source_owner,_object(output,'actual copy output')))
    # The guarded byte count remains paired with initialized source length.
    # This is an envelope of writes, not a claim of max-count source reads on
    # smaller alternatives. Both copies here have aligned source/destination.
    require(source.length==count,'Widget copy count is not paired with its current initialized prefix')
    writes=((output,0,count.upper),) if count.upper else ()
    require_writes((output,),writes,guards+(source_owner,))
    return dict(proof=proof,builtin=child,byte_count_domain=count,possible_writes=writes,
        source=source,physical_child_paths=((16 if aligned else 8,),(72,)),
        restored_tail_storage=True,architectural_LR_preserved=False,
        requires_actual_source_count_transport_and_original_return=True)


def band_copy_interface(objects,rows,target,source,output,count,frame,ancestor_protected):
    """The post-initializer set_bands copy, with0/16/32/48 actual bytes."""
    _visitor(frame)
    require(isinstance(source,InitializedPrefix) and source.view.storage==frame and source.view.offset==940 and
        source.view.extent==48 and isinstance(output,View) and output.storage==frame and output.writable and
        output.offset==1000 and output.extent==48 and isinstance(count,Span) and count.upper<=3,
        'Band copy loses captured source/final Advanced epoch or capacity')
    bytes_=Span(16*count.lower,16*count.upper)
    result=_bounded_copy(objects,rows,target,source,output,bytes_,frame,ancestor_protected,True)
    result.update(band_count=count,byte_count_relation='16*the same completed Band count, not arbitrary bytes',
        requires_complete_Band_source_and_retired_initializer_epoch=True)
    return result


def label_copy_interface(objects,rows,target,source,output,frame,ancestor_protected):
    """Copy1..5 validated source bytes into the separate zero-tailed label."""
    _visitor(frame)
    require(isinstance(source,InitializedPrefix) and source.view.storage==frame and source.view.offset==1188 and
        source.view.extent==5 and 1<=source.length.lower<=source.length.upper<=5 and
        isinstance(output,View) and output.storage==frame and output.writable and output.offset==1440 and output.extent==5,
        'Band label copy loses ASCII/length/source or normalized output epoch')
    result=_bounded_copy(objects,rows,target,source,output,source.length,frame,ancestor_protected,False)
    result.update(preserves_existing_tail=True,
        requires_preceding_actual_tail_clear_and_validated_same_label=True)
    return result
