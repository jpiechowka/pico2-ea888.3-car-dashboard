"""Focused Tail checks using retained machine rows and explicit abstract inputs."""
import unittest
from dataclasses import replace


class TailLiteralBodies(unittest.TestCase):
    @staticmethod
    def fixture(extent=None,delta=0):
        import json
        import struct
        from pathlib import Path
        from test_config_sources import ActualDecodedSources
        from test_config_stack import StartupConfigurationCopy,SameBLoaderBoundary
        from llvm_stack import Instruction
        objects,rows,source,item=ActualDecodedSources.fixture(extent,delta)
        B=objects.read_only(0x1001c654+7124,4)
        extra=json.loads((Path(__file__).parent/'fixtures/config-decoded-tail.json').read_bytes())
        for at,encoded in extra['segments']:StartupConfigurationCopy.put(objects,at,bytes.fromhex(encoded))
        for f in extra['functions']:
            for name,address,extent,index in f['aliases']:objects.elf.functions[name]=(address,extent,index)
            objects.elf.symbol_entries=list(dict.fromkeys(objects.elf.symbol_entries+[tuple(v) for v in f['symbols']]))
            objects.elf.mapping_symbols.extend(tuple(v) for v in f['mapping'])
            if f['frame'] is not None:objects.sizes[f['address']]=f['frame']
        objects.elf.mapping_symbols=list(dict.fromkeys(objects.elf.mapping_symbols))
        for r in extra['records']:
            rows[r['address']]=Instruction(r['address'],bytes.fromhex(r['bytes']),r['mnemonic'],r['operands'])
        for offset in (3508,7124):StartupConfigurationCopy.put(objects,0x1001c654+offset,B)
        return objects,rows,SameBLoaderBoundary.rebound(objects,source),item

    def test_complete_control_and_LED_bodies_bind_actual_targets_and_partial_output_order(self):
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        objects,rows,_,_=self.fixture()
        expected={'ControlAction':(190,16),'ButtonBinding':(90,24),'ScreenBindings':(312,64),'LedPattern':(200,48)}
        for shape,want in expected.items():
            proof=tail_coverage(objects,_name(objects,ROLES[shape]),rows)
            self.assertEqual((proof['extent'],proof['frame']),want)
            self.assertNotIn('completed_value',proof)
        action=tail_coverage(objects,_name(objects,ROLES['ControlAction']),rows)
        self.assertEqual(action['action_targets'],tuple(0x1001bc64+i for i in
            (72,138,114,126,96,150,162,132,180,108,174,90,102,156,84,120,78,144,168)))
        self.assertNotIn(0x1001bc64+71,action['action_targets'])
        screen=tail_coverage(objects,_name(objects,ROLES['ScreenBindings']),rows)
        self.assertTrue(any(e[0]==250 and e[1]=='b.w' and e[3]==ROLES['OUTLINED_FUNCTION_75'] for e in screen['ordinary_edges']))

    def test_coherent_table_guard_output_and_saved_tail_mutants_reject(self):
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        from test_config_stack import StartupConfigurationCopy
        from llvm_stack import Instruction
        for mutant in ('table','guard','output','tail'):
            objects,rows,_,_=self.fixture()
            shape='ControlAction' if mutant in ('table','guard') else 'ScreenBindings'
            if mutant=='table':StartupConfigurationCopy.put(objects,0x1001bc64+52,b'\x0b')
            else:
                at={'guard':0x1001bc64+44,'output':0x1001bb2c+296,'tail':0x1001bb2c+250}[mutant]
                row=rows[at]
                if mutant=='guard':raw=bytes((row.raw[0]^1,))+row.raw[1:];operands='r0, #0x13';op=row.mnemonic
                elif mutant=='output':raw=row.raw[:2]+b'\x07\x00';operands='r0, [r4, #0x7]';op=row.mnemonic
                else:raw=StartupConfigurationCopy.branch_link(at,0x10028682);operands=hex(0x10028682);op='bl'
                StartupConfigurationCopy.put(objects,at,raw);rows[at]=Instruction(at,raw,op,operands)
            with self.subTest(mutant=mutant),self.assertRaises(AssertionError):
                tail_coverage(objects,_name(objects,ROLES[shape]),rows)

    def test_Action_table_and_Screen_restore_relocate_by_current_typed_relationships(self):
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        from test_config_stack import ActualDecodedViews
        objects,rows,_,_=self.fixture()
        ActualDecodedViews.move_function(objects,rows,ROLES['ControlAction'],0x10071000)
        proof=tail_coverage(objects,_name(objects,ROLES['ControlAction']),rows)
        self.assertEqual(proof['action_targets'][18],0x10071000+168)
        ActualDecodedViews.move_function(objects,rows,ROLES['ScreenBindings'],0x10072000)
        ActualDecodedViews.move_function(objects,rows,'OUTLINED_FUNCTION_75',0x10073000)
        screen=tail_coverage(objects,_name(objects,ROLES['ScreenBindings']),rows)
        self.assertTrue(any(e[2]==0x10073000 and e[3]==ROLES['OUTLINED_FUNCTION_75'] for e in screen['ordinary_edges']))
        self.assertTrue(any(e[2]==0x10071000 and e[3]==ROLES['ControlAction'] for e in screen['ordinary_edges']))


