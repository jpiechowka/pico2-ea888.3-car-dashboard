"""Pinned decoder framing and mapping regressions; target code is never run."""
import copy
from pathlib import Path
import tempfile
import unittest
import sys
import struct
import time
import hashlib
import ctypes
import os
import subprocess
from unittest.mock import patch
import llvm_stack
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

    def test_nonlocal_typed_and_nonzero_mapping_symbols_reject(self):
        elf=self.fixture()
        offset=elf.sections[2][4]+3*16
        for field,value in [('info',16),('info',2),('size',2)]:
            data=bytearray(elf.data)
            if field=='info':data[offset+12]=value
            else:struct.pack_into('<I',data,offset+8,value)
            path=Path(tempfile.mktemp(suffix='.elf'))
            self.addCleanup(path.unlink,missing_ok=True)
            path.write_bytes(data)
            with self.assertRaisesRegex(AssertionError,'mapping symbol'):
                ElfImage(path)

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

    def test_inherited_pipe_cannot_extend_decoder_deadline(self):
        with tempfile.TemporaryDirectory() as folder:
            marker=Path(folder)/'finished'
            script='import time; from pathlib import Path; time.sleep(1); Path('+repr(str(marker))+').write_text("done")'
            parent='import subprocess,sys; subprocess.Popen([sys.executable,"-c",'+repr(script)+'])'
            started=time.monotonic()
            try:
                with self.assertRaisesRegex(AssertionError,'deadline'):
                    _run([sys.executable,'-c',parent],128,timeout=.2)
                self.assertLess(time.monotonic()-started,.7)
            finally:
                # The fixture's only descendant ends itself; keep its lifetime
                # owned and wait for its completion before removing test files.
                deadline=time.monotonic()+3
                while not marker.exists() and time.monotonic()<deadline:time.sleep(.01)
                self.assertTrue(marker.exists(),'fixture descendant did not complete')

    def test_pipe_setup_failure_reaps_child_and_closes_both_pipes(self):
        # Exercise the real owned child. Only the failing platform setup is
        # injected; leaving setup outside cleanup makes these assertions fail.
        original=subprocess.Popen
        platforms=[('posix',os,'set_blocking')]
        # WinDLL/msvcrt is native Windows setup; Linux verifies its own POSIX
        # setup owner without pretending that Windows imports are available.
        if os.name=='nt':platforms.insert(0,('nt',ctypes,'WinDLL'))
        for platform,owner,attribute in platforms:
            children=[]
            def spawn(*args,**kwargs):
                child=original(*args,**kwargs)
                children.append(child)
                return child
            with self.subTest(platform=platform):
                try:
                    with patch.object(llvm_stack.subprocess,'Popen',side_effect=spawn), \
                         patch.object(llvm_stack.os,'name',platform), \
                         patch.object(owner,attribute,side_effect=OSError('injected pipe setup failure'),create=True):
                        with self.assertRaisesRegex(OSError,'injected pipe setup failure'):
                            _run([sys.executable,'-c','import time; time.sleep(30)'],128,timeout=1)
                    self.assertEqual(len(children),1)
                    child=children[0]
                    self.assertIsNotNone(child.poll(),'owned child survived setup failure')
                    self.assertTrue(child.stdout.closed,'stdout survived setup failure')
                    self.assertTrue(child.stderr.closed,'stderr survived setup failure')
                finally:
                    # RED also owns its cleanup: never leave its failed child.
                    for child in children:
                        if child.poll() is None:child.kill()
                        child.wait(timeout=3)
                        child.stdout.close();child.stderr.close()

    def test_compiler_change_during_decoder_execution_rejects(self):
        elf=self.fixture()
        with tempfile.TemporaryDirectory() as folder:
            source=Path(folder)/'source.elf';source.write_bytes(elf.data)
            compiler=Path(folder)/'rustc';compiler.write_bytes(b'compiler-before')
            tool=Path(folder)/'objdump';tool.write_bytes(b'decoder')
            context=llvm_stack.LlvmContext(1,str(compiler),hashlib.sha256(compiler.read_bytes()).hexdigest(),
                'reviewed compiler',folder,'host',str(tool),hashlib.sha256(tool.read_bytes()).hexdigest(),
                'reviewed decoder',llvm_stack.TARGET,llvm_stack.DECODER_ARGS,'fixture','output',
                hashlib.sha256(Path(llvm_stack.__file__).read_bytes()).hexdigest())
            # Isolate the identity boundary with a real byte-checked leaf. The
            # external invocation is the only mocked dependency and changes the
            # compiler bytes while it runs.
            elf.functions={'tail':elf.functions['tail']}
            def output(*args):
                compiler.write_bytes(b'compiler-after')
                return '10000024: e7ec b 0x10000000 <probe>\n10000026: 4770 bx lr\n'
            with patch.object(llvm_stack,'_run',side_effect=output):
                with self.assertRaisesRegex(AssertionError,'changed during admission'):
                    llvm_stack.disassemble(source,elf,context)


if __name__=='__main__': unittest.main()
