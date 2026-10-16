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


class SourceRegisterReturns(unittest.TestCase):
    @staticmethod
    def fixture():
        from config_decoded_sources import _parents
        from config_source_children import source_children,source_return_profiles
        from config_finite_returns import _closed_child_returns
        from config_widget_contracts import ROLES
        objects,rows,_,_=SourceLiteralBodies.fixture()
        parents=_parents(objects,rows,0x1001c654,0x2000031c)
        children=source_children(objects,rows,parents);sp=0x20070000
        def evaluate(current):
            return _closed_child_returns(current,parents,children,sp,source_return_profiles(),
                ('seq_f32',ROLES['warning_colors']))
        return rows,parents,children,sp,evaluate

    def test_actual_57_calls_restore_high_carriers_but_outline338_clobbers_r4(self):
        rows,parents,children,sp,evaluate=self.fixture();nodes=evaluate(rows)
        expected=tuple('r'+str(i) for i in range(4,12))
        roots=tuple((p['function']+off+4,target,role) for p in parents
            for off,operation,target,role,frame in p['ordinary_edges'])
        self.assertEqual(len(roots),57)
        for pc,target,role in roots:
            matching=[n for n in nodes if n['function']==target and n['role']==role and
                n['entry_SP']==sp and n['return_PC']==pc]
            self.assertEqual(len(matching),1)
            self.assertEqual(matching[0]['preserved_high_gprs'],expected)
            self.assertFalse(matching[0]['architectural_LR_preserved'])
        setup=[n for n in nodes if n['function']==0x10029178]
        self.assertTrue(setup)
        self.assertTrue(all('r4' not in n['preserved_high_gprs'] for n in setup))
        # Both current typed roles must remain separate memoized activations.
        self.assertEqual(len({n['role'] for n in setup}),2)
        memcpy=[n for n in nodes if n['function']==0x100294f4]
        self.assertTrue(memcpy)
        self.assertTrue(all(n['architectural_LR_after']!=n['return_PC'] for n in memcpy))

    def test_changed_unsaved_high_definition_changes_the_actual_return_transfer(self):
        # Private transfer injection, not a coherent current-body mutant.
        # Complete literal matching remains a separate public prerequisite.
        rows,parents,children,sp,evaluate=self.fixture();changed=dict(rows)
        at=0x10029178;row=changed[at]
        self.assertEqual(row.operands,'r4, r0')
        changed[at]=replace(row,operands='r8, r0')
        nodes=evaluate(changed)
        self.assertTrue(any(n['entry_SP']==sp and 'r8' not in n['preserved_high_gprs'] for n in nodes))
        from config_source_control import _source_carrier_returns
        with self.assertRaisesRegex(AssertionError,'actual live high GPR'):
            _source_carrier_returns(changed,parents,children,nodes,sp)

    def test_expanded_Source_graph_checks_high_FP_and_exact_57_return_members(self):
        from config_source_control import _source_carrier_returns
        rows,parents,children,sp,evaluate=self.fixture();nodes=evaluate(rows)
        result=_source_carrier_returns(rows,parents,children,nodes,sp)
        self.assertEqual(len(result['parent_calls']),57)
        self.assertFalse(result['architectural_LR_preserved'])
        with self.assertRaisesRegex(AssertionError,'actual live high GPR'):
            _source_carrier_returns(rows,parents,children,(),sp)
        changed=dict(rows);at=0x1001bb02+20;row=changed[at]
        # Private scanner injection. Actual current children contain no VFP
        # instruction here; literal matching would reject this changed body.
        changed[at]=replace(row,mnemonic='vldr',operands='s16, [r1]')
        with self.assertRaisesRegex(AssertionError,'live float source carrier'):
            _source_carrier_returns(changed,parents,children,nodes,sp)


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
        self.assertTrue(all(p.parent_envelopes['stores']==80 for p in state.phases))
        self.assertFalse(state.structural.confers_active_state)
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


