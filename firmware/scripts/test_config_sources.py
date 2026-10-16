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
            self.assertTrue(any(v.space==frame.space and (v.start,v.extent)==(frame.start+280,4)
                for v in e.protected),'initialized saved top count must remain protected after4360')
            saved,value,producer=e.saved_top_count
            self.assertEqual((saved.start,saved.extent,value.lower,value.upper,producer),
                (frame.start+280,4,7,7,e.call_site-16))
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

    def test_failed_primitive_results_keep_exact_origin_and_short_arm_chain(self):
        from config_source_effects import channel_source_interface
        result=channel_source_interface(*self.fixture())
        self.assertTrue(result['errors'])
        self.assertTrue(all('origin' in error and 'child_outcome' in error for error in result['errors']))
        from config_source_effects import _source_error_origins
        _source_error_origins(result)
        origins=result['primitive_invocations']
        response=next(v for v in origins if v['site']==448)
        request=response['predecessor']
        self.assertEqual((request['site'],request['semantic_role']),(420,'seq_u16'))
        self.assertIs(response['before'],response['predecessor_outcome']['state'])
        self.assertTrue(any(response['predecessor_outcome'] is v for v in request['result']['outcomes']))
        self.assertTrue(any(v['origin'] is response for v in result['errors']))
        self.assertTrue(any(v['origin']['site']==582 for v in result['errors']))
        self.assertTrue(any(v['origin']['site']==54 and v['child_outcome']['kind']=='success'
            for v in result['errors'] if v['stage'].endswith('.invalid')))
        error=next(v for v in result['errors'] if v['origin'] is response)
        saved=error['child_outcome'];error['child_outcome']=dict(saved)
        with self.assertRaisesRegex(AssertionError,'origin'):_source_error_origins(result)
        error['child_outcome']=saved
        saved_site=response['site'];response['site']=420
        with self.assertRaisesRegex(AssertionError,'origin'):_source_error_origins(result)
        response['site']=saved_site
        saved_previous=response['predecessor'];response['predecessor']=next(v for v in origins if v['site']==134)
        with self.assertRaisesRegex(AssertionError,'origin'):_source_error_origins(result)
        response['predecessor']=saved_previous
        for key,value in (('error',99),('cursor',replace(saved['cursor'],lower=0,upper=0)),
            ('count_after',replace(error['count_after'],lower=0,upper=0))):
            prior=error[key];error[key]=value
            with self.assertRaisesRegex(AssertionError,'origin'):_source_error_origins(result)
            error[key]=prior
        _source_error_origins(result)
        short=channel_source_interface(*self.fixture(extent=6))
        _source_error_origins(short)
        failed=tuple(arm for step in short['transitions'] for arm in step.get('all_alternatives',())
            if arm['after'] is None and arm['children'])
        self.assertTrue(failed)
        self.assertTrue(all(any(origin['result'] is child for origin in short['primitive_invocations'])
            for arm in failed for child in arm['children']))
        none=channel_source_interface(*self.fixture(count=0))
        self.assertEqual(none['primitive_invocations'],())
        self.assertEqual(none['errors'],())


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