class TailStructuralInputs(unittest.TestCase):
    def test_repeated_current_coverage_reuses_structure_but_copies_returned_proofs(self):
        import copy
        from unittest.mock import patch
        import config_tail_children as tail
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        from config_structural import _detached_inputs
        from config_decoded_sources import _phase_graph
        objects,rows,_,_=TailLiteralBodies.fixture();objects,rows=_detached_inputs(objects,rows)
        name=_name(objects,ROLES['ControlAction'])
        with patch.object(tail,'audit_coverage',wraps=tail.audit_coverage) as audit:
            first=tail.tail_coverage(objects,name,rows)
            again=tail.tail_coverage(objects,name,rows,outline_role=None)
            self.assertEqual(first,again)
            self.assertEqual(_phase_graph(first),_phase_graph(again))
            self.assertIsNot(first,again)
            self.assertEqual(audit.call_count,1,'same detached Tail input must not repeat full-map ingress work')
            saved=copy.deepcopy(again);first['objects']['injected']='bad'
            self.assertEqual(tail.tail_coverage(objects=objects,name=name,rows=rows),saved)

    def test_exact_nine_APIs_and_context_keys_cannot_borrow_another_package(self):
        from config_structural import _APIS,_detached_inputs
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        original={('config_helpers','helper_body'),('config_helpers','helper_coverage'),
            ('config_logger','logger_body'),('config_refill_children','child_body'),
            ('config_refill_children','child_coverage'),('config_utf8','utf8_body'),('config_widget_children','widget_body')}
        sources=original|{('config_source_children','source_coverage')}
        self.assertEqual(_APIS,sources|{('config_tail_children','tail_coverage')})
        objects,rows,_,_=TailLiteralBodies.fixture();objects,rows=_detached_inputs(objects,rows)
        name=_name(objects,ROLES['ControlAction']);tail_coverage(objects,name,rows)
        for args in ((objects,name,dict(rows)),(objects,'unknown',rows),(objects,name,rows,'@outline:unknown'),
                     (objects,name,rows,ROLES['OUTLINED_FUNCTION_75'])):
            with self.subTest(context=args[1:2]),self.assertRaises(AssertionError):tail_coverage(*args)

    def test_old_snapshot_detaches_and_new_consumed_table_and_frame_reject(self):
        from config_structural import _detached_inputs
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        from test_config_stack import StartupConfigurationCopy
        for mutation in ('table','frame'):
            objects,rows,_,_=TailLiteralBodies.fixture();name=_name(objects,ROLES['ControlAction'])
            frozen,current=_detached_inputs(objects,rows);proof=tail_coverage(frozen,name,current)
            if mutation=='table':StartupConfigurationCopy.put(objects,0x1001bc64+52,b'\x0b')
            else:objects.sizes[0x1001bc64]=24
            self.assertEqual(tail_coverage(frozen,name,current),proof)
            fresh,new_rows=_detached_inputs(objects,rows)
            with self.subTest(mutation=mutation),self.assertRaises(AssertionError):tail_coverage(fresh,name,new_rows)

    def test_unsupported_reader_rechecks_consumed_non_table_body_bytes(self):
        from config_stack import DiagnosticObjects
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        from config_structural import _detached_inputs
        class Custom(DiagnosticObjects):
            reads=0;changed=False
            def read_only(self,address,extent):
                self.reads+=1;raw=super().read_only(address,extent)
                if self.changed and address<=0x1001bc64+44<address+extent:
                    at=0x1001bc64+44-address;raw=raw[:at]+bytes((raw[at]^1,))+raw[at+1:]
                return raw
        objects,rows,_,_=TailLiteralBodies.fixture();custom=Custom(objects.elf,objects.sizes)
        self.assertIs(_detached_inputs(custom,rows)[0],custom)
        name=_name(custom,ROLES['ControlAction']);first=tail_coverage(custom,name,rows);reads=custom.reads
        self.assertEqual(tail_coverage(custom,name,rows),first);self.assertGreater(custom.reads,reads)
        custom.changed=True
        with self.assertRaises(AssertionError):tail_coverage(custom,name,rows)


