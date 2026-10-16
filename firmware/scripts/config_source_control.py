"""Closed Sources structural extension of the existing private control walk.

Pending later invocations retain their exact contexts without becoming active
successors, source packages, return facts or ownership. This raw structure
still needs the actual per-entry Source/aggregate guards and reached uses.
"""
from dataclasses import dataclass
from config_decoded_views import _ControlSelection,_control_selection,_selected_successors,_foreign_ingress
from config_source_children import CHANNEL_SOURCE,ACCESS_TARGET_OFFSETS,source_children,source_return_profiles
from validation import require


# Reviewed current predecessor contexts, normalized to the same typed parent.
# The BL identity is rebound through the current complete seq_u16 proof.
SOURCE_PENDING={
 6156:((6144,4,'bl @pending:seq_u16'),(6148,2,'mov r4, r0'),(6150,2,'lsrs r6, r0, #0x10'),(6152,2,'uxth r0, r0'),(6154,2,'cmp r0, #0x2'),(6156,4,'beq.w @parent+1a00')),
 6186:((6170,4,'bl @pending:seq_u16'),(6174,2,'lsrs r5, r0, #0x10'),(6176,2,'uxth r1, r0'),(6178,2,'cmp r1, #0x2'),(6180,4,'bne.w @parent+1932'),(6184,2,'mov r6, r5'),(6186,2,'b @parent+1a00')),
 6476:((6460,4,'bl @pending:seq_u16'),(6464,2,'uxth r1, r0'),(6466,4,'lsr.w r8, r0, #0x10'),(6470,2,'cmp r1, #0x2'),(6472,2,'bne @parent+197c'),(6474,2,'mov r6, r8'),(6476,2,'b @parent+1a00')),
 6550:((6534,4,'bl @pending:seq_u16'),(6538,2,'uxth r1, r0'),(6540,4,'lsr.w r9, r0, #0x10'),(6544,2,'cmp r1, #0x2'),(6546,2,'bne @parent+1998'),(6548,2,'mov r6, r9'),(6550,2,'b @parent+1a00')),
 6578:((6562,4,'bl @pending:seq_u16'),(6566,2,'uxth r1, r0'),(6568,4,'lsr.w r10, r0, #0x10'),(6572,2,'cmp r1, #0x2'),(6574,2,'bne @parent+19b4'),(6576,2,'mov r6, r10'),(6578,2,'b @parent+1a00')),
 6606:((6590,4,'bl @pending:seq_u16'),(6594,2,'uxth r1, r0'),(6596,4,'lsr.w r11, r0, #0x10'),(6600,2,'cmp r1, #0x2'),(6602,2,'bne @parent+19d0'),(6604,2,'mov r6, r11'),(6606,2,'b @parent+1a00')),
 6632:((6616,4,'bl @pending:seq_u16'),(6620,2,'lsrs r1, r0, #0x10'),(6622,2,'str r1, [sp, #0x11c]'),(6624,2,'uxth r1, r0'),(6626,2,'cmp r1, #0x2'),(6628,2,'bne @parent+19ea'),(6630,2,'ldr r6, [sp, #0x11c]'),(6632,2,'b @parent+1a00')),
 6856:((6854,2,'movs r6, #0xe'),(6856,2,'b @parent+1a00')),
}
SOURCE_FALLTHROUGH={
 4408:dict(target=4410,context=((4402,2,'movs r0, #0x2'),(4404,2,'str r0, [sp, #0x11c]'),(4406,2,'movs r0, #0x0'),(4408,2,'str r0, [sp, #0x118]'))),
 6654:dict(target=6656,context=((6642,4,'bl @pending:seq_u16'),(6646,2,'lsrs r1, r0, #0x10'),(6648,2,'uxth r2, r0'),(6650,2,'cmp r2, #0x2'),(6652,2,'bne @parent+1a06'),(6654,2,'mov r6, r1'))),
}


@dataclass(frozen=True)
class _SourceSelection(_ControlSelection):
    widgets:object
    Source_parents:tuple
    Source_children:dict
    child_contexts:tuple


@dataclass(frozen=True)
class _SourceStructuralControl:
    selection:_SourceSelection
    raw_successors:tuple
    raw_returns:tuple
    Source_returns:tuple
    carrier_returns:dict
    nested_returns:tuple
    outside_shared_contexts:tuple
    unentered_outgoing:tuple
    confers_active_state:bool=False

    def __post_init__(self):
        require(type(self.selection) is _SourceSelection and self.confers_active_state is False,
            'raw Sources structure cannot grant active execution')


def _pending_profile(selection):
    require(type(selection) is _SourceSelection,'unreviewed Sources control selection')
    require(selection.context is selection.widgets.views.context and selection.widget,
        'Sources selected parent leaves its actual Widget predecessor')
    return ({site:dict(target=6656,operation=rows[-1][2].split(' ',1)[0],context=rows)
        for site,rows in SOURCE_PENDING.items()},SOURCE_FALLTHROUGH)