class SourceCopyDescriptions(unittest.TestCase):
    @staticmethod
    def inputs():
        from config_structural import _detached_inputs
        objects,rows,_,_=SourceLiteralBodies.fixture()
        return _detached_inputs(objects,rows)

    @staticmethod
    def effect():
        # Explicit post-check descriptor representation fixture. It does not
        # replace the actual API/body/provider checks in the adapter below.
        child=dict(shape='copy',role='typed-copy',function=8,extent=12,frame=8,
            objects=({'kind':'fixed','value':4},),callbacks=(),ordinary_edges=(),indirect_sites=(),
            requires_parent_outline_abi=False,transfers=((0,'bx','lr'),),
            requires_runtime_ownership_entry_and_guard_closure=True,
            requires_typed_inputs_live_sink_and_default_options=True,
            manual_frame_requires_complete_builtin=None,reference_only=False)
        return dict(children=(child,),possible_writes=((1,2,3),))

    def test_complete_children_share_without_effect_or_mutable_backing_aliases(self):
        from config_source_sharing import _CopyDescriptions
        objects,rows=self.inputs();pool=_CopyDescriptions(objects,rows)
        original=self.effect();first=pool.checked(original,objects,rows,8)
        second=pool.checked(self.effect(),objects,rows,8)
        self.assertIs(first['children'],second['children']);self.assertIsNot(first,second)
        self.assertEqual(first['possible_writes'],original['possible_writes'])
        with self.assertRaises(TypeError):first['children'][0]['objects'][0]['value']=7
        original['children'][0]['objects'][0]['value']=9
        self.assertEqual(first['children'][0]['objects'][0]['value'],4)

    def test_complete_context_types_fields_and_unsupported_fallback_do_not_merge(self):
        from config_source_sharing import _CopyDescriptions
        import copy
        objects,rows=self.inputs();pool=_CopyDescriptions(objects,rows);first=pool.checked(self.effect(),objects,rows,8)
        for key,value in (('role','other'),('function',10),('frame',16),('callbacks',((4,'other'),)),
            ('requires_parent_outline_abi',0),('objects',({'kind':'fixed','value':True},)),
            ('new_context',('ancestry',4))):
            effect=self.effect();effect['children'][0][key]=value
            self.assertIsNot(pool.checked(effect,objects,rows,8)['children'],first['children'])
        other,other_rows=self.inputs()
        with self.assertRaisesRegex(AssertionError,'input context'):pool.checked(self.effect(),other,other_rows,8)
        self.assertIsNot(pool.checked(self.effect(),objects,rows,10)['children'],first['children'])
        effect=self.effect();effect['children'][0]['transfers']=list(effect['children'][0]['transfers'])
        self.assertIs(pool.checked(effect,objects,rows,8),effect)
        custom=object();unshared=self.effect()
        self.assertIs(_CopyDescriptions(custom,rows).checked(unshared,custom,rows,8),unshared)

    def test_array_copies_keep_all_fresh_dynamic_checks_after_equal_descriptors(self):
        from unittest.mock import patch
        from config_source_sharing import _CopyDescriptions
        from config_structural import _detached_inputs
        from config_decoded_sources import _array_effects,_parents
        from config_refill_children import opaque_copy_interface
        from config_regions import Storage
        SourceArrayEffects.setUpClass();a=SourceArrayEffects
        objects,rows=_detached_inputs(a.objects,a.rows);parents=_parents(objects,rows,0x1001c654,a.owner.start)
        pool=_CopyDescriptions(objects,rows)
        with patch('config_refill_children.opaque_copy_interface',wraps=opaque_copy_interface) as checked:
            one=_array_effects(objects,rows,parents,a.array,a.owner,a.frame,a.guards,a.sequence,pool)
            two=_array_effects(objects,rows,parents,a.array,a.owner,a.frame,a.guards,a.sequence,pool)
            self.assertEqual(checked.call_count,24)
            self.assertIsNot(one['transports'][0][0]['effect'],two['transports'][0][0]['effect'])
            self.assertIs(one['transports'][0][0]['effect']['children'],two['transports'][0][0]['effect']['children'])
            deepest=Storage('address','changed protected child byte',a.frame.start-72,1,1,a.frame.phase,False)
            with self.assertRaisesRegex(AssertionError,'overlap|intersects'):
                _array_effects(objects,rows,parents,a.array,a.owner,a.frame,a.guards+(deepest,),a.sequence,pool)

    def test_custom_reader_keeps_fresh_consumed_byte_checks_without_pool_reuse(self):
        from config_source_sharing import _CopyDescriptions
        from config_decoded_sources import _array_effects
        SourceArrayEffects.setUpClass();a=SourceArrayEffects
        class Provider:
            def __init__(self):
                self.elf=a.objects.elf;self.sizes=a.objects.sizes
                self.calls=0;self.changed=False;self.changed_read=False;self.target=None
            def read_only(self,start,extent):
                self.calls+=1;raw=a.objects.read_only(start,extent)
                if self.changed and start<=self.target<start+extent:
                    self.changed_read=True;raw=bytearray(raw);raw[self.target-start]^=1;raw=bytes(raw)
                return raw
        provider=Provider();pool=_CopyDescriptions(provider,a.rows)
        first=_array_effects(provider,a.rows,a.parents,a.array,a.owner,a.frame,a.guards,a.sequence,pool)
        reads=provider.calls
        second=_array_effects(provider,a.rows,a.parents,a.array,a.owner,a.frame,a.guards,a.sequence,pool)
        self.assertGreater(provider.calls,reads);self.assertFalse(pool._values)
        one=first['transports'][0][0]['effect']['children'];two=second['transports'][0][0]['effect']['children']
        self.assertEqual(one,two);self.assertIsNot(one,two)
        provider.target=one[1]['function'];provider.changed=True
        with self.assertRaises(AssertionError):
            _array_effects(provider,a.rows,a.parents,a.array,a.owner,a.frame,a.guards,a.sequence,pool)
        self.assertTrue(provider.changed_read,'rejection must follow the changed consumed builtin bytes')