class TailChildReturns(unittest.TestCase):
    @staticmethod
    def evaluate(shape,mutation=None):
        from config_tail_children import tail_coverage,tail_children,tail_return_profiles
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        from config_finite_returns import _closed_child_returns
        from config_structural import _detached_inputs
        from config_owner_entry import _direct_target
        objects,rows,_,_=TailLiteralBodies.fixture();objects,rows=_detached_inputs(objects,rows)
        proof=tail_coverage(objects,_name(objects,ROLES[shape]),rows)
        off={'ScreenBindings':5712,'LedPattern':6792}[shape];site=0x1001c654+off
        assert rows[site].mnemonic=='bl' and _direct_target(rows[site],'bl')==proof['function']
        parent=dict(function=0x1001c654,ordinary_edges=((off,'bl',proof['function'],ROLES[shape],proof['frame']),))
        children={(proof['function'],proof['role']):proof,**tail_children(objects,rows,(proof,))}
        if mutation:mutation(children)
        return _closed_child_returns(rows,(parent,),children,0x20070000,tail_return_profiles(),
            (ROLES['ControlAction'],ROLES['ButtonBinding'],ROLES['LedPattern']),objects=objects)

    def test_all_Action_arms_nested_Buttons_Screen_return_original_PC_and_callee_values(self):
        from config_tail_contracts import ROLES
        nodes=self.evaluate('ScreenBindings')
        screen=next(n for n in nodes if n['role']==ROLES['ScreenBindings'])
        self.assertEqual((screen['return_PC'],screen['entry_SP'],screen['return_SP'],screen['lowest_SP']),
            (0x1001c654+5716,0x20070000,0x20070000,0x20070000-152))
        self.assertEqual(screen['preserved_high_gprs'],tuple('r'+str(i) for i in range(4,12)))
        actions=[n for n in nodes if n['role']==ROLES['ControlAction']]
        self.assertTrue(actions)
        self.assertTrue(all(len(n['returns'])==21 for n in actions))
        self.assertTrue(all(n['preserved_high_gprs']==tuple('r'+str(i) for i in range(4,12)) for n in actions))
        self.assertFalse(screen['architectural_LR_preserved'])

    def test_LED_scratch_pop_restores_actual_original_high_saves_and_PC(self):
        from config_tail_contracts import ROLES
        nodes=self.evaluate('LedPattern');led=next(n for n in nodes if n['role']==ROLES['LedPattern'])
        self.assertEqual((led['return_PC'],led['return_SP'],led['lowest_SP']),
            (0x1001c654+6796,0x20070000,0x20070000-96))
        self.assertEqual(led['preserved_high_gprs'],tuple('r'+str(i) for i in range(4,12)))
        self.assertFalse(led['architectural_LR_preserved'])

    def test_private_changed_Action_target_vector_cannot_authorize_return_walk(self):
        from config_tail_contracts import ROLES
        def changed(children):
            key=(0x1001bc64,ROLES['ControlAction'])
            children[key]=dict(children[key],action_targets=children[key]['action_targets'][:-1])
        with self.assertRaisesRegex(AssertionError,'guarded.*(Action|table)'):
            self.evaluate('ScreenBindings',changed)


