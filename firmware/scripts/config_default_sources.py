"""Grouped default source-array/terminal phase; validator is an unresolved edge.

Matched source constructors and owned effects retain abstract inactive payload.
This does not establish actual incoming ownership, validation or a graph pass.
"""
import re
from config_default import _ingress
from config_default_widget import widget_loop,_store
from config_entry import entry_object
from config_finite import _literal_body
from config_headers import ROOT
from config_helpers import _helper_function
from config_refill_children import _readonly,child_coverage,definition_interface,period_interface,opaque_copy_interface
from config_regions import Storage,Argument,Region,Transfer,borrow,_control
from config_source_contract import SOURCE_CONTRACT,SOURCE_CASES,SOURCE_WRITES
from config_stack import canonical_symbol,transfer_family
from validation import require


_LED=bytes.fromhex('0100e007640090013200020000f8640090013200010000000000')
_LED_PADDING=(1,5,9,11,15,19,25)


def _object(objects,value,descriptor):
    kind=descriptor['kind']
    if kind=='entry_static':return entry_object(objects,value,descriptor)
    if kind=='source_literal':require(value==descriptor['value'],'source ID/control literal differs')
    elif kind=='source_channels':require(_readonly(objects,value,8,1)==bytes(range(8)),'source Channel_ALL pairing differs')
    else:
        require(kind=='source_led_default','unreviewed source constant role');raw=_readonly(objects,value,26,4)
        require(all(raw[i]==_LED[i] for i in range(26) if i not in _LED_PADDING),'source LED semantic fields differ')
    return ()


def _no_outline(*args):raise AssertionError('source phase has no outline child')


def _coverage(objects,name,instructions,start,proof):
    end=start+342;rows={a:r for a,r in instructions.items() if start<=a<end};edges={start+v[0]:v for v in proof['ordinary_edges']};transfers=[]
    require(len(edges)==4 and not proof['indirect_sites'],'source children/callback inventory differs')
    for at,row in rows.items():
        kind,conditional=_control(row);stops=False
        if kind in ('b','bl','cbz','cbnz'):
            match=re.search(r'(?<![#\w])0x([0-9a-f]+)(?:\s|$)',row.operands.split('@',1)[0]);require(match is not None,'source transfer is unresolved');target=int(match[1],16)
            if kind=='bl':
                require(at in edges,'source ordinary transfer lacks current binding');edge=edges[at]
                transfers.append(Transfer(at,row.mnemonic,'call',target,'current source child or unresolved validator handoff',edge[3]))
            else:
                require(target in rows,'source branch escapes its matched phase')
                transfers.append(Transfer(at,row.mnemonic,'loop' if target<=at else 'branch',target,'paired source index/phase'));stops=kind=='b' and not conditional
        else:require(kind is None,'source phase has unreviewed return/computed control')
        if not stops:require(at+len(row.raw) in rows or at+len(row.raw)==end and at in edges,'source fallthrough escapes or enters data')
    return Region((objects.elf.functions[name][0]&~1,'default sources/terminal'),'loop',start,end,tuple(sorted(rows)),(),tuple(transfers))


def _child(objects,instructions,proof,offset,shape):
    edge=next(e for e in proof['ordinary_edges'] if e[0]==offset)
    names=[n for n,e in objects.elf.functions.items() if e[0]&~1==edge[2] and canonical_symbol(n)==edge[3]]
    require(len(names)==1,'source child loses its exact typed role');bound=child_coverage(objects,names[0],instructions,(edge[2],'default source child',proof['function']+offset))
    require(bound['shape']==shape,'source child has another ABI');return bound