class ActualSourceActiveControl(unittest.TestCase):
    def test_actual_source_phases_close_guarded_ordinary_edges_without_an_owner(self):
        from config_decoded_sources import _first_boot_source_phases
        from config_source_control import _source_control
        objects,rows,source,_=SourceLiteralBodies.fixture()
        state=_first_boot_source_phases(objects,rows,source)
        control=_source_control(state)
        V=state.widgets.views.context.headers['visitor']['proof']['function']
        self.assertIs(control.state,state)
        self.assertEqual(len(control.source_guards),len(state.phases))
        self.assertIn(V+5630,control.record['stopped_frontiers'])
        self.assertNotIn(V+4376,control.record['stopped_frontiers'])
        self.assertFalse(control.record['pending_contexts_confer_state'])
        self.assertTrue(any(v['complete_Source_array'] for v in control.source_guards))
        self.assertTrue(any(v['Controls_ready'] for v in control.source_guards))
        for guarded,phase in zip(control.source_guards,state.phases):
            relation=guarded['publication_lineage']
            if not guarded['complete_Source_array']:
                self.assertIsNone(relation);continue
            self.assertEqual(relation['capture_order'],(7,0,1,2,3,4,5,6))
            self.assertEqual(relation['payload_lineage'][-1]['slot']['complete_after'],V+4796)
            for i,item in enumerate(relation['payload_lineage']):
                self.assertIs(item['package'],phase.array['packages'][i])
        self.assertTrue(all(v['guarded_Access_domain']==(0,5)
            for phase in control.source_guards for v in phase['invocations'] if v['Access_table_entered']))
        self.assertFalse(control.complete_ordinary_group_issued)
        with self.assertRaisesRegex(AssertionError,'actual private phase'):
            _source_control(state.structural)
        original=control.record['successors'];control.record['successors']=()
        try:
            with self.assertRaisesRegex(AssertionError,'active source/control record changed'):
                control.current()
        finally:control.record['successors']=original
        original=control.source_guards[0]['parsed_sources'];control.source_guards[0]['parsed_sources']=-1
        try:
            with self.assertRaisesRegex(AssertionError,'active source/control record changed'):
                control.current()
        finally:control.source_guards[0]['parsed_sources']=original
        self.assertIs(control.current(),control)


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


class SourceStructuralControl(unittest.TestCase):
    @staticmethod
    def consume(objects,rows,source):
        from config_source_control import _source_structural_control
        from config_decoded_widgets import _first_boot_decoded_widgets_state
        from config_decoded_sources import _parents
        from config_source_children import source_children
        _,widgets=_first_boot_decoded_widgets_state(objects,rows,source)
        objects=widgets.family_bridge.objects;rows=widgets.family_bridge.rows
        c=widgets.views.context;V=c.headers['visitor']['proof']['function']
        parents=_parents(objects,rows,V,c.owner.start);children=source_children(objects,rows,parents)
        return _source_structural_control(objects,rows,widgets,parents,children)

    def test_current_complete_group_retains_pending_contexts_without_active_authority(self):
        objects,rows,source,_=SourceLiteralBodies.fixture();result=self.consume(objects,rows,source)
        V=result.selection.context.headers['visitor']['proof']['function']
        self.assertEqual(tuple(at-V for at,target,reason in result.outside_shared_contexts),
            (4408,6118,6156,6186,6476,6550,6578,6606,6632,6654,6756,6760,6774,6856))
        self.assertEqual(result.selection.cuts&{V+4376,V+5630},frozenset((V+5630,)))
        self.assertTrue(result.unentered_outgoing)
        self.assertFalse(result.confers_active_state)
        self.assertTrue(result.selection.child_contexts)

    def test_coherent_pending_predecessor_interior_and_unlisted_fallthrough_reject(self):
        from test_config_stack import StartupConfigurationCopy,SameBLoaderBoundary
        from llvm_stack import Instruction
        V=0x1001c654
        # Manually encoded byte/row pairs, with the explicit source context
        # rebound to that image. No fresh decoder or stale-byte failure.
        for off,raw,op,args,wanted in (
            (6148,bytes.fromhex('0c46'),'mov','r4, r1','shared-error predecessor differs'),
            (6652,bytes.fromhex('01d1'),'bne',hex(V+6658),'foreign direct ingress'),
            (6216,bytes.fromhex('00bf'),'nop','','unselected fallthrough')):
            objects,rows,source,_=SourceLiteralBodies.fixture();at=V+off
            StartupConfigurationCopy.put(objects,at,raw);rows[at]=Instruction(at,raw,op,args)
            with self.subTest(offset=off),self.assertRaisesRegex(AssertionError,wanted):
                self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,source))