class SourceUseGraph(unittest.TestCase):
    def test_closed_batch_checks_shared_graph_once_and_all_exact_members(self):
        from types import SimpleNamespace
        from unittest.mock import patch
        import config_source_uses as subject
        from config_decoded_sources import _phase_graph
        package={'cursor':[1,2]}
        uses=tuple(subject._SourceUse('graph-only fixture',0,i,None,None,None,(0,1),(),{'package':package},()) for i in range(1000))
        family=SimpleNamespace(role='graph-only fixture',footprints=lambda index:((0,1),))
        control=SimpleNamespace(source_guards=({'package':package},),record={})
        graph=_phase_graph(((family.role,),(),uses,control.source_guards,control.record))
        # The real source/current producer is intentionally supplied here;
        # only collection/alias traversal and call counts are under test.
        bridge=subject._SourceAccessBridge(control,(family,),(),uses,graph)
        checked=[]
        def current(value):
            checked.append(value)
            self.assertIs(value,bridge)
            self.assertEqual(_phase_graph(((family.role,),(),value.uses,control.source_guards,control.record)),graph)
            return value
        with patch.object(subject._SourceAccessBridge,'current',current),\
            patch.object(subject,'_check_member',wraps=subject._check_member) as member,\
            patch.object(subject,'_member_graph',wraps=subject._member_graph) as projection:
            # The family root is supplied: whole-current comparison checks
            # all member values and aliases in this representation fixture.
            self.assertIs(subject._check_source_uses(bridge),uses)
            self.assertEqual(member.call_count,1000)
            self.assertEqual(checked,[bridge])
            self.assertEqual(projection.call_count,0,
                'whole-current checked collection must not rebuild shared member projections')
            package['cursor'].append(3)
            with self.assertRaises(AssertionError):subject._check_source_uses(bridge)
            self.assertEqual(checked,[bridge,bridge])
            self.assertEqual(member.call_count,1000,'failed fresh whole-current check must consume no members')

    def test_one_shared_snapshot_retains_values_and_cross_member_aliases(self):
        from config_decoded_sources import _phase_graph
        from config_source_uses import _SourceUse,_member_graph
        package={'cursor':[1,2],'fields':{'selector':1}}
        uses=tuple(_SourceUse('graph-only fixture',0,i,None,None,None,(0,1),(),
            {'package':package,'same_package':package},()) for i in range(1000))
        graph=_phase_graph(uses);root,nodes=graph;members=nodes[root[1]][1]
        self.assertLess(len(nodes),2200)
        self.assertEqual(_member_graph(graph,members[500]),_phase_graph(uses[500]))
        different=replace(uses[500],source={'package':package,'same_package':dict(package)})
        self.assertNotEqual(_member_graph(graph,members[500]),_phase_graph(different))
        package['fields']['selector']=2
        self.assertNotEqual(_member_graph(graph,members[500]),_phase_graph(uses[500]))

    def test_projection_keeps_types_and_rejects_cyclic_or_unsupported_nodes(self):
        from config_decoded_sources import _phase_graph
        from config_source_uses import _member_graph
        value={'tag':True};graph=_phase_graph(value)
        self.assertEqual(_member_graph(graph,graph[0]),graph)
        self.assertNotEqual(_member_graph(graph,graph[0]),_phase_graph({'tag':1}))
        with self.assertRaisesRegex(AssertionError,'cyclic'):
            _member_graph((('reference',0),(('tuple',(('reference',0),)),)),('reference',0))
        with self.assertRaisesRegex(AssertionError,'unsupported'):
            _member_graph((('reference',0),(('unreviewed',()),)),('reference',0))

    def test_equal_copy_visible_only_outside_member_requires_whole_graph(self):
        from types import SimpleNamespace
        from config_decoded_sources import _phase_graph
        from config_source_uses import _SourceUse,_member_graph,_checked_bridge_graph
        package={'cursor':[1,2]}
        uses=tuple(_SourceUse('graph-only fixture',0,i,None,None,None,(0,1),(),{'package':package},()) for i in range(2))
        control=SimpleNamespace(source_guards=({'package':package},),record={})
        bridge=SimpleNamespace(families=(),allocation_lineage=(),uses=uses,control=control)
        bridge.checkpoint=_phase_graph((bridge.families,bridge.allocation_lineage,uses,control.source_guards,control.record))
        _checked_bridge_graph(bridge)
        before=_phase_graph(uses[0]);uses[0].source['package']=dict(package)
        # The source has one local reference, so this alone is insufficient.
        self.assertEqual(before,_phase_graph(uses[0]))
        with self.assertRaisesRegex(AssertionError,'bridge/use record changed'):_checked_bridge_graph(bridge)
        uses[0].source['package']=package;_checked_bridge_graph(bridge)