class TailActionInterface(unittest.TestCase):
    @staticmethod
    def fixture(count=2,extent=32):
        from config_abstract import DecoderInterface,SequenceInterface,Span
        from config_regions import Storage,borrow
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        objects,rows,_,_=TailLiteralBodies.fixture();phase='supplied_Tail_Action'
        visitor=Storage('stack:Tail','supplied Visitor',0x20070000,1608,8,phase,True)
        loader=Storage(visitor.space,'supplied loader',visitor.start+1608,456,8,phase,True)
        screen=Storage(visitor.space,'supplied Screen',visitor.start-64,64,8,phase,True)
        button=Storage(visitor.space,'supplied Button',screen.start-24,24,8,phase,True)
        action=Storage(visitor.space,'supplied Action',button.start-16,16,8,phase,True)
        payload=Storage('address','supplied readonly input',0x10032b2a,extent,1,phase,False)
        decoder=DecoderInterface(borrow(loader,236,8,phase,True),borrow(payload,0,extent,phase),Span(0,0))
        state=SequenceInterface(borrow(button,0,8,phase,True),decoder,Span(count,count))
        saved=Storage(visitor.space,'supplied Button original saves',button.start+8,16,4,phase,False)
        proof=tail_coverage(objects,_name(objects,ROLES['ControlAction']),rows)
        return objects,rows,proof,state,action,(saved,),0x1001bd22+22

    def test_Action_source_domain_missing_and_error_origins_are_distinct(self):
        from config_tail_effects import control_action_interface
        result=control_action_interface(*self.fixture())
        self.assertEqual(result['normal']['domain'],(0,18))
        self.assertEqual(result['normal']['after'].count.lower,1)
        self.assertEqual(result['normal']['after'].decoder.cursor.lower,1)
        self.assertEqual(result['normal']['mapping'],tuple((i,t) for i,t in enumerate(
            (72,138,114,126,96,150,162,132,180,108,174,90,102,156,84,120,78,144,168))))
        invalid=next(e for e in result['errors'] if e['stage']=='Action domain')
        self.assertEqual((invalid['error'],invalid['status'],invalid['child_outcome']['kind']),(14,1,'success'))
        self.assertIs(invalid['origin']['before'],result['before'].decoder)
        none=control_action_interface(*self.fixture(count=0))
        self.assertIsNone(none['normal']);self.assertEqual((none['none']['status'],none['none']['action_low']),(0,255))
        self.assertEqual(none['primitive_invocations'],());self.assertEqual(none['none']['after'].count.lower,0)

    def test_warmed_structure_does_not_cache_EOF_source_phase_or_protection(self):
        from config_tail_effects import control_action_interface
        from config_structural import _detached_inputs
        from config_abstract import Span
        from config_regions import Storage
        args=list(self.fixture());args[0],args[1]=_detached_inputs(args[0],args[1])
        result=control_action_interface(*args);self.assertIsNotNone(result['normal'])
        old=args[3];args[3]=replace(old,decoder=replace(old.decoder,cursor=Span(32,32)))
        eof=control_action_interface(*args);self.assertIsNone(eof['normal'])
        self.assertEqual(tuple(e['error'] for e in eof['errors']),(4,))
        args[3]=old;frame=args[4]
        args[5]=args[5]+(Storage('address','physical deepest child overlap',frame.start-48,1,1,frame.phase,False),)
        with self.assertRaisesRegex(AssertionError,'overlap|protected'):
            control_action_interface(*args)

    def test_wrong_current_caller_receiver_and_body_proof_reject(self):
        from config_tail_effects import control_action_interface
        from config_regions import borrow
        args=list(self.fixture());before=args[3]
        args[3]=replace(before,header=borrow(before.header.storage,4,8,before.header.storage.phase,True))
        with self.assertRaises(AssertionError):control_action_interface(*args)
        args=list(self.fixture());args[-1]+=2
        with self.assertRaises(AssertionError):control_action_interface(*args)
        args=list(self.fixture());args[2]=dict(args[2],action_targets=())
        with self.assertRaises(AssertionError):control_action_interface(*args)