class SourceParentEnvelopes(unittest.TestCase):
    @staticmethod
    def fixture():
        objects,rows,parents,sequence,owner,protected=SourceRemainingAggregates.fixture()
        # This geometry-only adapter covers the whole Source parent, before
        # array assignment. The suffix-only fixture's completed Source guard
        # belongs to its later epoch and is not an ancestor of the whole group.
        from validation import require
        require((protected[-1].start,protected[-1].extent)==(owner.start+32,176),'supplied later Source guard differs')
        return objects,rows,parents,sequence,owner,protected[:-1]

    def test_all_current_parent_stores_have_owned_protected_footprints(self):
        from config_source_parent_effects import _parent_envelopes
        args=self.fixture()
        objects,rows,parents,sequence,owner,protected=args
        result=_parent_envelopes(objects,rows,parents,sequence,owner,protected)
        self.assertEqual((result['stores'],result['local_stores'],result['Config_stores']),(80,53,27))
        self.assertEqual(result['static_write_bytes'],707)
        self.assertEqual(result['unaligned_parent_requirement'],(0x1001c654+5570,5,2))
        self.assertFalse(result['confers_active_state'])
        self.assertEqual(tuple(v[0] for v in result['pointer_epochs']),(5376,5382,5398,5408,5468,5480))

    def test_parent_physical_alias_guard_is_retained(self):
        from config_source_parent_effects import _parent_envelopes
        from config_regions import Storage
        objects,rows,parents,sequence,owner,protected=self.fixture()
        frame=sequence.header.storage
        guard=Storage('address','physical protected header byte',frame.start+1004,1,1,frame.phase,False)
        with self.assertRaisesRegex(AssertionError,'overlap|intersects'):
            _parent_envelopes(objects,rows,parents,sequence,owner,protected+(guard,))


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
        self.assertTrue(result.get('Fuel_assignments'),'paired reached Fuel assignment lost after later Colors failure')
        assignment=result['Fuel_assignments'][0]
        self.assertEqual((assignment['Fuel']['wire_tag'],assignment['Fuel']['emitted_tag']),(0,2))
        self.assertEqual(assignment['Fuel']['complete_store'],0x1001c654+5570)
        self.assertEqual((assignment['after'].cursor.lower,assignment['after'].cursor.upper),(60,60))
        self.assertEqual(tuple((o,n) for v,o,n in assignment['possible_writes']),
            ((0,4),(29,1),(7,1),(28,1),(8,4),(12,4),(16,4),(20,4),(24,4),(4,1),(5,2)))
        self.assertFalse(result['complete_ordinary_group_issued'])


