"""Focused Tail checks using retained machine rows and explicit abstract inputs."""
import unittest
from dataclasses import replace


class TailLiteralBodies(unittest.TestCase):
    @staticmethod
    def fixture():
        import json
        from pathlib import Path
        from test_config_sources import SourceLiteralBodies
        from test_config_stack import StartupConfigurationCopy,SameBLoaderBoundary
        from llvm_stack import Instruction
        objects,rows,source,item=SourceLiteralBodies.fixture()
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


if __name__=='__main__':unittest.main()