class ActualSourceReachedUses(unittest.TestCase):
    def test_actual_private_allocation_and_source_reach_join_all_four_families(self):
        from config_decoded_sources import _first_boot_source_phases
        from config_source_control import _source_control
        from config_source_uses import _source_access_bridge,_check_source_use,_check_source_uses
        objects,rows,source,_=SourceLiteralBodies.fixture()
        state=_first_boot_source_phases(objects,rows,source)
        print('Sources preparation phase graph:',len(state.phases),'phases;',len(state.checkpoint[1]),'nodes',flush=True)
        copies=tuple(event['effect']['children'] for phase in state.phases if phase.effects is not None
            for event in phase.effects['transports'][0])+tuple(slot['payload']['effect']['children']
            for phase in state.phases if phase.effects is not None for slot in phase.effects['slots']
            if 'children' in slot['payload']['effect'])
        print('Sources actual post-check copy descriptions:',len(copies),'copy events;',
            len({id(v) for v in copies}),'child tuples;',len({id(child) for v in copies for child in v}),'bodies',flush=True)
        self.assertTrue(copies);self.assertEqual(len({id(v) for v in copies}),1)
        self.assertEqual(len({id(child) for v in copies for child in v}),2)
        control=_source_control(state)
        print('Sources preparation control graph:',len(control.checkpoint[1]),'nodes',flush=True)
        bridge=_source_access_bridge(control)
        print('Sources internal graph-only measurement:',len(bridge.uses),'uses;',len(bridge.checkpoint[1]),'shared nodes',flush=True)
        self.assertIs(bridge.control,control)
        self.assertEqual(tuple(v.role for v in bridge.families),
            ('source_route_local','source_route_output','warning_presentation_local','Fuel_final_halfword'))
        self.assertTrue(bridge.uses)
        self.assertEqual({v.role for v in bridge.uses},set(v.role for v in bridge.families))
        frame=state.widgets.views.context.headers['visitor_frame']
        for use in _check_source_uses(bridge):
            self.assertFalse(use.completed_prefix_issued)
            if use.role=='source_route_output':
                phase=state.phases[use.phase_index]
                self.assertIs(use.source['package'],phase.array['packages'][use.source_index])
                self.assertEqual(use.footprint,(frame.start+SOURCE_OUTPUTS[use.source_index]+6,
                    frame.start+SOURCE_OUTPUTS[use.source_index]+10))
            elif use.role=='source_route_local':
                self.assertEqual(use.footprint,(frame.start-47,frame.start-45))
                self.assertEqual(use.source['Route']['packed_word'],'request | (response <<16)')
            elif use.role=='Fuel_final_halfword':
                self.assertEqual(use.source['assignment']['Fuel']['complete_store'],0x1001c654+5570)
                self.assertIn(use.source['assignment']['source_guard'],((0,None),(1,None),(2,0),(2,1)))
        with self.assertRaisesRegex(AssertionError,'actual private active control'):
            _source_access_bridge(state.structural)
        with self.assertRaisesRegex(AssertionError,'internally derived use'):
            _check_source_use(bridge,replace(bridge.uses[0]))
        output=next(u for u in bridge.uses if u.role=='source_route_output')
        package=output.source['package'];output.source['package']=dict(package)
        try:
            with self.assertRaisesRegex(AssertionError,'bridge/use record changed'):
                _check_source_use(bridge,output)
        finally:output.source['package']=package
        old=bridge.uses[0].source['source_phase'];bridge.uses[0].source['source_phase']='wrong epoch'
        try:
            with self.assertRaisesRegex(AssertionError,'use record changed'):
                _check_source_use(bridge,bridge.uses[0])
        finally:bridge.uses[0].source['source_phase']=old


SOURCE_OUTPUTS=(1128,516,628,744,880,940,1000,1064)


class SourcePhaseSnapshot(unittest.TestCase):
    def test_construction_workspaces_release_without_cyclic_collection_on_success_and_error(self):
        import gc,sys,weakref
        from config_decoded_sources import _phase_graph
        visitors=[];tables=[]
        def observe(frame,event,arg):
            if event=='call' and frame.f_code.co_name=='visit' and frame.f_code.co_filename==_phase_graph.__code__.co_filename:
                values=frame.f_locals;visitor=values['visit']
                if not any(ref() is visitor for ref in visitors):
                    visitors.append(weakref.ref(visitor))
                    tables.append(tuple(values[name] for name in ('nodes','memo','active','literals')))
        enabled=gc.isenabled();old=sys.getprofile();gc.disable()
        try:
            sys.setprofile(observe)
            shared={'tag':True,'payload':[1,b'bytes',None]};value=(shared,shared)
            snapshots=tuple(_phase_graph(value) for _ in range(4))
            cycle=[];cycle.append(cycle)
            for invalid,message in ((cycle,'cyclic'),({'unsupported':object()},'unsupported')):
                with self.assertRaisesRegex(AssertionError,message):_phase_graph(invalid)
            sys.setprofile(old)
            self.assertEqual(len(visitors),6)
            self.assertTrue(all(not table for group in tables for table in group),
                'completed/failed snapshots must release construction-only tables without waiting for GC')
            self.assertTrue(all(ref() is None for ref in visitors),
                'recursive visitor closure must not keep itself alive')
            self.assertTrue(all(snapshot==snapshots[0] for snapshot in snapshots))
            shared['tag']=1
            self.assertNotEqual(_phase_graph(value),snapshots[0])
            self.assertEqual(snapshots[0][1][0][1],(('reference',1),('reference',1)))
        finally:
            sys.setprofile(old)
            if enabled:gc.enable()

    def test_local_scalar_tokens_share_without_changing_literal_values_types_or_container_aliases(self):
        from config_decoded_sources import _phase_graph
        shared={'x':1,'y':True,'z':[1,False,None,'x',b'x']};value=[shared,shared]
        expected=(('reference',0),(
            ('list',(('reference',1),('reference',1))),
            ('mapping',((('value','str','x'),('value','int',1)),
                (('value','str','y'),('value','bool',True)),
                (('value','str','z'),('reference',2)))),
            ('list',(('value','int',1),('value','bool',False),('value','NoneType',None),
                ('value','str','x'),('value','bytes',b'x')))))
        result=_phase_graph(value);self.assertEqual(result,expected)
        scalar=result[1][1][1][0][1];again=result[1][2][1][0]
        self.assertIs(scalar,again,'same typed scalar token must share local immutable storage')
        self.assertIsNot(scalar,result[1][1][1][1][1])
        self.assertIsNot(scalar,_phase_graph(value)[1][1][1][0][1])
        shared['z'].append('later mutation')
        self.assertEqual(result,expected);self.assertNotEqual(_phase_graph(value),result)

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

    def test_saved_count_footprint_survives_selected_writes_with_exact_adjacent_captures(self):
        from config_source_parent_effects import _parent_envelopes
        from config_regions import Storage
        objects,rows,parents,sequence,owner,protected=self.fixture();frame=sequence.header.storage
        saved=Storage(frame.space,'supplied initialized saved count',frame.start+280,4,4,frame.phase,False)
        result=_parent_envelopes(objects,rows,parents,sequence,owner,protected+(saved,))
        self.assertEqual(result['stores'],80)
        for shift in (-1,1):
            with self.assertRaisesRegex(AssertionError,'intersects'):
                _parent_envelopes(objects,rows,parents,sequence,owner,protected+(replace(saved,start=saved.start+shift,alignment=1),))