class SourceParentChronology(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from config_decoded_sources import _array_effects
        from config_source_parent_effects import _parent_envelopes
        from config_source_control import _source_carrier_returns
        from config_source_children import source_children,source_return_profiles
        from config_finite_returns import _closed_child_returns
        from config_regions import borrow
        SourceArrayEffects.setUpClass();a=SourceArrayEffects
        cls.args=(a.objects,a.rows,a.parents,a.array,
            _array_effects(a.objects,a.rows,a.parents,a.array,a.owner,a.frame,a.guards,a.sequence),a.frame)
        children=source_children(a.objects,a.rows,a.parents)
        from config_widget_contracts import ROLES
        returns=_closed_child_returns(a.rows,a.parents,children,a.frame.start,source_return_profiles(),('seq_f32',ROLES['warning_colors']))
        transfer=_source_carrier_returns(a.rows,a.parents,children,returns,a.frame.start)
        carriers=tuple((borrow(a.frame,cell,4,a.frame.phase),a.frame.start+target)
            for cell,target in ((216,1001),(224,688),(228,576),(232,1420),(236,1380),(240,1400),(244,1440),(248,1360)))
        cls.tail=(carriers,transfer,_parent_envelopes(a.objects,a.rows,a.parents,
            replace(a.sequence,header=borrow(a.frame,288,8,a.frame.phase,True)),a.owner,a.guards))

    def test_conditional_normal_parent_keeps_real_order_and_package_aliases(self):
        from config_source_lineage import _publication_lineage
        value=_publication_lineage(*self.args,*self.tail)
        V=0x1001c654;path=value['normal_parent_path']
        self.assertEqual(value['capture_order'],(7,0,1,2,3,4,5,6))
        self.assertLess(path.index(V+5410),path.index(V+5414))
        self.assertLess(path.index(V+5426),path.index(V+5430))
        self.assertLess(path.index(V+5854),path.index(V+4410))
        self.assertLess(path.index(V+4664),path.index(V+4670))
        self.assertEqual(tuple(v['returned_tag_domain'] for v in value['source_calls']),((0,1),)*8)
        for i,item in enumerate(value['payload_lineage']):
            self.assertIs(item['package'],self.args[3]['packages'][i])
            self.assertIs(item['slot']['source_package'],item['package'])
        self.assertFalse(value['complete_ordinary_Source_prefix_issued'])
        self.assertIs(value['copied_padding_is_initialized'],False)

    def test_stale_transport_package_and_missing_child_transfer_reject(self):
        from config_source_lineage import _publication_lineage
        effects=dict(self.args[4]);chains=list(effects['transports'])
        changed=list(chains[1]);changed[0]=dict(changed[0],source=chains[0][1]['destination'])
        chains[1]=tuple(changed);effects['transports']=tuple(chains)
        with self.assertRaisesRegex(AssertionError,'same-package source/destination'):
            _publication_lineage(*self.args[:4],effects,self.args[5],*self.tail)
        transfer=dict(self.tail[1],parent_calls=self.tail[1]['parent_calls'][1:])
        with self.assertRaisesRegex(AssertionError,'actual child transfer'):
            _publication_lineage(*self.args,self.tail[0],transfer,self.tail[2])
        carriers=list(self.tail[0]);carriers[2]=(carriers[2][0],carriers[1][1])
        with self.assertRaisesRegex(AssertionError,'inherited pointer producer'):
            _publication_lineage(*self.args,tuple(carriers),*self.tail[1:])

    def test_equal_package_copy_and_sentinel_only_domain_do_not_authorize_publication(self):
        from config_source_lineage import _publication_lineage
        array=self.args[3];effects=dict(self.args[4]);slots=list(effects['slots'])
        slots[0]=dict(slots[0],source_package=dict(slots[0]['source_package']));effects['slots']=tuple(slots)
        with self.assertRaisesRegex(AssertionError,'package/tag/copy association'):
            _publication_lineage(*self.args[:4],effects,self.args[5],*self.tail)
        normal=array['packages'][0]['source'];original=normal['tag_domain'];normal['tag_domain']=(0,1,3)
        try:
            with self.assertRaisesRegex(AssertionError,'package/tag/source epoch'):
                _publication_lineage(*self.args,*self.tail)
        finally:normal['tag_domain']=original


class SourceAccessFamilies(unittest.TestCase):
    @staticmethod
    def fixture():
        from config_regions import Storage,borrow
        from config_source_access import SourceAccessContext,SourceFamily,SITE_SHAPES,EXPECTED_SOURCE_ROLES,SOURCE_EPOCHS
        objects,rows,_,_=SourceLiteralBodies.fixture();phase='decoded_sources_group'
        visitor=Storage('stack:Reset:0x20070870','supplied Visitor',0x20070000,1608,8,phase,True)
        loader=Storage(visitor.space,'supplied loader',visitor.start+1608,456,8,phase,True)
        config=Storage('address','private unpublished ConfigSlot',0x2000031c,14604,4,phase,True)
        child=Storage(visitor.space,'supplied ChannelSource',visitor.start-72,72,8,phase,True)
        guards=(Storage(visitor.space,'loader saves',loader.start+352,104,4,phase,False),
            Storage(visitor.space,'Visitor saves',visitor.start+1512,96,4,phase,False),
            Storage('address','metadata/taken/padding',config.start+14576,28,4,phase,False))
        families=[];caller=0x1001c654
        for role in EXPECTED_SOURCE_ROLES:
            source=role in EXPECTED_SOURCE_ROLES[:2];frame=child if source else visitor
            protected=guards+((Storage(visitor.space,'Source saves',child.start+36,36,4,phase,False),) if source else ())
            context=SourceAccessContext(loader,visitor,frame,borrow(config,0,14576,phase),protected)
            parent=0x1001b500 if source else caller
            sites=tuple((parent+off,raw,field,width,direction,label) for off,raw,field,width,direction,label in SITE_SHAPES[role])
            epochs=tuple((caller+off,out,tag,8-i) for i,(off,out,tag) in enumerate(SOURCE_EPOCHS)) if source else ()
            families.append(SourceFamily(role,'sizez',parent,caller,sites,context,epochs))
        return objects,rows,tuple(families)

    def test_four_conditional_families_keep_eight_paired_nonaffine_outputs(self):
        from config_source_access import read_source_family,current_source_family
        objects,rows,families=self.fixture()
        self.assertEqual(len(families),4)
        for family in families:
            self.assertEqual(read_source_family(family.to_record()),family)
            self.assertIs(current_source_family(objects,family),family)
            self.assertIs(family.to_record()['physical_tag_is_completed_Source'],False)
        local,output,warning,fuel=families
        self.assertEqual(len(output.epochs),8)
        self.assertEqual(tuple(a-output.context.visitor.start for i in range(8) for a,b in output.footprints(i)),
            (1134,522,634,750,886,946,1006,1070))
        self.assertEqual(local.footprints(7),((local.context.frame.start+25,local.context.frame.start+27),))
        self.assertEqual(warning.footprints(0),((warning.context.visitor.start+1065,warning.context.visitor.start+1067),))
        self.assertEqual(fuel.footprints(0),((fuel.context.config.start+5,fuel.context.config.start+7),))

    def test_width_numeric_alias_epoch_order_and_saved_cells_cannot_be_substituted(self):
        import copy
        from config_source_access import read_source_family
        _,_,families=self.fixture()
        cases=[]
        for value in (2.0,True,4):
            record=copy.deepcopy(families[0].to_record());record['sites'][0]['width']=value;cases.append(record)
        record=copy.deepcopy(families[1].to_record());record['epochs'].reverse();cases.append(record)
        record=copy.deepcopy(families[0].to_record());record['context']['protected'].pop();cases.append(record)
        record=copy.deepcopy(families[2].to_record());record['role']='source_route_local';cases.append(record)
        record=copy.deepcopy(families[3].to_record());record['physical_tag_is_completed_Source']=True;cases.append(record)
        for i,record in enumerate(cases):
            with self.subTest(case=i),self.assertRaises(AssertionError):read_source_family(record)

    def test_current_source_and_visitor_access_bytes_are_checked_separately(self):
        from config_source_access import current_source_family
        from test_config_stack import StartupConfigurationCopy
        for index in (1,2,3):
            objects,rows,families=self.fixture();family=families[index]
            current_source_family(objects,family)
            at=family.sites[0][0];raw=objects.read_only(at,4)
            StartupConfigurationCopy.put(objects,at,bytes([raw[0]^1])+raw[1:])
            with self.subTest(role=family.role),self.assertRaisesRegex(AssertionError,'operation bytes'):
                current_source_family(objects,family)

    def test_full_deeper_physical_activation_stays_disjoint_beside_adjacency(self):
        _,_,families=self.fixture();original=families[0];c=original.context
        end=c.config.start+14604
        def moved(visitor_start):
            delta=visitor_start-c.visitor.start
            shift=lambda v:replace(v,start=v.start+delta) if v.space.startswith('stack:') else v
            return replace(original,context=replace(c,loader=shift(c.loader),visitor=shift(c.visitor),
                frame=shift(c.frame),protected=tuple(shift(v) for v in c.protected)))
        adjacent=moved(end+120)
        self.assertEqual(adjacent.context.visitor.start-120,end)
        with self.assertRaisesRegex(AssertionError,'overlap|physical'):
            moved(end+112)
        with self.assertRaisesRegex(AssertionError,'interval|physical'):
            moved(-8)


if __name__=='__main__':unittest.main()
