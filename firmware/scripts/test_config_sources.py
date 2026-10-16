"""Focused actual-input Sources continuation checks.

Fixtures retain their supplied machine/source attribution. The complete
producer, ownership and final resource result remain separate gates.
"""
import unittest
from dataclasses import replace


class SourcePrivateHandoff(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from test_config_stack import ActualDecodedWidgets
        from config_decoded_widgets import _first_boot_decoded_widgets_state
        objects,rows,source,_=ActualDecodedWidgets.fixture()
        _,cls.state=_first_boot_decoded_widgets_state(objects,rows,source)
        cls.objects=cls.state.family_bridge.objects;cls.rows=cls.state.family_bridge.rows

    def test_actual_zero_and_nonempty_Widget_exits_retain_carriers_and_prefix(self):
        from config_source_handoff import _source_entries
        entries=_source_entries(self.objects,self.rows,self.state)
        self.assertEqual(len(entries),len(self.state.exits))
        frame=self.state.views.context.headers['visitor_frame'];B=self.state.views.context.owner.start
        self.assertTrue(any(not e.exit.committed for e in entries))
        self.assertTrue(any(e.exit.committed for e in entries))
        for e in entries:
            self.assertTrue(any(e.exit is v for v in self.state.exits))
            self.assertEqual(e.sequence.count.lower,8)
            self.assertEqual(e.call_site,self.state.views.context.headers['visitor']['proof']['function']+4376)
            self.assertEqual(dict(e.call_arguments),{'r0':frame.start+1128,'r1':frame.start+1500})
            self.assertEqual((e.old_Source_array.start,e.old_Source_array.extent,e.old_Source_array.writable),(B+32,176,False))
            self.assertTrue(any(v.space=='address' and (v.start,v.extent)==(B+244,200) for v in e.protected))
            self.assertEqual(tuple((v.start-frame.start,target-frame.start) for v,target in e.carriers),
                ((216,1001),(224,688),(228,576),(232,1420),(236,1380),(240,1400),(244,1440),(248,1360)))
            if e.exit.committed:
                n=220*len(e.exit.committed)
                self.assertTrue(any(v.space=='address' and (v.start,v.extent)==(B+444,n) for v in e.protected))
                self.assertTrue(any(v.space==frame.space and (v.start,v.extent)==(B+444,n) for v in e.protected))
            self.assertFalse(e.completed_Source_issued)

    def test_copied_description_or_reordered_commit_is_not_a_selected_exit(self):
        from config_source_handoff import _source_entries
        from config_abstract import Span
        state=self.state
        changed=replace(state,exits=(replace(state.exits[0],sequence=replace(state.exits[0].sequence,count=Span(6,6))),))
        with self.assertRaisesRegex(AssertionError,'Sources predecessor'):
            _source_entries(self.objects,self.rows,changed)
        e=next(v for v in state.exits if v.committed)
        item=replace(e.committed[0],source_fields=(('fake','receipt'),))
        changed=replace(state,exits=(replace(e,committed=(item,)+e.committed[1:]),))
        with self.assertRaisesRegex(AssertionError,'source/commit'):
            _source_entries(self.objects,self.rows,changed)

    def test_current_carrier_instruction_and_dependent_metadata_cannot_drift(self):
        from config_source_handoff import _source_entries
        state=self.state;V=state.views.context.headers['visitor']['proof']['function']
        changed=dict(self.rows);row=changed[V+546]
        changed[V+546]=replace(row,operands='r0, [sp, #0xdc]')
        with self.assertRaises(AssertionError):
            _source_entries(self.objects,changed,state)
        empty=replace(state.exits[0],Source_sequence=replace(state.exits[0].Source_sequence,
            decoder=replace(state.exits[0].Source_sequence.decoder,cursor=state.exits[0].sequence.decoder.cursor)))
        # A source header copied to a different current epoch remains invalid
        # even if its pointer and count values appear equal.
        from config_regions import borrow
        frame=state.views.context.headers['visitor_frame']
        empty=replace(empty,Source_sequence=replace(empty.Source_sequence,header=borrow(frame,1496,8,frame.phase,True)))
        with self.assertRaisesRegex(AssertionError,'Sources predecessor'):
            _source_entries(self.objects,self.rows,replace(state,exits=(empty,)))

    def test_removed_actual_child_returns_cannot_leave_a_live_Source_handoff(self):
        from config_source_handoff import _source_entries
        import copy
        changed=replace(self.state.control,record=copy.deepcopy(self.state.control.record))
        changed.record['nested_returns']=()
        # The selected frontier/receiver still match. They cannot replace the
        # same current complete ordinary return/ingress relation.
        with self.assertRaisesRegex(AssertionError,'Sources predecessor.*control'):
            _source_entries(self.objects,self.rows,replace(self.state,control=changed))


class SourceLiteralBodies(unittest.TestCase):
    @staticmethod
    def fixture():
        import json
        from pathlib import Path
        from test_config_stack import ActualDecodedWidgets,StartupConfigurationCopy,SameBLoaderBoundary
        from llvm_stack import Instruction
        objects,rows,source,item=ActualDecodedWidgets.fixture()
        extra=json.loads((Path(__file__).parent/'fixtures/config-decoded-sources.json').read_bytes())
        for at,encoded in extra['segments']:
            StartupConfigurationCopy.put(objects,at,bytes.fromhex(encoded))
        for f in extra['functions']:
            for name,address,extent,index in f['aliases']:objects.elf.functions[name]=(address,extent,index)
            objects.elf.symbol_entries=list(dict.fromkeys(objects.elf.symbol_entries+[tuple(v) for v in f['symbols']]))
            objects.elf.mapping_symbols.extend(tuple(v) for v in f['mapping'])
            objects.sizes[f['address']]=f['frame']
        objects.elf.mapping_symbols=list(dict.fromkeys(objects.elf.mapping_symbols))
        for r in extra['records']:
            rows[r['address']]=Instruction(r['address'],bytes.fromhex(r['bytes']),r['mnemonic'],r['operands'])
        return objects,rows,SameBLoaderBoundary.rebound(objects,source),item

    def test_complete_current_Source_retains_guarded_six_targets_and_saved_return_tail(self):
        from config_source_children import source_coverage,CHANNEL_SOURCE
        from config_stack import canonical_symbol
        objects,rows,_,_=self.fixture()
        name=next(n for n in objects.elf.functions if canonical_symbol(n)==CHANNEL_SOURCE)
        proof=source_coverage(objects,name,rows)
        self.assertEqual((proof['extent'],proof['frame']),(754,72))
        self.assertEqual(proof['access_targets'],tuple(proof['function']+off for off in (314,84,182,226,126,252)))
        self.assertEqual(proof['guarded_access_index_domain'],(0,5))
        self.assertTrue(proof['requires_same_source_successful_varint_argument'])
        self.assertEqual(proof['original_return_outline_remaining_saves'],36)
        self.assertTrue(any(e[0]==364 and e[1]=='b.w' for e in proof['ordinary_edges']))
        self.assertNotIn('completed_Source',proof)

    def test_coherent_table_byte_and_BL_to_original_return_outline_reject(self):
        from config_source_children import source_body,CHANNEL_SOURCE
        from config_stack import canonical_symbol
        from test_config_stack import StartupConfigurationCopy
        from llvm_stack import Instruction
        for mutation in ('table','tail'):
            objects,rows,_,_=self.fixture();name=next(n for n in objects.elf.functions if canonical_symbol(n)==CHANNEL_SOURCE)
            at=objects.elf.functions[name][0]&~1
            if mutation=='table':StartupConfigurationCopy.put(objects,at+78,b'\x77')
            else:
                site=at+364;target=0x10028682;raw=StartupConfigurationCopy.branch_link(site,target)
                StartupConfigurationCopy.put(objects,site,raw);rows[site]=Instruction(site,raw,'bl',hex(target))
            with self.subTest(mutation=mutation),self.assertRaises(AssertionError):source_body(objects,name,rows)

    def test_full_Source_activation_restores_original_PC_for_all_six_Access_arms(self):
        import config_source_children as sources
        from config_source_contracts import GRAMMARS
        from config_headers import VISITOR
        from config_widget_children import _name
        from config_finite import _literal_body
        from config_finite_returns import _closed_child_returns
        objects,rows,_,_=self.fixture();V=0x1001c654
        def none(*args):self.fail('unexpected Source entry object')
        parent=_literal_body(objects,_name(objects,VISITOR),rows,VISITOR,'decoded_sources_4376',
            GRAMMARS['visitor_4376'],none,sources._outline,window=V+4376,function_consumer=sources._function)
        children=sources.source_children(objects,rows,(parent,))
        sp=0x20070000
        nodes=_closed_child_returns(rows,(parent,),children,sp,sources.source_return_profiles(),())
        source=next(v for v in nodes if v['function']==0x1001b500)
        self.assertEqual((source['entry_SP'],source['return_SP'],source['return_PC'],source['lowest_SP']),
            (sp,sp,V+4380,sp-120))
        self.assertTrue(all(pc==V+4380 and returned==sp for at,pc,returned in source['returns']))
        self.assertFalse(source['architectural_LR_preserved'])
        # Private-proof injection, distinct from the coherent raw table
        # mutation above. No alternative target vector enters this solver.
        key=(0x1001b500,sources.CHANNEL_SOURCE);wrong=dict(children)
        wrong[key]=dict(children[key],access_targets=children[key]['access_targets'][:-1])
        with self.assertRaisesRegex(AssertionError,'guarded Source Access'):
            _closed_child_returns(rows,(parent,),wrong,sp,sources.source_return_profiles(),())


class SourceParentFamilies(unittest.TestCase):
    def test_complete_parent_windows_bind_same_Config_and_all_eight_Source_calls(self):
        from config_decoded_sources import _parents
        from config_source_children import CHANNEL_SOURCE,source_children
        objects,rows,_,_=SourceLiteralBodies.fixture()
        parents=_parents(objects,rows,0x1001c654,0x2000031c)
        calls=tuple(sorted(p['function']+off-0x1001c654 for p in parents
            for off,operation,target,role,frame in p['ordinary_edges'] if role==CHANNEL_SOURCE))
        self.assertEqual(calls,(4376,4906,4944,4982,5154,5254,5292,5332))
        from config_source_contracts import GRAMMARS
        self.assertEqual(sum(len(GRAMMARS['visitor_'+str(p['function']-0x1001c654)]['instructions']) for p in parents),546)
        children=source_children(objects,rows,parents)
        self.assertIn((0x1001b500,CHANNEL_SOURCE),children)
        self.assertTrue(any(p.get('semantic_role')=='seq_f32' for p in children.values()))

    def test_coherent_parent_Config_pool_change_is_not_another_private_allocation(self):
        from config_decoded_sources import _parents
        from test_config_stack import StartupConfigurationCopy
        import struct
        objects,rows,_,_=SourceLiteralBodies.fixture();V=0x1001c654
        StartupConfigurationCopy.put(objects,V+7124,struct.pack('<I',0x20000320))
        with self.assertRaisesRegex(AssertionError,'same private Config'):
            _parents(objects,rows,V,0x2000031c)


class SourceAggregateInterface(unittest.TestCase):
    @staticmethod
    def fixture(count=8,extent=96):
        from config_source_children import source_coverage,CHANNEL_SOURCE
        from config_widget_children import _name
        from config_regions import Storage,borrow
        from config_abstract import DecoderInterface,SequenceInterface,Span
        objects,rows,_,_=SourceLiteralBodies.fixture();phase='supplied_Source_adapter'
        visitor=Storage('stack:Source-fixture','supplied Visitor',0x20070000,1608,8,phase,True)
        loader=Storage(visitor.space,'supplied loader',visitor.start+1608,456,8,phase,True)
        frame=Storage(visitor.space,'supplied ChannelSource',visitor.start-72,72,8,phase,True)
        payload=Storage('address','supplied readonly EAV payload',0x10032b2a,extent,1,phase,False)
        decoder=DecoderInterface(borrow(loader,236,8,phase,True),borrow(payload,0,extent,phase),Span(0,0))
        sequence=SequenceInterface(borrow(visitor,1500,8,phase,True),decoder,Span(count,count))
        output=borrow(visitor,1128,22,phase,True)
        protected=(Storage(visitor.space,'Visitor saves',visitor.start+1512,96,4,phase,False),
            Storage(visitor.space,'loader saves',loader.start+352,104,4,phase,False))
        proof=source_coverage(objects,_name(objects,CHANNEL_SOURCE),rows)
        return objects,rows,proof,sequence,output,frame,protected

    def test_grouped_Source_keeps_all_dependent_variant_payloads_and_unknown_values(self):
        from config_source_effects import channel_source_interface
        args=self.fixture();result=channel_source_interface(*args);normal=result['normal']
        self.assertIsNotNone(normal)
        self.assertEqual(normal['source_order'],('definition','access','decoder','evidence','route','poll','freshness_ms'))
        self.assertEqual(tuple(v['selector'] for v in normal['access']),tuple(range(6)))
        self.assertEqual(tuple(v['selector'] for v in normal['route']),tuple(range(3)))
        self.assertEqual(tuple(v['selector'] for v in normal['poll']),(0,1))
        physical=normal['route'][1]
        self.assertEqual(physical['payload'],(('request','u16',(0,65535)),('response','u16',(0,65535))))
        self.assertEqual(physical['packed_word'],'request | (response <<16)')
        self.assertEqual(normal['count_after'].lower,7)
        self.assertEqual(normal['after'].cursor.lower,7)
        self.assertEqual(result['physical_child_bytes'],120)
        self.assertEqual(result['store_partition']['caller_count'],(20,4,4))
        self.assertEqual(len(result['store_partition']['local']),18)
        self.assertEqual(result['output_padding'],(21,22))
        self.assertEqual(result['pending_after_early_tag'],((12,16),(6,10),(4,6),(3,4)))
        self.assertFalse(result['completed_Source_issued'])
        self.assertFalse(result['semantic_Config_validated'])

    def test_missing_or_short_Source_never_makes_a_complete_package(self):
        from config_source_effects import channel_source_interface
        none=channel_source_interface(*self.fixture(count=0))
        self.assertIsNone(none['normal']);self.assertEqual(none['none']['tag'],2)
        self.assertEqual(none['none']['count_after'].lower,0)
        short=channel_source_interface(*self.fixture(extent=6))
        self.assertIsNone(short['normal']);self.assertTrue(short['errors'])
        self.assertTrue(all(v['count_after'].lower==7 for v in short['errors']))
        self.assertTrue(any(v['error']==4 for v in short['errors']))
        self.assertFalse(short['completed_Source_issued'])

    def test_only_unwritten_padding_can_share_protection_with_output(self):
        from config_source_effects import channel_source_interface
        from config_regions import Storage
        args=list(self.fixture());output=args[4];frame=args[5]
        padding=Storage(frame.space,'protected opaque padding',output.start+21,1,1,frame.phase,False)
        args[-1]=args[-1]+(padding,)
        self.assertIsNotNone(channel_source_interface(*args)['normal'])
        args[-1]=args[-1][:-1]+(replace(padding,start=output.start+20),)
        with self.assertRaisesRegex(AssertionError,'protected live storage'):channel_source_interface(*args)

    def test_error_phase_retains_variant_relations_before_the_failed_final_field(self):
        from config_source_effects import channel_source_interface
        result=channel_source_interface(*self.fixture(extent=6))
        self.assertIsNone(result['normal'])
        self.assertEqual(tuple(v['selector'] for v in result['guarded_routes']),(0,2))
        self.assertTrue(any(v['field']=='Route payload' for v in result['transitions']))
        self.assertTrue(any(v['stage']=='freshness_ms.freshness_ms' for v in result['errors']))
        self.assertFalse(result['completed_Source_issued'])


class SourceArrayAdapter(unittest.TestCase):
    def test_deepest_Source_child_may_be_exactly_adjacent_to_Config(self):
        from config_decoded_sources import _parents,_source_array
        from config_regions import Storage,borrow
        objects,rows,proof,sequence,output,child,guards=SourceAggregateInterface.fixture(extent=96)
        B=0x2000031c;config_end=B+14604
        visitor=replace(sequence.header.storage,start=config_end+120)
        loader=replace(sequence.decoder.header.storage,start=visitor.start+1608)
        decoder=replace(sequence.decoder,header=borrow(loader,236,8,child.phase,True))
        sequence=replace(sequence,header=borrow(visitor,1500,8,child.phase,True),decoder=decoder)
        saved=(replace(guards[0],start=visitor.start+1512),replace(guards[1],start=loader.start+352))
        old=Storage('address','old Source subarray',B+32,176,4,child.phase,False)
        result=_source_array(objects,rows,_parents(objects,rows,0x1001c654,B),sequence,saved,old)
        self.assertEqual(result['sequence'].count.lower,0)
        self.assertEqual(len(result['packages']),8)

    def test_Source_deepest_child_cannot_alias_untouched_Config_suffix(self):
        from config_decoded_sources import _parents,_source_array
        from config_regions import Storage,borrow
        objects,rows,proof,sequence,output,child,guards=SourceAggregateInterface.fixture(extent=96)
        B=0x2000031c;config_end=B+14604
        # Main H72 is above Config. Its varint32 + pop16 reaches eight bytes
        # inside the enclosing suffix, which the old array alone cannot guard.
        visitor=replace(sequence.header.storage,start=config_end+112)
        loader=replace(sequence.decoder.header.storage,start=visitor.start+1608)
        decoder=replace(sequence.decoder,header=borrow(loader,236,8,child.phase,True))
        sequence=replace(sequence,header=borrow(visitor,1500,8,child.phase,True),decoder=decoder)
        saved=(replace(guards[0],start=visitor.start+1512),replace(guards[1],start=loader.start+352))
        old=Storage('address','old Source subarray',B+32,176,4,child.phase,False)
        with self.assertRaisesRegex(AssertionError,'Config.*child|overlap'):
            _source_array(objects,rows,_parents(objects,rows,0x1001c654,B),sequence,saved,old)

    def test_all_eight_sources_parse_before_any_fixed_array_commit(self):
        from config_decoded_sources import _parents,_source_array
        from config_regions import Storage
        args=SourceAggregateInterface.fixture(extent=96)
        objects,rows,proof,sequence,output,frame,guards=args
        old=Storage('address','supplied unchanged Source array',0x2000031c+32,176,4,frame.phase,False)
        parents=_parents(objects,rows,0x1001c654,0x2000031c)
        result=_source_array(objects,rows,parents,sequence,guards,old)
        self.assertEqual(tuple(v['index'] for v in result['packages']),tuple(range(8)))
        self.assertEqual(tuple(v['source']['count_after'].lower for v in result['packages']),tuple(range(7,-1,-1)))
        self.assertEqual(result['sequence'].count.lower,0)
        self.assertEqual(result['sequence'].decoder.cursor.lower,56)
        self.assertEqual(result['old_array'],old)
        self.assertFalse(result['fixed_array_committed'])
        for index,p in enumerate(result['packages']):
            self.assertEqual(p['source']['before'],sequence.decoder if not index else result['packages'][index-1]['source']['after'])
            self.assertEqual(p['source']['after'],p['sequence_after'].decoder)
            self.assertTrue(all(e['old_array_unchanged'] for e in result['errors']))

    def test_late_source_error_keeps_only_paired_parsed_prefix_and_old_array(self):
        from config_decoded_sources import _parents,_source_array
        from config_regions import Storage
        args=SourceAggregateInterface.fixture(extent=55)
        objects,rows,proof,sequence,output,frame,guards=args
        old=Storage('address','supplied old Source array',0x2000031c+32,176,4,frame.phase,False)
        result=_source_array(objects,rows,_parents(objects,rows,0x1001c654,0x2000031c),sequence,guards,old)
        self.assertIsNone(result['sequence'])
        self.assertEqual(len(result['packages']),7)
        self.assertEqual(len(result['invocations']),8)
        self.assertIsNone(result['invocations'][-1]['result']['normal'])
        self.assertTrue(any(v['index']==7 for v in result['errors']))
        self.assertTrue(all(v['old_array_unchanged'] for v in result['errors']))
        self.assertFalse(result['fixed_array_committed'])


class SourceArrayEffects(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from config_decoded_sources import _parents,_source_array
        from config_regions import Storage
        args=SourceAggregateInterface.fixture(extent=96)
        objects,rows,proof,sequence,output,source_frame,guards=args
        owner=Storage('address','supplied private Config cell',0x2000031c,14604,4,source_frame.phase,True)
        old=Storage('address','supplied old Source array',owner.start+32,176,4,source_frame.phase,False)
        cls.objects=objects;cls.rows=rows;cls.parents=_parents(objects,rows,0x1001c654,owner.start)
        cls.array=_source_array(objects,rows,cls.parents,sequence,guards,old)
        cls.owner=owner;cls.frame=sequence.header.storage;cls.guards=guards;cls.sequence=sequence

    def test_actual_copy_lineage_and_last_stores_preserve_partial_prefix(self):
        from config_decoded_sources import _array_effects
        result=_array_effects(self.objects,self.rows,self.parents,self.array,self.owner,self.frame,self.guards,self.sequence)
        self.assertEqual(tuple(v['complete_after']-0x1001c654 for v in result['slots']),
            (4680,4694,4712,4728,4746,4762,4780,4796))
        self.assertEqual(result['opaque_Config_padding'],tuple((53+22*i,54+22*i) for i in range(8)))
        self.assertEqual(result['nonpadding_written_bytes'],168)
        self.assertEqual(tuple(v['prior_prefix_bytes'] for v in result['slots']),tuple(22*i for i in range(8)))
        self.assertTrue(all(v['source_package'] is self.array['packages'][i] for i,v in enumerate(result['slots'])))
        self.assertFalse(result['complete_ordinary_Source_prefix_issued'])

    def test_one_byte_deeper_child_overlap_and_source_order_splice_reject(self):
        from config_decoded_sources import _array_effects
        from config_regions import Storage
        deepest=Storage('address','physical byte inside restored memcpy child',self.frame.start-1,1,1,self.frame.phase,False)
        with self.assertRaisesRegex(AssertionError,'overlap|intersects'):
            _array_effects(self.objects,self.rows,self.parents,self.array,self.owner,self.frame,self.guards+(deepest,),self.sequence)
        changed=dict(self.array,packages=(self.array['packages'][1],self.array['packages'][0])+self.array['packages'][2:])
        with self.assertRaisesRegex(AssertionError,'paired Source order'):
            _array_effects(self.objects,self.rows,self.parents,changed,self.owner,self.frame,self.guards,self.sequence)


class ActualSourcePrivatePhases(unittest.TestCase):
    def test_same_private_predecessor_derives_Source_array_and_remaining_aggregates_once(self):
        from config_decoded_sources import _first_boot_source_phases
        objects,rows,source,item=SourceLiteralBodies.fixture()
        state=_first_boot_source_phases(objects,rows,source)
        self.assertTrue(state.entries)
        self.assertTrue(state.phases)
        self.assertTrue(any(p.array['sequence'] is not None for p in state.phases))
        self.assertTrue(any(p.remaining is not None and p.remaining['normal'] for p in state.phases))
        self.assertTrue(all(p.entry.predecessor is state.widgets for p in state.phases))
        self.assertTrue(all(p.entry is e for p,e in zip(state.phases,state.entries)))
        self.assertIs(state.source_context,state.widgets.family_bridge.source_context)
        self.assertEqual(state.current(),state)
        self.assertFalse(state.complete_ordinary_group_issued)
        self.assertEqual(state.ordinary_frontier_pending,5630)
        # A detached checkpoint must retain both values and the actual shared
        # package association, even when a replacement compares equal.
        phase=next(p for p in state.phases if p.effects is not None)
        slot=phase.effects['slots'][0];original=slot['source_package']
        self.assertIs(original,phase.array['packages'][0])
        slot['source_package']=dict(original)
        try:
            with self.assertRaisesRegex(AssertionError,'record changed'):state.current()
        finally:slot['source_package']=original
        old=phase.array['fixed_array_committed'];phase.array['fixed_array_committed']=True
        try:
            with self.assertRaisesRegex(AssertionError,'record changed'):state.current()
        finally:phase.array['fixed_array_committed']=old


class SourcePhaseSnapshot(unittest.TestCase):
    def test_repeated_packages_are_independent_immutable_graph_nodes(self):
        from config_decoded_sources import _phase_graph
        package={'cursor':[1,2],'fields':{'selector':1}}
        graph={'packages':(package,),'copies':tuple(package for _ in range(1000))}
        frozen=_phase_graph(graph)
        self.assertLess(len(frozen[1]),10)
        package['fields']['selector']=2
        self.assertNotEqual(_phase_graph(graph),frozen)
        package['fields']['selector']=1
        self.assertEqual(_phase_graph(graph),frozen)

    def test_equal_value_replacement_loses_the_actual_alias(self):
        from config_decoded_sources import _phase_graph
        package={'cursor':1};shared={'source':package,'effect':package}
        independent={'source':package,'effect':dict(package)}
        self.assertEqual(shared,independent)
        self.assertNotEqual(_phase_graph(shared),_phase_graph(independent))
        self.assertNotEqual(_phase_graph({'tag':True}),_phase_graph({'tag':1}))

    def test_cycles_and_unsupported_values_reject(self):
        from config_decoded_sources import _phase_graph
        cycle=[];cycle.append(cycle)
        with self.assertRaisesRegex(AssertionError,'cyclic'):_phase_graph(cycle)
        with self.assertRaisesRegex(AssertionError,'unsupported'):_phase_graph({'value':object()})


class SourceRemainingAggregates(unittest.TestCase):
    @staticmethod
    def fixture(extent=160):
        from config_decoded_sources import _parents
        from config_regions import Storage,borrow
        from config_abstract import Span
        objects,rows,proof,source,output,frame,guards=SourceAggregateInterface.fixture(extent=extent)
        visitor=source.header.storage;phase=frame.phase
        owner=Storage('address','supplied private Config',0x2000031c,14604,4,phase,True)
        payload=Storage('address','embedded_eav',source.decoder.payload.start-12,extent+12,1,phase,False)
        top=replace(source,header=borrow(visitor,288,8,phase,True),
            decoder=replace(source.decoder,payload=borrow(payload,12,extent,phase),cursor=Span(56,56)),count=Span(7,7))
        prefix=Storage('address','supplied already-committed Sources',owner.start+32,176,4,phase,False)
        return objects,rows,_parents(objects,rows,0x1001c654,owner.start),top,owner,guards+(prefix,)

    def test_warning_Fuel_and_colors_keep_source_counts_unknown_bits_and_partial_stores(self):
        from config_decoded_sources import _remaining_aggregates
        result=_remaining_aggregates(*self.fixture())
        self.assertEqual(result['warning']['scope_domain'],(0,1))
        self.assertEqual(result['warning']['mode_domain'],(0,1))
        self.assertEqual(result['warning']['packed_word'],'page_ms | (scope <<16) | (mode <<24)')
        self.assertEqual(result['warning']['complete_store'],0x1001c654+5040)
        self.assertEqual(tuple((v['wire_tag'],v['emitted_tag']) for v in result['normal']),((0,2),(1,3),(2,0),(2,1)))
        for alternative in result['normal']:
            self.assertEqual(alternative['after'].count.lower,4)
            self.assertEqual(alternative['Fuel']['complete_store'],0x1001c654+5570)
            self.assertEqual(alternative['colors']['partial_background_store'],0x1001c654+5618)
            self.assertEqual(alternative['colors']['complete_store'],0x1001c654+5628)
            self.assertEqual(alternative['Fuel']['padding'],(30,32))
        stoich=result['normal'][1]
        self.assertEqual(stoich['Fuel']['active_fields'],(('ratio','f32_bits',(0,0xffffffff)),))
        blend=result['normal'][-1]
        self.assertEqual(blend['Fuel']['density_newtype_count_decrement'],0)
        self.assertEqual(blend['Fuel']['remaining_sequence_counts'],(4,3,2,1,0))
        self.assertFalse(result['complete_ordinary_group_issued'])

    def test_short_colors_preserve_completed_warning_and_Fuel_but_no_final_frontier(self):
        from config_decoded_sources import _remaining_aggregates
        result=_remaining_aggregates(*self.fixture(extent=60))
        self.assertFalse(result['normal'])
        self.assertTrue(any(v['stage']=='global WarningColors' and v['completed_aggregates']==('Sources','WarningPresentation','Fuel')
            for v in result['errors']))
        self.assertFalse(result['complete_ordinary_group_issued'])


if __name__=='__main__':unittest.main()
