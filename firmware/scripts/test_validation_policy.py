"""Corrupted source and accidentally accepted invalid profiles must fail at -O."""
from pathlib import Path
import runpy
import subprocess
import unittest
from unittest.mock import patch
import check_vendor
import check_memory_layout
import hashlib
import io
import json
import struct
import tempfile
import zlib
from types import SimpleNamespace
from contextlib import redirect_stdout


class ValidationPolicy(unittest.TestCase):
    def test_configuration_stack_budget_has_exact_64k_boundary(self):
        for headroom in (-1, 0, 32768, 65535):
            with self.assertRaises(AssertionError):
                check_memory_layout.check_headroom(headroom)
        for headroom in (65536, 65544, 131072):
            check_memory_layout.check_headroom(headroom)

    def test_modified_vendor_source_is_rejected(self):
        original = Path.read_bytes
        changed = check_vendor.ROOT / 'embassy-rp/src/lib.rs'
        with patch.object(Path, 'read_bytes', lambda p: original(p) + (b'changed' if p == changed else b'')):
            with self.assertRaisesRegex(AssertionError, 'vendor mismatch'):
                check_vendor.check()

    def test_invalid_feature_compile_success_is_rejected(self):
        accepted = subprocess.CompletedProcess([], 0, '', '')
        catalogue=json.dumps({'profiles':[{'profile':{'feature':'fixture_'+str(i)}} for i in range(8)]}).encode()
        with patch('subprocess.check_output',return_value=catalogue),patch('subprocess.run', return_value=accepted):
            with self.assertRaises(AssertionError):
                runpy.run_path(str(Path(__file__).with_name('check_feature_profiles.py')), run_name='__main__')