def _source_table_targets(selection,at,row):
    require(type(selection) is _SourceSelection,'Views selected invocation has an unresolved PC effect')
    proofs=tuple(p for p in selection.Source_children.values() if p['role']==CHANNEL_SOURCE)
    require(len(proofs)==1,'Sources table lacks its current complete child')
    proof=proofs[0];start=proof['function'];targets=tuple(start+off for off in ACCESS_TARGET_OFFSETS)
    require(at==start+74 and row.mnemonic=='tbb' and row.operands=='[pc, r6]' and
        proof['shape']=='ChannelSource' and proof['access_targets']==targets and
        proof['guarded_access_index_domain']==(0,5) and all(v in selection.selected for v in targets),
        'Sources structural table loses its exact guarded current targets')
    # This is only fixed structure. Actual source-successful selector and its
    # dependent arm/return/effect pairing remain required by active control.
    return targets


def _source_structural_control(objects,rows,widgets,parents,children):
    from config_decoded_widgets import _WidgetsState
    from config_decoded_sources import _parents
    from config_finite_returns import _closed_child_returns
    from config_widget_contracts import ROLES as WIDGET
    require(type(widgets) is _WidgetsState,'Sources structure lacks the actual private predecessor')
    bridge=widgets.family_bridge.current();c=widgets.views.context;V=c.headers['visitor']['proof']['function']
    require(objects is bridge.objects and rows is bridge.rows and
        parents==_parents(objects,rows,V,c.owner.start) and children==source_children(objects,rows,parents),
        'Sources structure changes its current parent/child input pair')
    combined=dict(widgets.views.children)
    lineage=[]
    for family in (widgets.children,children):
        for key,p in family.items():
            if key in combined:
                prior=combined[key]
                require({k:v for k,v in prior.items() if k!='coverage'}=={k:v for k,v in p.items() if k!='coverage'},
                    'Sources shared child has inconsistent current bodies')
                a=prior.get('coverage');b=p.get('coverage')
                require(a==b or isinstance(a,dict) and isinstance(b,dict) and
                    {k:v for k,v in a.items() if k!='context'}=={k:v for k,v in b.items() if k!='context'},
                    'Sources shared child has inconsistent current coverage')
                lineage.append((key,prior,p))
            combined[key]=p
    all_parents=widgets.views.parents+widgets.parents+parents
    base=_control_selection(objects,rows,c,all_parents,combined)
    selection=_SourceSelection(base.context,base.children,base.proofs,base.selected,base.edges,
        frozenset((base.cuts-{V+4376})|{V+5630}),base.widget,widgets,parents,children,tuple(lineage))
    successors,returns,dormant=_selected_successors(selection)
    nested=_closed_child_returns(rows,parents,children,c.headers['visitor_frame'].start,source_return_profiles(),
        ('seq_f32',WIDGET['warning_colors']))
    require(nested and min(v['lowest_SP'] for v in nested)==c.headers['visitor_frame'].start-120,
        'Sources structure loses its current child save/return geometry')
    carriers=_source_carrier_returns(rows,parents,children,nested,c.headers['visitor_frame'].start)
    pending=_foreign_ingress(objects,rows,selection)
    return _SourceStructuralControl(selection,successors,returns,
        nested,carriers,widgets.structural.nested_returns+nested,pending,dormant)


def _source_carrier_returns(rows,parents,children,returns,sp):
    """Actual per-call transfer, conditional on separate protected save slots.

    Complete current body matching and the existing finite saved-PC walk are
    prerequisites. This does not initialize a carrier, choose a source arm,
    or establish a pointer's parent lifetime from the callee ABI alone.
    """
    from config_decoded_widgets import _source_fp_preservation
    high=tuple('r'+str(i) for i in range(4,12));checked=[]
    for parent in parents:
        for off,operation,target,role,frame in parent['ordinary_edges']:
            pc=parent['function']+off+4
            matches=tuple(v for v in returns if v['function']==target and v['role']==role and
                v['entry_SP']==sp and v['return_PC']==pc and v['return_SP']==sp)
            require(operation=='bl' and len(matches)==1 and matches[0]['preserved_high_gprs']==high,
                'Sources child changes an actual live high GPR carrier')
            checked.append((pc-4,target,role,high))
    require(len(checked)==57 and len({v[0] for v in checked})==57,
        'Sources carrier graph omits a current parent call')
    return dict(parent_calls=tuple(checked),high_fp=_source_fp_preservation(rows,children),
        architectural_LR_preserved=False,requires_actual_protected_save_slots_and_parent_lifetimes=True)


def _structural_data(value,widgets,parents,children):
    """Complete immutable-checkpoint input, excluding only checked identities."""
    require(type(value) is _SourceStructuralControl and value.selection.widgets is widgets and
        value.selection.context is widgets.views.context and value.selection.Source_parents is parents and
        value.selection.Source_children is children and value.confers_active_state is False,
        'Sources raw structure lost its same private context/parent/child association')
    s=value.selection
    return dict(selection=(s.proofs,s.selected,s.edges,tuple(sorted(s.cuts)),s.widget,s.children,s.child_contexts),
        raw_successors=value.raw_successors,raw_returns=value.raw_returns,Source_returns=value.Source_returns,
        carrier_returns=value.carrier_returns,
        nested_returns=value.nested_returns,outside_shared_contexts=value.outside_shared_contexts,
        unentered_outgoing=value.unentered_outgoing,confers_active_state=value.confers_active_state)
