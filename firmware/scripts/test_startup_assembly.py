"""Focused current assembly contract regressions; historical captures stay intact."""
import json
from pathlib import Path
import struct
from types import SimpleNamespace
import unittest
from llvm_stack import Instruction
from config_stack import DiagnosticObjects, typed_function
from config_startup import assembly_pre_init, startup_configuration_copy
import test_config_stack as historical


class AssemblyStartup(unittest.TestCase):
    def fixture(self, address=0x10000198):
        f=json.loads((Path(__file__).parent/'fixtures/config-startup-assembly.json').read_text())
        old=f['function'][0]&~1;raw=bytes.fromhex(f['body_hex']);rows={}
        for r in f['records']:
            a=address+r['address']-old;operand=r['operands']
            if r['mnemonic']=='bne':operand=hex(int(operand,16)+address-old)
            rows[a]=Instruction(a,bytes.fromhex(r['bytes']),r['mnemonic'],operand)
        elf=SimpleNamespace(data=raw,sections=[(0,1,6,address,0,len(raw),0,0,4,0)],
            functions={'__pre_init':(address|1,42,0)},symbol_entries=[('__pre_init',address|1,42,18,0)],
            mapping_symbols=[('$t',address,0),('$d',address+44,0)])
        return DiagnosticObjects(elf,{}),rows

    def test_current_body_and_relocation_require_no_generic_missing_frame_fallback(self):
        for address in (0x10000198,0x10071000):
            objects,rows=self.fixture(address)
            proof=assembly_pre_init(objects,rows)
            self.assertEqual((proof['extent'],proof['frame']),(42,0))
            self.assertEqual([v[2] for v in proof['external_literals']], [0xd000017c,0x40018004,0xe000e008])
            with self.assertRaisesRegex(AssertionError,'missing typed function frame'):
                typed_function(objects,address,'__pre_init')

    def test_every_code_byte_mutation_is_rejected_even_with_stale_decoder_text(self):
        for offset in range(42):
            objects,rows=self.fixture();raw=bytearray(objects.elf.data);raw[offset]^=1;objects.elf.data=bytes(raw)
            with self.subTest(offset=offset),self.assertRaises(AssertionError):assembly_pre_init(objects,rows)

    def test_all_three_mmio_addresses_are_exact(self):
        for offset in (44,48,52):
            objects,rows=self.fixture();raw=bytearray(objects.elf.data);raw[offset]^=4;objects.elf.data=bytes(raw)
            with self.subTest(offset=offset),self.assertRaisesRegex(AssertionError,'MMIO address'):
                assembly_pre_init(objects,rows)

    def test_metadata_mapping_frame_and_pool_alias_mutations_reject(self):
        for fault in ('extent','symbol_type','symbol_duplicate','thumb','frame','bool_frame','code_mapping','pool_mapping','alias','writable'):
            objects,rows=self.fixture();elf=objects.elf;a=next(iter(rows))
            if fault=='extent':elf.functions['__pre_init']=(a|1,44,0)
            elif fault=='symbol_type':elf.symbol_entries=[('__pre_init',a|1,42,2,0)]
            elif fault=='symbol_duplicate':elf.symbol_entries*=2
            elif fault=='thumb':elf.functions['__pre_init']=(a,42,0)
            elif fault=='frame':objects.sizes[a]=8
            elif fault=='bool_frame':objects.sizes[a]=False
            elif fault=='code_mapping':elf.mapping_symbols.insert(1,('$d',a+20,0))
            elif fault=='pool_mapping':elf.mapping_symbols=[('$t',a,0)]
            elif fault=='alias':elf.functions['other']=(a+45,4,0)
            else:elf.sections=[(0,1,7,a,0,56,0,0,4,0)]
            with self.subTest(fault=fault),self.assertRaises(AssertionError):assembly_pre_init(objects,rows)

    def test_same_bytes_with_wrong_return_or_branch_text_reject(self):
        for offset,op,args in ((40,'bx','r0'),(16,'bne','0x100001ae'),(8,'lsls','r1, r1, #0x17')):
            objects,rows=self.fixture();a=next(iter(rows))+offset;r=rows[a]
            rows[a]=Instruction(a,r.raw,op,args)
            with self.subTest(offset=offset),self.assertRaises(AssertionError):assembly_pre_init(objects,rows)

    def install(self,objects,rows):
        base=historical.StartupConfigurationCopy();elf=objects.elf
        fresh,newrows=self.fixture(0x10071000);address=0x10071000;index=len(elf.sections)
        offset=len(elf.data);elf.data+=fresh.elf.data
        elf.sections.append((0,1,6,address,offset,56,0,0,4,0))
        old=elf.functions['__pre_init'][0]&~1
        elf.functions['__pre_init']=(address|1,42,index)
        elf.symbol_entries=[(n,address|1,42,18,index) if n=='__pre_init' else (n,a,z,t,ix) for n,a,z,t,ix in elf.symbol_entries]
        elf.mapping_symbols.extend([('$t',address,index),('$d',address+44,index)])
        objects.sizes.pop(old,None);rows.update(newrows)
        reset=elf.functions['Reset'][0]&~1;encoded=base.branch_link(reset,address)
        base.put(objects,reset,encoded);rows[reset]=Instruction(reset,encoded,'bl',hex(address))
        return reset

    def test_current_hook_composes_with_unchanged_Reset_copy_contract(self):
        for mode in ('speed2','sizes','sizez'):
            base=historical.StartupConfigurationCopy();objects,rows,_=base.fixture(mode);elf=objects.elf
            reset=self.install(objects,rows)
            proof=startup_configuration_copy(objects,rows)
            self.assertFalse(proof['requires_M2_pre_init_source_reconciliation'])
            self.assertEqual(proof['pre_init']['frame'],0)
            self.assertEqual(proof['copy']['pre_init_stack'],(proof['copy']['initial_sp'],)*2)
            self.assertEqual(proof['reset']['ordinary_edges'][0][4],0)

    def test_current_hook_preserves_complete_selected_history_and_closed_LR(self):
        from config_boot_history import first_boot_history
        from config_build_source import make_source_context
        for mode in ('speed2','sizes','sizez'):
            objects,rows,context=historical.FirstBootHistory().fixture(mode)
            self.install(objects,rows)
            context=make_source_context(objects.elf.data,context.attempt,context.build_options,dict(context.sources))
            proof=first_boot_history(objects,rows,context)
            self.assertFalse(proof['requires_M2_pre_init_source_reconciliation'])
            hook=next(v for v in proof['selected_returns'] if v['phase']=='pre_init')
            self.assertIsNone(hook['saved_LR'])
            self.assertEqual(hook['entry_SP'],hook['return_SP'])



if __name__=='__main__':unittest.main()