class MemoryQualification(unittest.TestCase):
    def setUp(self):
        self.stdout=io.StringIO();self.enterContext(redirect_stdout(self.stdout))

    def fixture(self):
        from config_platform import PlatformPolicy,qualified_stack_result
        payload=b'current payload';eav=b'EAV1'+struct.pack('<HH',4,len(payload))+bytes(4)+payload
        eav=eav[:8]+struct.pack('<I',zlib.crc32(eav[:8]+payload))+payload
        data=(0,1,3,0x20000000,len(eav),128,0,0,4,0)
        bss=(0,8,3,0x20000080,0,128,0,0,4,0);uninit=(0,8,3,0x20000100,0,256,0,0,4,0)
        flash=(0,1,2,0x10000000,0,len(eav),0,0,1,0)
        elf=SimpleNamespace(data=eav+bytes(128),sections=[data,bss,uninit,flash],segments=[],
            named={'.data':data,'.bss':bss,'.uninit':uninit,'.end_block':flash},
            symbols=dict(__euninit=0x20000200,__sheap=0x20000200,_stack_end=0x20000200,
                         _stack_start=0x20080000,SIO_IRQ_FIFO=0x20000101,sio_fifo_ram=0x20000109))
        policy=PlatformPolicy(0x1000000c,0x10000000+len(eav),hashlib.sha256(payload).hexdigest(),hashlib.sha256(elf.data).hexdigest())
        stack=qualified_stack_result(dict(boot=4096,constructor=4096,runtime=8192,dashboard=16384),0x7fe00,policy)
        return elf,eav,stack

    def owned_fixture(self):
        # A deliberately synthetic full-image consumer fixture. Only the
        # declared source producer is mocked; readers really check policy,
        # geometry, exact instruction sites and current ELF/EAV identities.
        from config_platform import PlatformPolicy,qualified_stack_result
        from config_owned_access import OwnedAccess
        from config_regions import Storage,borrow
        from config_headers import ROOT
        from config_entry import CONFIG
        _,eav,_=self.fixture();advanced=0x10001000;loader=0x10002000
        code=bytearray(0x1000+1200)
        code[84:88]=bytes.fromhex('c4f83900')
        config_anchor=loader+32;widget_anchor=loader+256
        code[config_anchor+28-advanced:config_anchor+32-advanced]=bytes.fromhex('48f80010')
        code[widget_anchor+670-advanced:widget_anchor+674-advanced]=bytes.fromhex('c4f85209')
        data_offset=len(eav)+len(code);raw=eav+code+bytes(14604)
        flash=(0,1,2,0x10000000,0,len(eav),0,0,1,0)
        text=(0,1,6,advanced,len(eav),len(code),0,0,2,0)
        data=(0,1,7,0x20000000,data_offset,14604,0,0,4,0)
        bss=(0,8,3,0x2000390c,0,128,0,0,4,0)
        uninit=(0,8,3,0x2000398c,0,256,0,0,4,0)
        role='_RNvMs2_NtCs_16dashboard_config8advancedNtB5_15AdvancedDisplay11for_channel'
        elf=SimpleNamespace(data=bytes(raw),sections=[flash,text,data,bss,uninit],segments=[],
            named={'.data':data,'.bss':bss,'.uninit':uninit,'.end_block':flash},
            symbols=dict(__euninit=0x20003a8c,__sheap=0x20003a90,_stack_end=0x20003a90,
                _stack_start=0x20080000,SIO_IRQ_FIFO=0x2000398d,sio_fifo_ram=0x20003995),
            functions={role:(advanced|1,104,1),ROOT:(loader|1,1200,1)},
            symbol_entries=[(CONFIG,0x20000000,14604,1,2)])
        frame=Storage('stack:configuration.default','loader',0,456,8,'default_refill',True)
        guards=(Storage(frame.space,'loader:saved',352,104,4,frame.phase,False),
            Storage(frame.space,'original fault',8,16,4,frame.phase,False))
        slot=Storage('address','private unpublished ConfigSlot',0x20000000,14604,4,frame.phase,True)
        dependencies=(OwnedAccess('configuration_versions',config_anchor,config_anchor+28,'default',
            borrow(slot,0,14576,frame.phase,writable=True),frame,'r8',guards),
            OwnedAccess('advanced_default_fields',advanced,advanced+84,'widget',
            borrow(frame,236,64,frame.phase,writable=True),frame,'r4',guards))+tuple(
            OwnedAccess('widget_colors',widget_anchor,widget_anchor+670,'widget'+str(i),
                borrow(slot,444+220*i,220,frame.phase,writable=True),frame,'r4',guards,i) for i in range(8))
        policy=PlatformPolicy(0x1000000c,0x10000000+len(eav),hashlib.sha256(eav[12:]).hexdigest(),
            hashlib.sha256(elf.data).hexdigest(),owned_dependencies=dependencies,owned_family='sizez_default_refill')
        headroom=0x20080000-0x20003a90
        stack=qualified_stack_result(dict(boot=4096,constructor=4096,runtime=8192,dashboard=16384),headroom,policy)
        return elf,eav,stack

    def test_actual_image_reader_preserves_owned_family_and_rejects_lost_context(self):
        from copy import deepcopy
        elf,eav,stack=self.owned_fixture()
        with patch.object(check_memory_layout,'ElfImage',return_value=elf),\
             patch.object(check_memory_layout,'check_stack_budget',return_value=stack),patch.object(check_memory_layout,'check_flash_lockout'):
            record=check_memory_layout.check('synthetic-owned.elf')
        self.assertEqual(check_memory_layout.read_image_result(json.loads(json.dumps(record)),elf,eav),record)
        policy=record['stack']['policies'][0]
        self.assertEqual((policy['schema'],policy['owned_family'],len(policy['owned_dependencies'])),(4,'sizez_default_refill',10))
        for fault in ('missing','empty','one_lost','duplicate','old','receiver','pair','phase','operation','width'):
            changed=deepcopy(record);p=changed['stack']['policies'][0]
            if fault=='missing':p.pop('owned_dependencies')
            elif fault=='empty':p['owned_dependencies']=[]
            elif fault=='one_lost':p['owned_dependencies'].pop()
            elif fault=='duplicate':p['owned_dependencies'][-1]=deepcopy(p['owned_dependencies'][-2])
            elif fault=='old':p['scope']='h2-rp2350-secure-owned-access-v2'
            elif fault=='receiver':p['owned_dependencies'][1]['receiver_start']+=4
            elif fault=='pair':p['owned_dependencies'][-1]['loop_index']=0
            elif fault=='phase':p['owned_dependencies'][0]['owner']['storage']['phase']='published'
            elif fault=='operation':p['owned_dependencies'][0]['operation']='STREX'
            else:p['owned_dependencies'][0]['width']=8
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                check_memory_layout.read_image_result(changed,elf,eav)

    def test_actual_memory_path_returns_record_only_after_all_checks(self):
        elf,eav,stack=self.fixture();events=[]
        with patch.object(check_memory_layout,'ElfImage',return_value=elf),patch.object(check_memory_layout,'check_stack_budget',return_value=stack),\
             patch.object(check_memory_layout,'check_flash_lockout',side_effect=lambda e:events.append('flash')):
            result=check_memory_layout.check('fixture.elf')
        self.assertEqual(events,['flash'])
        self.assertEqual(result['status'],'CONDITIONAL_IMAGE_PASS')
        self.assertFalse(result['hardware_qualified'])
        self.assertEqual(check_memory_layout.read_image_result(result,elf,eav),result)
        self.assertIn('CONDITIONAL_IMAGE_PASS, assumed_external, hardware_qualified=false',self.stdout.getvalue())

    def test_missing_lost_stale_and_intermediate_stack_result_reject(self):
        from copy import deepcopy
        elf,eav,stack=self.fixture()
        cases=[None,dict(stack,status='STACK_PASS'),dict(stack,headroom=65536),dict(stack,hardware_qualified=True)]
        for field,value in (('elf_sha256','0'*64),('input_sha256','0'*64),('input_end',0x10000010),
                            ('target','other'),('security','nonsecure'),('core',1)):
            changed=deepcopy(stack);changed['policies'][0][field]=value;cases.append(changed)
        for value in cases:
            with self.subTest(value=value),patch.object(check_memory_layout,'ElfImage',return_value=elf),\
                 patch.object(check_memory_layout,'check_stack_budget',return_value=value),patch.object(check_memory_layout,'check_flash_lockout'):
                with self.assertRaises(AssertionError):check_memory_layout.check('fixture.elf')

    def test_failed_later_gate_does_not_return_ready_record(self):
        elf,eav,stack=self.fixture()
        with patch.object(check_memory_layout,'ElfImage',return_value=elf),patch.object(check_memory_layout,'check_stack_budget',return_value=stack),\
             patch.object(check_memory_layout,'check_flash_lockout',side_effect=AssertionError('flash failed')):
            with self.assertRaisesRegex(AssertionError,'flash failed'):check_memory_layout.check('fixture.elf')

    def test_cli_writes_only_complete_success_and_refuses_stale_output(self):
        elf,eav,stack=self.fixture()
        with tempfile.TemporaryDirectory() as folder:
            target=Path(folder)/'result.json'
            with patch.object(check_memory_layout,'ElfImage',return_value=elf),patch.object(check_memory_layout,'check_stack_budget',return_value=stack),\
                 patch.object(check_memory_layout,'check_flash_lockout',side_effect=AssertionError('late failure')):
                with self.assertRaises(AssertionError):check_memory_layout.main(['fixture.elf','--result-json',str(target)])
            self.assertFalse(target.exists())
            with patch.object(check_memory_layout,'ElfImage',return_value=elf),patch.object(check_memory_layout,'check_stack_budget',return_value=stack),\
                 patch.object(check_memory_layout,'check_flash_lockout'):
                check_memory_layout.main(['fixture.elf','--result-json',str(target)])
            self.assertEqual(json.loads(target.read_text())['status'],'CONDITIONAL_IMAGE_PASS')
            with patch.object(check_memory_layout,'check') as gate,self.assertRaises(AssertionError):
                check_memory_layout.main(['fixture.elf','--result-json',str(target)])
            gate.assert_not_called()

    def test_input_policy_rejects_partial_allocated_alias_and_wrapped_owner(self):
        for fault in ('alias','wrapped_owner'):
            elf,eav,stack=self.fixture()
            if fault=='alias':elf.sections.append((0,8,3,0x1000000d,0,2,0,0,1,0))
            else:
                owner=list(elf.sections[-1]);owner[5]=0xffffffff;elf.sections[-1]=tuple(owner)
            with self.subTest(fault=fault),patch.object(check_memory_layout,'ElfImage',return_value=elf),\
                 patch.object(check_memory_layout,'check_stack_budget',return_value=stack),patch.object(check_memory_layout,'check_flash_lockout'),\
                 self.assertRaises(AssertionError):check_memory_layout.check('fixture.elf')

    def test_reader_rejects_stack_only_record_and_changed_image_input_or_status(self):
        from copy import deepcopy
        elf,eav,stack=self.fixture()
        with patch.object(check_memory_layout,'ElfImage',return_value=elf),patch.object(check_memory_layout,'check_stack_budget',return_value=stack),\
             patch.object(check_memory_layout,'check_flash_lockout'):
            result=check_memory_layout.check('fixture.elf')
        bad=deepcopy(result);bad['stack']['platform_policy']='established'
        for value in (None,stack,bad,dict(result,status='IMAGE_PASS'),dict(result,headroom=65536)):
            with self.subTest(value=value),self.assertRaises(AssertionError):check_memory_layout.read_image_result(value,elf,eav)
        with self.assertRaises(AssertionError):check_memory_layout.read_image_result(result,elf,eav+b'changed')
        elf.data+=b'changed'
        with self.assertRaises(AssertionError):check_memory_layout.read_image_result(result,elf,eav)


if __name__ == '__main__':
    unittest.main()