def source_terminal(objects,name,instructions,frame,ancestor_protected):
    ancestor_protected=tuple(ancestor_protected);widget=widget_loop(objects,name,instructions,frame,ancestor_protected)
    start=widget['continuation'];end=start+342;phase=frame.phase;base=widget['prepared']['config'];protected=widget['prepared']['protected']
    proof=_literal_body(objects,name,instructions,ROOT,'default sources and terminal',SOURCE_CONTRACT,_object,_no_outline,
        window=start,function_consumer=_helper_function)
    require(proof['objects'][3]==base.start,'source loop reload changes its private Config owner')
    _ingress(instructions,objects.elf.functions[name][0]&~1,objects.elf.functions[name][1],start,end)
    for at,row in instructions.items():
        if start<=at<end:continue
        if transfer_family(row.mnemonic.removesuffix('.w')) in ('b','bl','cbz','cbnz'):
            target=re.search(r'(?<![#\w])0x([0-9a-f]+)(?:\s|$)',row.operands.split('@',1)[0])
            require(target is None or int(target[1],16)!=start or at==widget['proof']['function']+2,
                'foreign source entry bypasses complete Widget handoff')
    region=_coverage(objects,name,instructions,start,proof)
    definition=_child(objects,instructions,proof,52,'catalog_definition_z');period=_child(objects,instructions,proof,94,'minimum_period_z')
    copies=[e for e in proof['ordinary_edges'] if e[0]==176 and e[3]=='__aeabi_memcpy'];require(len(copies)==1,'source copy lacks its complete ordinary identity')
    # Widget scratch owners retire before these new ID/pointer/transport epochs.
    cells=tuple(Storage(frame.space,label,frame.start+o,n,4,phase,False) for label,o,n in
        (('source pointer cells',52,12),('DefinitionId array',244,8)))
    live=protected+cells
    local_writes=tuple(_store(borrow(frame,o,n,phase,writable=True),0,n,protected) for site,owner,o,n,field in SOURCE_WRITES if owner=='P')
    iterations=[];writes=[]
    for i,case in enumerate(SOURCE_CASES):
        require((case['i'],case['channel'],case['offset'],case['source_base'])==(i,i,22*i,32+22*i),'source pairing/stride differs')
        output=borrow(frame,112,36,phase,writable=True)
        value=definition_interface(objects,instructions,definition,Argument('DefinitionId',None,0,8,frozenset((case['id'],))),output,
            Storage(frame.space,'source Definition',frame.start-52,52,4,phase,True),live)
        require(len(value['outcomes'])==1,'source Definition has an unpaired result');typed=value['outcomes'][0]
        require((typed['variant'],typed['channel'],typed['access']['variant'])==(case['definition'],i,case['access'][0]),'source catalogue result/constructor pair differs')
        timing=period_interface(objects,instructions,period,Argument('Channel',None,0,8,frozenset((i,))),
            Storage(frame.space,'source period',frame.start-8,8,8,phase,True),live)
        enabled=typed['unavailable'] is None and case['id']!=0
        poll=('EveryMs',timing['outcomes'][0]['period_ms']) if enabled else ('Off',None)
        require((poll[0]=='Off' and case['poll']=='Off') or list(poll)==case['poll'],'source unavailable/period pairing differs')
        source=borrow(base.storage,32+22*i,22,phase,writable=True)
        for site,owner,o,n,field in SOURCE_WRITES:
            if owner=='S_i':writes.append(_store(source,o,n,live))
        copied=opaque_copy_interface(objects,instructions,copies[0][2],borrow(frame,208,10,phase),
            borrow(base.storage,source.offset+6,10,phase,writable=True),10,frame,live)
        require((set('r'+str(v) for v in range(4,12))|{'sp'})<=set(copied['normal_preserves']),
            'source copy loses suffix/reset carriers')
        iterations.append(dict(index=i,id=case['id'],channel=i,offset=source.offset,source=source,typed_definition=typed,
            child_history=dict(definition=value,possible_period=timing),poll=poll,opaque_poll_payload=(2,4) if not enabled else (),
            route=case['route'],opaque_route_payload=(6,10),physical_access_range=(10,16),
            active_access_payload_ranges=tuple((a-14,b-14) for a,b in typed['access']['active_payload_ranges']),
            trailing_padding=(21,22),freshness_ms=500 if i in (0,7) else 2000 if i==6 else 3000,
            opaque_copy=copied,requires_complete_source_constructor_and_child_result_transport=True))
    for site,owner,o,n,field in SOURCE_WRITES:
        if owner=='B':writes.append(_store(base,o,n,protected))
    history=tuple(_store(base,14528+2*i,2,protected) for i in range(8));writes.extend(history)
    # Validator's specialized constant receiver is not ordinary r1: this edge
    # must join the complete callee/static/source/preservation contract later.
    validator=next(e for e in proof['ordinary_edges'] if e[0]==338)
    return dict(proof=proof,widget=widget,region=region,iterations=tuple(iterations),local_writes=local_writes,
        possible_writes=tuple(writes),history_count=8,led_padding=_LED_PADDING,normal_exit_count=8,
        target_access_requirements=widget['target_access_requirements'],parent_frame=456,
        physical_peak_child_paths=((52,),(8,),(72,)),
        outgoing=dict(call=validator,result=borrow(frame,236,12,phase,writable=True),r8=frame.start+112,r0=frame.start+236,
            r1=proof['objects'][11]+24,r10=base.start,r11=frame.start+252,
            requires_specialized_validator_receiver_Result_preservation_and_graph=True),
        requires_actual_parent_ownership_fault_SP_and_complete_ingress=True,
        requires_validation_all_outcomes_fault_publication_and_epilogue=True,
        requires_full_ordinary_diagnostic_default_sink_PANICKED_boot_graph=True)