class TailButtonInterface(unittest.TestCase):
    @staticmethod
    def fixture(count=4,extent=32):
        from config_abstract import SequenceInterface,Span
        from config_regions import Storage,borrow
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        objects,rows,_,action,_,_,_=TailActionInterface.fixture(extent=extent)
        button=action.header.storage;screen=Storage(button.space,'supplied Screen',button.start+24,64,8,button.phase,True)
        sequence=SequenceInterface(borrow(screen,20,8,screen.phase,True),action.decoder,Span(count,count))
        saved=Storage(screen.space,'Screen original saves',screen.start+28,36,4,screen.phase,False)
        proof=tail_coverage(objects,_name(objects,ROLES['ButtonBinding']),rows)
        return objects,rows,proof,sequence,button,(saved,),0x1001bb2c+30

    def test_tap_then_hold_keep_distinct_same_source_origins_and_packed_domains(self):
        from config_tail_effects import button_binding_interface
        result=button_binding_interface(*self.fixture())
        normal=result['normal'];self.assertIsNotNone(normal)
        self.assertEqual(normal['fields'],(('tap',(0,18)),('hold',(0,18))))
        self.assertEqual((normal['after'].count.lower,normal['after'].decoder.cursor.lower),(3,2))
        self.assertEqual(tuple(v['site'] for v in result['invocations']),(0x1001bd22+22,0x1001bd22+48))
        first,second=result['invocations']
        self.assertIs(second['before'],first['result']['normal']['after'])
        self.assertIs(second['predecessor'],first)
        self.assertEqual(normal['upper_bits'],'opaque; only low16 carry tap/hold')
        self.assertTrue(any(e['stage']=='hold' and e['cause']['origin']['caller']==0x1001bd22+48 for e in result['errors']))

    def test_short_hold_and_zero_outer_count_never_make_packed_success(self):
        from config_tail_effects import button_binding_interface
        short=button_binding_interface(*self.fixture(extent=1))
        self.assertIsNone(short['normal']);self.assertTrue(any(e['stage']=='hold' and e['error']==4 for e in short['errors']))
        missing=button_binding_interface(*self.fixture(count=0))
        self.assertIsNone(missing['normal']);self.assertEqual(missing['none']['low'],255)
        self.assertEqual(missing['invocations'],())

    def test_wrong_screen_Q_or_ancestor_alias_rejects_with_structure_warm(self):
        from config_tail_effects import button_binding_interface
        from config_structural import _detached_inputs
        from config_regions import Storage,borrow
        args=list(self.fixture());args[0],args[1]=_detached_inputs(args[0],args[1])
        self.assertIsNotNone(button_binding_interface(*args)['normal'])
        before=args[3];args[3]=replace(before,header=borrow(before.header.storage,12,8,before.header.storage.phase,True))
        with self.assertRaises(AssertionError):button_binding_interface(*args)
        args[3]=before;frame=args[4]
        args[5]=args[5]+(Storage('address','deeper Action alias',frame.start-64,1,1,frame.phase,False),)
        with self.assertRaises(AssertionError):button_binding_interface(*args)


