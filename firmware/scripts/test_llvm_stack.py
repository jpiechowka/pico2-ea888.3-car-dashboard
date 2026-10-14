"""Pinned decoder framing and mapping regressions; target code is never run."""
import copy
from pathlib import Path
import tempfile
import unittest
import sys
from llvm_stack import capability_elf, decode_listing, mapping_ranges, _run
from elf_image import ElfImage


class Decoder(unittest.TestCase):
    def fixture(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        path = Path(folder.name)/'probe.elf'
        path.write_bytes(capability_elf())
        return ElfImage(path)

    def test_duplicate_thumb_mapping_names_and_literal_interval_are_retained(self):
        elf = self.fixture()
        self.assertEqual([n for n,_,_ in elf.mapping_symbols], ['$t','$d','$t'])
        self.assertEqual(mapping_ranges(elf, 'probe'), [(0x10000000,0x10000020,'t')])
        self.assertEqual(mapping_ranges(elf, 'tail'), [(0x10000024,0x10000028,'t')])

    def test_raw_instruction_bytes_coverage_and_literal_data(self):
        elf = self.fixture()
        rows = ['10000000: f000 f810 bl 0x10000024 <tail>',
                '10000004: d100 bne 0x10000008 <probe+8>',
                '10000006: bf08 it eq', '10000008: bd10 popeq {r4, pc}',
                '1000000a: ed2d 8b10 vpush {d8, d9, d10, d11, d12, d13, d14, d15}',
                '1000000e: ecbd 8b10 vpop {d8, d9, d10, d11, d12, d13, d14, d15}',
                '10000012: eeb4 8a40 vcmp.f32 s16, s0',
                '10000016: eef1 fa10 vmrs APSR_nzcv, fpscr',
                '1000001a: 4798 blx r3', '1000001c: 4770 bx lr', '1000001e: bf00 nop',
                '10000020: 00 f0 00 f8 .word 0xf800f000',
                '10000024: e7ec b 0x10000000 <probe>', '10000026: 4770 bx lr']
        listing='\n'.join(rows)
        decoded=decode_listing(listing,elf)
        self.assertEqual(decoded[0x10000000].raw, bytes.fromhex('00f010f8'))
        self.assertNotIn(0x10000020,decoded)
        self.assertEqual(len(decoded),13)
        for changed in [listing.replace('f810','f811'), listing.replace(rows[0]+'\n',''),
                        listing+'\n'+rows[0], listing.replace('vpush','<unknown>'),
                        listing.replace('vpush','.inst')]:
            with self.subTest(changed=changed[:55]), self.assertRaises(AssertionError):
                decode_listing(changed,elf)

    def test_missing_conflicting_arm_and_hidden_code_mappings_reject(self):
        elf=self.fixture()
        for entries in [[],[('$a',0x10000000,1)],
                        [('$t',0x10000000,1),('$d',0x10000000,1)],
                        [('$t',0x10000001,1)], [('$d',0x10000000,1)]]:
            bad=copy.deepcopy(elf);bad.mapping_symbols=entries
            with self.assertRaises(AssertionError): mapping_ranges(bad,'probe')

    def test_alias_extents_must_agree_and_overlapping_owners_reject(self):
        elf=self.fixture()
        for function in [(0x10000001,30,1),(0x10000005,4,1)]:
            bad=copy.deepcopy(elf);bad.functions['other']=function
            with self.assertRaisesRegex(AssertionError,'conflicting|overlapping'):
                decode_listing('',bad)

    def test_child_diagnostics_output_bounds_and_deadline_are_fail_closed(self):
        self.assertEqual(_run([sys.executable,'-c','print("ok")'],128), 'ok\n')
        for script in ['import sys; sys.stderr.write("warning")',
                       'import sys; sys.exit(3)', 'print("x"*4096)']:
            with self.assertRaises(AssertionError):_run([sys.executable,'-c',script],128)
        with self.assertRaisesRegex(AssertionError,'deadline'):
            _run([sys.executable,'-c','import time; time.sleep(5)'],128,timeout=.1)


if __name__=='__main__': unittest.main()