class SourceRemainingAggregates(unittest.TestCase):
    def test_uncounted_temperature_and_failed_Colors_keep_their_paired_child_results(self):
        from config_decoded_sources import _remaining_aggregates
        from config_abstract import Span
        result=_remaining_aggregates(*self.fixture());V=0x1001c654
        temperature=tuple(v for v in result['transitions'] if v['field']=='Blend density temperature')
        self.assertEqual(len(temperature),1,'same-source direct6266 result must remain available to the parent join')
        extra=temperature[0]
        self.assertEqual((extra['site'],extra['source_guard']),(V+6266,(2,1)))
        self.assertEqual((extra['sequence_count_before'],extra['sequence_count_after']),(Span(4,4),Span(4,4)))
        self.assertEqual(extra['after'],next(v['state'] for v in extra['child']['outcomes'] if v['kind']=='success'))
        tail=tuple(v for v in result['transitions'] if v['field']=='Blend gasoline_stoich' and v['before'].decoder==extra['after'])
        self.assertEqual(len(tail),1);self.assertEqual(tail[0]['before'].count,Span(4,4))
        short=_remaining_aggregates(*self.fixture(extent=60))
        self.assertFalse(short['normal']);self.assertTrue(short['Colors_invocations'])
        for invocation,assigned in zip(short['Colors_invocations'],short['Fuel_assignments']):
            self.assertIs(invocation['Fuel_assignment'],assigned)
            self.assertEqual(invocation['before'].decoder,assigned['after'])
            self.assertEqual(invocation['before'].count,Span(5,5))
            self.assertTrue(invocation['child']['outcomes'])
        errors=tuple(v for v in short['errors'] if v['stage']=='global WarningColors')
        self.assertTrue(errors)
        for error in errors:
            self.assertTrue(any(error['source_invocation'] is v for v in short['Colors_invocations']))
            self.assertTrue(any(error['child_outcome'] is v for v in error['source_invocation']['child']['outcomes']))

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

    def test_warning_domain_errors_retain_the_actual_packed_intermediate_route(self):
        from config_decoded_sources import _remaining_aggregates
        result=_remaining_aggregates(*self.fixture());V=0x1001c654
        for stage in ('Warning scope.domain','Warning mode.domain'):
            errors=tuple(v for v in result['errors'] if v['stage']==stage)
            self.assertTrue(errors)
            for error in errors:
                self.assertEqual(error['frontier'],V+56,'Warning domain failure must use its packed carrier before56')
                self.assertEqual(error['error'],14)
                self.assertEqual(error['error_transport'],dict(kind='packed_Warning_lowbyte',
                    path=tuple(V+o for o in (6446,6448,4866,4870,4872,4886,4888,4890,5024,5032,56)),
                    mask=0x020000ff,whole_r5_is_scalar_error=False))
        page=tuple(v for v in result['errors'] if v['stage']=='Warning page')
        self.assertTrue(page)
        self.assertTrue(all(v['error_transport']['kind']=='packed_Warning_lowbyte' and
            v['frontier']==V+56 and v['error_transport']['path'][0]==V+5210 for v in page))


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