class TailOutputAggregates(unittest.TestCase):
    @staticmethod
    def fixture(shape='ScreenBindings',count=None,extent=64):
        from config_abstract import DecoderInterface,SequenceInterface,Span
        from config_regions import Storage,borrow
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        objects,rows,_,_=TailLiteralBodies.fixture();phase='supplied_Tail_output'
        visitor=Storage('stack:Tail-output','supplied Visitor',0x20070000,1608,8,phase,True)
        loader=Storage(visitor.space,'supplied loader',visitor.start+1608,456,8,phase,True)
        payload=Storage('address','supplied readonly input',0x10032b2a,extent,1,phase,False)
        decoder=DecoderInterface(borrow(loader,236,8,phase,True),borrow(payload,0,extent,phase),Span(0,0))
        q,out,frame,default,site=(1128,940,64,3,5712) if shape=='ScreenBindings' else (1000,1064,48,7,6792)
        state=SequenceInterface(borrow(visitor,q,8,phase,True),decoder,Span(default if count is None else count,default if count is None else count))
        activation=Storage(visitor.space,'supplied '+shape,visitor.start-frame,frame,8,phase,True)
        saved=Storage(visitor.space,'Visitor original saves',visitor.start+1512,96,4,phase,False)
        proof=tail_coverage(objects,_name(objects,ROLES[shape]),rows)
        return objects,rows,proof,state,borrow(visitor,out,10,phase,True),activation,(saved,),0x1001c654+site

    def test_Screen_source_order_domains_late_words_and_original_return(self):
        from config_tail_effects import screen_bindings_interface
        result=screen_bindings_interface(*self.fixture());normal=result['normal']
        self.assertIsNotNone(normal)
        self.assertEqual(normal['source_order'],('buttonA','buttonB','buttonX','buttonY','rotation','click'))
        self.assertEqual(normal['rotation_domain'],(0,3));self.assertEqual(normal['click_domain'],(0,18))
        self.assertEqual(normal['after'].decoder.cursor.lower,10)
        self.assertEqual(normal['after'].count.lower,2)
        self.assertEqual(normal['physical_output_order'],((266,1,1),(268,0,1),(296,6,4),(306,2,4)))
        self.assertEqual(normal['outstanding_after_tag'],((2,8),))
        self.assertEqual(normal['initialized_bytes'],tuple(range(10)))
        self.assertTrue(any(e['stage']=='rotation domain' and e['child_outcome']['kind']=='success' for e in result['errors']))
        self.assertTrue(result['requires_bound_Screen_word_access_policy'])

    def test_LED_Off_still_decodes_all_fields_and_keeps_opaque_padding(self):
        from config_tail_effects import led_pattern_interface
        result=led_pattern_interface(*self.fixture('LedPattern'));normal=result['normal']
        self.assertEqual(normal['fields'],(('kind',(0,3)),('color',(0,65535)),('brightness',(0,255)),('period_ms',(0,65535)),('on_percent',(0,255))))
        self.assertEqual(normal['after'].decoder.cursor.lower,5)
        self.assertEqual(normal['after'].count.lower,6)
        self.assertEqual(normal['initialized_bytes'],(0,2,3,4,6,7,8))
        self.assertEqual(normal['opaque_bytes'],(1,5,9))
        self.assertEqual(normal['physical_output_order'],((184,0,1),(186,6,4),(194,2,4)))
        self.assertEqual(len(result['invocations']),5)
        self.assertTrue(result['requires_bound_LED_word_access_policy'])

    def test_short_missing_or_wrong_epoch_outputs_never_become_complete(self):
        from config_tail_effects import screen_bindings_interface,led_pattern_interface
        from config_regions import borrow
        for shape,interface,short in (('ScreenBindings',screen_bindings_interface,9),('LedPattern',led_pattern_interface,4)):
            with self.subTest(shape=shape):
                result=interface(*self.fixture(shape,extent=short));self.assertIsNone(result['normal'])
                self.assertTrue(result['errors'])
                missing=interface(*self.fixture(shape,count=0));self.assertIsNone(missing['normal'])
                self.assertEqual(missing['none']['tag'],255);self.assertEqual(missing['invocations'],())
                args=list(self.fixture(shape));output=args[4]
                args[4]=borrow(output.storage,output.offset+4,10,output.storage.phase,True)
                with self.assertRaises(AssertionError):interface(*args)

    def test_Screen_receiver_output_and_deep_physical_guard_recheck_after_warmup(self):
        from config_tail_effects import screen_bindings_interface
        from config_structural import _detached_inputs
        from config_regions import Storage
        args=list(self.fixture());args[0],args[1]=_detached_inputs(args[0],args[1])
        self.assertIsNotNone(screen_bindings_interface(*args)['normal'])
        frame=args[5];guard=Storage('address','deepest descendant boundary',frame.start-88,1,1,frame.phase,False)
        args[6]=args[6]+(guard,)
        with self.assertRaises(AssertionError):screen_bindings_interface(*args)

    def test_outer_None_still_protects_pushed_Screen_LED_frames_but_has_no_nested_child(self):
        from config_tail_effects import screen_bindings_interface,led_pattern_interface
        from config_regions import Storage
        for shape,interface in (('ScreenBindings',screen_bindings_interface),('LedPattern',led_pattern_interface)):
            with self.subTest(shape=shape):
                args=list(self.fixture(shape,count=0));frame=args[5]
                original=args[6]
                args[6]=original+(Storage('address','actual pushed None save',frame.start+frame.extent-1,1,1,frame.phase,False),)
                with self.assertRaises(AssertionError):interface(*args)
                args[6]=original+(Storage('address','below inactive None child',frame.start-1,1,1,frame.phase,False),)
                result=interface(*args);self.assertEqual(result['none'].get('physical_child_bytes'),frame.extent)
                self.assertEqual(result['none']['ordinary_return']['return_SP'],frame.start+frame.extent)

    def test_LED_unwritten_padding_guard_survives_containing_output_envelope(self):
        from config_tail_effects import led_pattern_interface
        from config_regions import Storage
        args=list(self.fixture('LedPattern'));output=args[4];frame=args[5];original=args[6]
        pad=Storage(frame.space,'protected old LED padding',output.start+1,1,1,frame.phase,False)
        args[6]=original+(pad,)
        result=led_pattern_interface(*args);self.assertIsNotNone(result['normal'])
        self.assertIn(pad,result['protected'])
        for offset in (0,2,5,9):
            args[6]=original+(replace(pad,start=output.start+offset),)
            with self.subTest(offset=offset),self.assertRaises(AssertionError):led_pattern_interface(*args)


class TailParentBodies(unittest.TestCase):
    def test_all_parent_windows_bind_complete_tail_calls_same_B_and_restore(self):
        from config_decoded_tail import _parents
        objects,rows,_,_=TailLiteralBodies.fixture();V=0x1001c654;B=0x2000031c
        parents=_parents(objects,rows,V,B)
        self.assertEqual(tuple(p['function']-V for p in parents),(170,5630,5922,6018,6412,6450,7000,56))
        edges=tuple((p['function']+e[0]-V,e[3]) for p in parents for e in p['ordinary_edges'])
        from config_tail_contracts import ROLES
        self.assertEqual(tuple(at for at,role in edges if role==ROLES['ScreenBindings']),(5712,6198,6426))
        self.assertEqual(tuple(at for at,role in edges if role==ROLES['LedPattern']),(6792,6828))
        self.assertIn((68,ROLES['OUTLINED_FUNCTION_155']),edges)
        self.assertFalse(any(p['function']<=V+7122<p['function']+p['extent'] for p in parents))
        with self.assertRaisesRegex(AssertionError,'same.*Config'):_parents(objects,rows,V,B+4)

    def test_coherent_early_success_late_store_and_original_restore_changes_reject(self):
        from config_decoded_tail import _parents
        from test_config_stack import StartupConfigurationCopy
        from llvm_stack import Instruction
        for off in (7100,7108,68):
            objects,rows,_,_=TailLiteralBodies.fixture();V=0x1001c654;at=V+off;row=rows[at]
            if off==68:
                raw=StartupConfigurationCopy.branch_link(at,0x100289f0);op='bl';args=hex(0x100289f0)
            else:raw=bytes.fromhex('00bf')*(len(row.raw)//2);op='nop' if len(raw)==2 else 'nop.w';args=''
            StartupConfigurationCopy.put(objects,at,raw);rows[at]=Instruction(at,raw,op,args)
            with self.subTest(off=off),self.assertRaises(AssertionError):_parents(objects,rows,V,0x2000031c)


class TailPrivateHandoff(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from config_decoded_sources import _first_boot_decoded_sources_state
        objects,rows,source,_=TailLiteralBodies.fixture(72)
        _,cls.state=_first_boot_decoded_sources_state(objects,rows,source)
        cls.objects=cls.state.phases.objects;cls.rows=cls.state.phases.rows

    def test_actual_Source_exit_initializes_no_Tail_fields_and_retires_only_saved7(self):
        from config_decoded_tail import _tail_entries
        from config_abstract import Span
        entries=_tail_entries(self.objects,self.rows,self.state)
        self.assertTrue(entries);self.assertEqual(len(entries),len(self.state.exits))
        for entry,source in zip(entries,self.state.exits):
            frame=self.state.phases.widgets.views.context.headers['visitor_frame'];V=0x1001c654
            self.assertIs(entry.exit,source);self.assertIs(entry.sequence,source.sequence)
            self.assertEqual(entry.sequence.count,Span(4,4));self.assertEqual(entry.frontier,V+5630)
            self.assertEqual(entry.r4_pointer,(frame.start+1064,V+5538,V+5578))
            self.assertEqual((entry.second_screen_pointer[0].start,entry.second_screen_pointer[1]),(frame.start+216,frame.start+1001))
            self.assertTrue(any(v.start==frame.start+216 and v.extent==4 for v in entry.protected))
            self.assertFalse(any(v.space==frame.space and v.start==frame.start+280 and v.extent==4 for v in entry.protected))
            self.assertEqual(entry.retired_saved_count[1:3],(7,V+5052))
            self.assertIsNone(entry.Controls_sequence)
            self.assertEqual((entry.old_controls.offset,entry.old_controls.extent),(212,32))
            self.assertFalse(entry.completed_ordinary_group_issued)

    def test_copied_public_receipt_or_cloned_Source_exit_is_not_handoff(self):
        from config_decoded_tail import _tail_entries
        with self.assertRaises(AssertionError):_tail_entries(self.objects,self.rows,dict(Controls_ready=True))
        first=self.state.exits[0]
        changed=replace(self.state,exits=(replace(first,source_normal=dict(first.source_normal)),)+self.state.exits[1:])
        with self.assertRaisesRegex(AssertionError,'outgoing state'):_tail_entries(self.objects,self.rows,changed)
        with self.assertRaises(AssertionError):_tail_entries(self.objects,dict(self.rows),self.state)


if __name__=='__main__':unittest.main()