class SourceSuffixChronology(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from config_decoded_sources import _remaining_aggregates
        from config_source_children import source_children,source_return_profiles
        from config_source_control import _source_carrier_returns
        from config_source_parent_effects import _parent_envelopes
        from config_finite_returns import _closed_child_returns
        from config_widget_contracts import ROLES
        objects,rows,parents,sequence,owner,protected=SourceRemainingAggregates.fixture()
        cls.suffix=_remaining_aggregates(objects,rows,parents,sequence,owner,protected)
        children=source_children(objects,rows,parents);frame=sequence.header.storage
        returns=_closed_child_returns(rows,parents,children,frame.start,source_return_profiles(),('seq_f32',ROLES['warning_colors']))
        transfer=_source_carrier_returns(rows,parents,children,returns,frame.start)
        envelopes=_parent_envelopes(objects,rows,parents,sequence,owner,protected[:-1])
        cls.args=(objects,rows,parents,cls.suffix,frame,owner,transfer,envelopes)

    def test_conditional_suffix_pairs_real_calls_guards_and_late_stores(self):
        from config_source_lineage import _suffix_lineage
        result=_suffix_lineage(*self.args);V=0x1001c654
        self.assertEqual(result['warning']['scope_alternatives'],(0,1))
        self.assertEqual(result['warning']['mode_alternatives'],(0,1))
        self.assertEqual(result['warning']['complete_store'],V+5040)
        self.assertEqual(tuple(v['source_guard'] for v in result['Fuel']),((0,None),(1,None),(2,0),(2,1)))
        for arm,assignment in zip(result['Fuel'],self.suffix['Fuel_assignments']):
            self.assertIs(arm['assignment'],assignment)
            self.assertEqual(arm['physical_store_order'],tuple(V+o for o in (5532,5534,5542,5546,5550,5554,5558,5562,5566,5568,5570)))
            self.assertEqual(arm['pending_after_tag'],((5,7),))
        blend=result['Fuel'][-1]
        self.assertEqual(blend['direct_density_count_decrement'],0)
        self.assertEqual(blend['counted_tail'],(4,3,2,1,0))
        self.assertEqual(result['Colors']['source_order'],('foreground','background'))
        self.assertEqual(result['Colors']['store_order'],(V+5618,V+5628))
        self.assertEqual(result['frontier'],V+5630)
        self.assertFalse(result['confers_active_state'])
        self.assertTrue('argument_transports' in result,'conditional suffix must retain its actual argument transports')
        arguments=result['argument_transports']
        self.assertEqual(tuple(v['site']-V for v in arguments['children']),
            tuple(v['site']-V for v in self.suffix['transitions'])+(5578,)*len(self.suffix['Colors_invocations']))
        self.assertEqual(arguments['page_flag_path'],tuple(V+off for off in (5194,5198,5200,5204,5206,5210)))
        self.assertEqual(arguments['normal_inactive_predication'],(V+5022,V+5130))
        self.assertEqual(arguments['low_FP_source'],(V+6406,V+5562,'s0'))

    def test_suffix_rejects_unpaired_child_count_and_Fuel_color_association(self):
        from config_source_lineage import _suffix_lineage
        from config_abstract import Span
        steps=list(self.suffix['transitions']);index=next(i for i,v in enumerate(steps) if v['field']=='Blend gasoline_stoich')
        steps[index]=dict(steps[index],before=replace(steps[index]['before'],count=Span(5,5)))
        changed=dict(self.suffix,transitions=tuple(steps))
        with self.assertRaisesRegex(AssertionError,'counted Blend|paired child'):
            _suffix_lineage(*self.args[:3],changed,*self.args[4:])
        normal=list(self.suffix['normal']);normal[0]=dict(normal[0],Fuel=dict(normal[0]['Fuel']))
        changed=dict(self.suffix,normal=tuple(normal))
        with self.assertRaisesRegex(AssertionError,'same completed Fuel'):
            _suffix_lineage(*self.args[:3],changed,*self.args[4:])
        transfer=dict(self.args[-2],parent_calls=tuple(v for v in self.args[-2]['parent_calls'] if v[0]!=0x1001c654+5578))
        with self.assertRaisesRegex(AssertionError,'actual child transfer'):
            _suffix_lineage(*self.args[:-2],transfer,self.args[-1])
        pairs=list(self.args[-2]['parent_calls']);position=next(i for i,v in enumerate(pairs) if v[0]==0x1001c654+5578)
        site,target,role,mask=pairs[position];pairs[position]=(site,target+2,role,mask)
        transfer=dict(self.args[-2],parent_calls=tuple(pairs))
        with self.assertRaisesRegex(AssertionError,'actual child transfer'):
            _suffix_lineage(*self.args[:-2],transfer,self.args[-1])

    def test_short_colors_do_not_erase_prior_reached_Fuel_or_make_outgoing_state(self):
        from config_decoded_sources import _remaining_aggregates
        from config_source_lineage import _suffix_lineage
        objects,rows,parents,sequence,owner,protected=SourceRemainingAggregates.fixture(extent=60)
        suffix=_remaining_aggregates(objects,rows,parents,sequence,owner,protected)
        result=_suffix_lineage(objects,rows,parents,suffix,sequence.header.storage,owner,*self.args[-2:])
        self.assertTrue(result['Fuel']);self.assertFalse(suffix['normal'])
        self.assertFalse(result['Colors']['normal']);self.assertFalse(result['outgoing_top4'])
        self.assertTrue(all(v['completed_Fuel_store']==0x1001c654+5570 for v in result['Colors']['errors']))

    def test_error_only_suffix_still_binds_exact_preceding_success_input(self):
        from config_decoded_sources import _remaining_aggregates
        from config_source_lineage import _suffix_lineage
        objects,rows,parents,sequence,owner,protected=SourceRemainingAggregates.fixture(extent=57)
        suffix=_remaining_aggregates(objects,rows,parents,sequence,owner,protected)
        self.assertIsNone(suffix['warning'])
        self.assertTrue(any(v['stage']=='Warning mode' for v in suffix['errors']))
        _suffix_lineage(objects,rows,parents,suffix,sequence.header.storage,owner,*self.args[-2:])
        steps=list(suffix['transitions']);i=next(i for i,v in enumerate(steps) if v['field']=='Warning mode')
        steps[i]=dict(steps[i],before=replace(steps[i]['before']))
        changed=dict(suffix,transitions=tuple(steps))
        with self.assertRaisesRegex(AssertionError,'source chain'):
            _suffix_lineage(objects,rows,parents,changed,sequence.header.storage,owner,*self.args[-2:])

    def test_error_carrier_keeps_packed_Warning_frontier_and_completed_Fuel(self):
        from config_source_lineage import _suffix_lineage
        value=_suffix_lineage(*self.args);V=0x1001c654
        self.assertEqual(len(value['error_carriers']),len(self.suffix['errors']))
        packed=next(v for v in value['error_carriers'] if v['error']['stage']=='Warning scope.domain')
        self.assertEqual(packed['carrier'],'packed_0x02000000_or_error_u8')
        self.assertTrue(all(V+off in packed['path'] for off in (6446,6448,4866,4870,4872,4876,4880,4884,4886,4888,4890,5024,5028,5032)))
        self.assertEqual(packed['stopped_before'],V+56);self.assertFalse(packed['whole_register_narrowed'])
        self.assertTrue(all(v['prior_physical_stores']==(V+4794,V+5040,V+5570)
            for v in value['error_carriers'] if v['error']['stage']=='global WarningColors'))
        errors=list(self.suffix['errors']);i=next(i for i,v in enumerate(errors) if v['stage']=='Warning scope.domain')
        errors[i]=dict(errors[i],frontier=0x1001c654+170)
        changed=dict(self.suffix,errors=tuple(errors))
        with self.assertRaisesRegex(AssertionError,'error carrier'):
            _suffix_lineage(*self.args[:3],changed,*self.args[4:])


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


class SourcePolicyV7(unittest.TestCase):
    @staticmethod
    def augment(dependencies):
        # Supplied conditional dependency geometry only. These fixtures do
        # not derive the inherited sixteen from an actual complete producer.
        from dataclasses import fields,is_dataclass
        from config_regions import Storage,borrow
        from config_source_access import SourceAccessContext,SourceFamily,EXPECTED_SOURCE_ROLES,SITE_SHAPES,SOURCE_EPOCHS
        def physical(value):
            if type(value) is Storage:
                return replace(value,start=value.start+0x20070648) if value.space.startswith('stack:') else value
            if is_dataclass(value):return replace(value,**{f.name:physical(getattr(value,f.name)) for f in fields(value)})
            if type(value) is tuple:return tuple(physical(v) for v in value)
            return value
        dependencies=tuple(physical(v) for v in dependencies);widget=dependencies[-4]
        old=widget.context;phase='decoded_sources_group'
        loader=replace(old.loader,phase=phase);visitor=replace(old.visitor,phase=phase)
        config=replace(old.config.storage,phase=phase)
        child=Storage(visitor.space,'supplied ChannelSource',visitor.start-72,72,8,phase,True)
        guards=tuple(replace(v,phase=phase) for v in old.protected);sources=[]
        for role in EXPECTED_SOURCE_ROLES:
            local=role in EXPECTED_SOURCE_ROLES[:2];parent=0x1001b500 if local else widget.parent
            protected=guards+((Storage(visitor.space,'Source saves',child.start+36,36,4,phase,False),) if local else ())
            context=SourceAccessContext(loader,visitor,child if local else visitor,borrow(config,0,14576,phase),protected)
            sites=tuple((parent+off,*rest) for off,*rest in SITE_SHAPES[role])
            epochs=tuple((widget.parent+off,out,tag,8-i) for i,(off,out,tag) in enumerate(SOURCE_EPOCHS)) if local else ()
            sources.append(SourceFamily(role,'sizez',parent,widget.parent,sites,context,epochs))
        return dependencies+tuple(sources)

    def test_full_conditional_v7_tuple_roundtrips_without_creating_completion(self):
        import json
        import config_platform as subject
        from test_config_stack import ValidatorPlatformFamilies,ConditionalStackPolicy
        dependencies=ValidatorPlatformFamilies().dependencies('sizez')
        if len(dependencies)==20:dependencies=self.augment(dependencies)
        objects,view,context=ConditionalStackPolicy().fixture()
        policy=subject.reviewed_eav_policy(objects,view,context,contradictory_effects=(),owned_dependencies=dependencies)
        self.assertEqual((policy.schema,policy.scope,policy.scope_sha256),(6,'h2-rp2350-secure-owned-access-v7',
            '50408f1a4774caff701c236184a4f17060cb9e17ade5fffaa0ca208d4f1b6181'))
        self.assertEqual(len(dependencies),24)
        self.assertEqual(subject.read_policy(json.loads(json.dumps(policy.to_record())),policy),policy)
        self.assertTrue(all(v.to_record()['physical_tag_is_completed_Source'] is False for v in dependencies[-4:]))
        self.assertLessEqual(len((json.dumps(policy.to_record(),sort_keys=True)+'\n').encode()),65536)

    def test_v7_rejects_missing_duplicate_old_scope_and_cross_activation_sources(self):
        import json,copy
        import config_platform as subject
        from test_config_stack import ValidatorPlatformFamilies,ConditionalStackPolicy
        values=ValidatorPlatformFamilies().dependencies('sizez')
        if len(values)==20:values=self.augment(values)
        objects,view,context=ConditionalStackPolicy().fixture()
        policy=subject.reviewed_eav_policy(objects,view,context,contradictory_effects=(),owned_dependencies=values)
        for fault in ('missing','duplicate','reorder','legacy20','visitor','loader','config'):
            changed=list(values)
            if fault=='missing':changed.pop()
            elif fault=='duplicate':changed[-1]=changed[-2]
            elif fault=='reorder':changed[-4],changed[-3]=changed[-3],changed[-4]
            elif fault=='legacy20':changed=changed[:-4]
            else:
                family=changed[-4];c=family.context
                if fault=='loader':c=replace(c,loader=replace(c.loader,identity='another loader'))
                elif fault=='visitor':c=replace(c,visitor=replace(c.visitor,identity='another Visitor'))
                else:
                    config=replace(c.config.storage,start=c.config.start+0x4000)
                    c=replace(c,config=replace(c.config,storage=config),protected=tuple(
                        replace(v,start=v.start+0x4000) if v.space=='address' else v for v in c.protected))
                changed[-4]=replace(family,context=c)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                subject.reviewed_eav_policy(objects,view,context,contradictory_effects=(),owned_dependencies=tuple(changed))
        record=json.loads(json.dumps(policy.to_record()))
        for fault in ('old','width','bool','kind','complete'):
            changed=copy.deepcopy(record)
            if fault=='old':changed.update(schema=5,scope='h2-rp2350-secure-owned-access-v6',
                scope_sha256='627f472cf991e64d868d29eb67ef302218d57327a726cdfbc535ed5a97e6eeed')
            elif fault=='width':changed['owned_dependencies'][-1]['sites'][0]['width']=2.0
            elif fault=='bool':changed['owned_dependencies'][-1]['sites'][0]['offset']=True
            elif fault=='kind':changed['owned_dependencies'][-1]['kind']='decoded_widget_access_family'
            else:changed['owned_dependencies'][-1]['physical_tag_is_completed_Source']=True
            with self.subTest(fault=fault),self.assertRaises(AssertionError):subject.read_policy(changed,policy)

    def test_current_image_rechecks_each_Source_site_after_coherent_artifact_identity_change(self):
        import hashlib,copy
        from config_platform import current_image_policy
        from test_config_stack import ValidatorPlatformFamilies
        elf,eav,stack=ValidatorPlatformFamilies().image_fixture('sizez')
        record=stack['policies'][0]
        self.assertEqual(len(current_image_policy(record,elf,eav).owned_dependencies),24)
        original=elf.data
        for dependency in record['owned_dependencies'][-4:]:
            site=dependency['sites'][0]['site'];section=next(s for s in elf.sections if s[3]<=site<site+4<=s[3]+s[5])
            changed=bytearray(original);changed[section[4]+site-section[3]]^=1;elf.data=bytes(changed)
            policy=copy.deepcopy(record);policy['elf_sha256']=hashlib.sha256(elf.data).hexdigest()
            try:
                with self.subTest(role=dependency['role']),self.assertRaisesRegex(AssertionError,'Sources operation bytes'):
                    current_image_policy(policy,elf,eav)
            finally:elf.data=original

    def test_supplied_24_family_image_obeys_exact_enclosing_wire_cap(self):
        import json,tempfile
        from pathlib import Path
        from unittest.mock import patch
        import check_memory_layout as image
        import capture_target_bench as capture
        from test_config_stack import ValidatorPlatformFamilies
        supplied=ValidatorPlatformFamilies()
        elf,eav,stack=supplied.image_fixture('sizez',1)
        static,headroom=image.geometry(elf)
        small=image._qualified_image(stack,elf,static,headroom,eav,elf.fixture_source_context)
        size=lambda v:len((json.dumps(v,sort_keys=True)+'\n').encode())
        elf,eav,stack=supplied.image_fixture('sizez',65536-size(small)+1)
        static,headroom=image.geometry(elf)
        record=image._qualified_image(stack,elf,static,headroom,eav,elf.fixture_source_context)
        wire=(json.dumps(record,sort_keys=True)+'\n').encode()
        self.assertEqual(len(record['stack']['policies'][0]['owned_dependencies']),24)
        self.assertEqual(len(wire),65536)
        with tempfile.TemporaryDirectory() as folder,patch.object(capture,'ElfImage',return_value=elf):
            path=Path(folder)/'memory-layout.json';path.write_bytes(wire)
            self.assertEqual(capture.read_memory_gate(path,'supplied.elf',eav,elf.fixture_source_context)['record'],record)
            path.write_bytes(wire+b' ')
            with self.assertRaisesRegex(ValueError,'oversized'):
                capture.read_memory_gate(path,'supplied.elf',eav,elf.fixture_source_context)


if __name__=='__main__':unittest.main()
