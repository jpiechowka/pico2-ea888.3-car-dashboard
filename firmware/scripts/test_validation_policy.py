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


def supplied_stack_record(bounds,headroom,policy,elf,source_context=None):
    """Explicit synthetic stage facts for strict real readers, not analysis."""
    from config_platform import _execution_receipt
    if source_context is None:
        source_context=MemoryQualification.source_context(MemoryQualification(),elf)
    elf.fixture_source_context=source_context
    return dict(schema=2,status='CONDITIONAL_STACK_PASS',platform_policy='assumed_external',hardware_qualified=False,
        stack_bounds=dict(bounds),headroom=headroom,policies=[policy.to_record()],
        execution_scope=_execution_receipt(policy,source_context,elf).to_record())


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
        stack=supplied_stack_record(dict(boot=4096,constructor=4096,runtime=8192,dashboard=16384),0x7fe00,policy,elf)
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
        stack=supplied_stack_record(dict(boot=4096,constructor=4096,runtime=8192,dashboard=16384),headroom,policy,elf)
        return elf,eav,stack

    def test_actual_image_reader_preserves_owned_family_and_rejects_lost_context(self):
        from copy import deepcopy
        elf,eav,stack=self.owned_fixture()
        with patch.object(check_memory_layout,'ElfImage',return_value=elf),\
             patch.object(check_memory_layout,'check_stack_budget',return_value=stack),patch.object(check_memory_layout,'check_flash_lockout'):
            record=check_memory_layout.check('synthetic-owned.elf',source_context=elf.fixture_source_context)
        self.assertEqual(check_memory_layout.read_image_result(json.loads(json.dumps(record)),elf,eav,elf.fixture_source_context),record)
        policy=record['stack']['policies'][0]
        self.assertEqual((policy['schema'],policy['owned_family'],len(policy['owned_dependencies'])),(7,'sizez_default_refill',10))
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
                check_memory_layout.read_image_result(changed,elf,eav,elf.fixture_source_context)

    def test_actual_memory_path_returns_record_only_after_all_checks(self):
        elf,eav,stack=self.fixture();events=[]
        with patch.object(check_memory_layout,'ElfImage',return_value=elf),patch.object(check_memory_layout,'check_stack_budget',return_value=stack),\
             patch.object(check_memory_layout,'check_flash_lockout',side_effect=lambda e:events.append('flash')):
            result=check_memory_layout.check('fixture.elf',source_context=elf.fixture_source_context)
        self.assertEqual(events,['flash'])
        self.assertEqual(result['status'],'CONDITIONAL_IMAGE_PASS')
        self.assertFalse(result['hardware_qualified'])
        self.assertEqual(check_memory_layout.read_image_result(result,elf,eav,elf.fixture_source_context),result)
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
                with self.assertRaises(AssertionError):check_memory_layout.check('fixture.elf',source_context=elf.fixture_source_context)

    def test_failed_later_gate_does_not_return_ready_record(self):
        elf,eav,stack=self.fixture()
        with patch.object(check_memory_layout,'ElfImage',return_value=elf),patch.object(check_memory_layout,'check_stack_budget',return_value=stack),\
             patch.object(check_memory_layout,'check_flash_lockout',side_effect=AssertionError('flash failed')):
            with self.assertRaisesRegex(AssertionError,'flash failed'):check_memory_layout.check('fixture.elf',source_context=elf.fixture_source_context)

    def test_cli_writes_only_complete_success_and_refuses_stale_output(self):
        elf,eav,stack=self.fixture()
        with tempfile.TemporaryDirectory() as folder:
            target=Path(folder)/'result.json'
            from config_build_source import write_source_context
            source=Path(folder)/'source-context.json';write_source_context(source,elf.fixture_source_context)
            with patch.object(check_memory_layout,'ElfImage',return_value=elf),patch.object(check_memory_layout,'check_stack_budget',return_value=stack),\
                 patch.object(check_memory_layout,'check_flash_lockout',side_effect=AssertionError('late failure')):
                with self.assertRaises(AssertionError):check_memory_layout.main(['fixture.elf','--gate','experimental-full','--result-json',str(target),'--source-context',str(source)])
            self.assertFalse(target.exists())
            with patch.object(check_memory_layout,'ElfImage',return_value=elf),patch.object(check_memory_layout,'check_stack_budget',return_value=stack),\
                 patch.object(check_memory_layout,'check_flash_lockout'):
                check_memory_layout.main(['fixture.elf','--gate','experimental-full','--result-json',str(target),'--source-context',str(source)])
            self.assertEqual(json.loads(target.read_text())['status'],'CONDITIONAL_IMAGE_PASS')
            with patch.object(check_memory_layout,'check') as gate,self.assertRaises(AssertionError):
                check_memory_layout.main(['fixture.elf','--gate','experimental-full','--result-json',str(target),'--source-context',str(source)])
            gate.assert_not_called()

    def test_input_policy_rejects_partial_allocated_alias_and_wrapped_owner(self):
        for fault in ('alias','wrapped_owner'):
            elf,eav,stack=self.fixture()
            if fault=='alias':elf.sections.append((0,8,3,0x1000000d,0,2,0,0,1,0))
            else:
                owner=list(elf.sections[-1]);owner[5]=0xffffffff;elf.sections[-1]=tuple(owner)
            with self.subTest(fault=fault),patch.object(check_memory_layout,'ElfImage',return_value=elf),\
                 patch.object(check_memory_layout,'check_stack_budget',return_value=stack),patch.object(check_memory_layout,'check_flash_lockout'),\
                 self.assertRaises(AssertionError):check_memory_layout.check('fixture.elf',source_context=elf.fixture_source_context)

    def test_reader_rejects_stack_only_record_and_changed_image_input_or_status(self):
        from copy import deepcopy
        elf,eav,stack=self.fixture()
        with patch.object(check_memory_layout,'ElfImage',return_value=elf),patch.object(check_memory_layout,'check_stack_budget',return_value=stack),\
             patch.object(check_memory_layout,'check_flash_lockout'):
            result=check_memory_layout.check('fixture.elf',source_context=elf.fixture_source_context)
        bad=deepcopy(result);bad['stack']['platform_policy']='established'
        for value in (None,stack,bad,dict(result,status='IMAGE_PASS'),dict(result,headroom=65536)):
            with self.subTest(value=value),self.assertRaises(AssertionError):check_memory_layout.read_image_result(value,elf,eav,elf.fixture_source_context)
        with self.assertRaises(AssertionError):check_memory_layout.read_image_result(result,elf,eav+b'changed',elf.fixture_source_context)
        elf.data+=b'changed'
        with self.assertRaises(AssertionError):check_memory_layout.read_image_result(result,elf,eav,elf.fixture_source_context)

    def source_context(self,elf):
        from config_build_source import SOURCE_PATHS,make_source_context
        root=Path(__file__).resolve().parents[2]
        return make_source_context(elf.data,'supplied-source-fixture',b'{}',
            {name:(root/name).read_bytes() for name in SOURCE_PATHS})

    def test_explicit_source_context_flows_through_memory_stack_and_result_reader(self):
        elf,eav,stack=self.fixture();value=self.source_context(elf)
        with patch.object(check_memory_layout,'ElfImage',return_value=elf),patch.object(
                check_memory_layout,'check_stack_budget',return_value=stack) as child,patch.object(check_memory_layout,'check_flash_lockout'):
            record=check_memory_layout.check('supplied.elf',source_context=value)
        child.assert_called_once_with(elf,0x7fe00,source_context=value)
        self.assertEqual(check_memory_layout.read_image_result(record,elf,eav,source_context=value),record)
        self.assertNotIn('source_context',record)  # unchanged memory schema
        from dataclasses import replace
        wrong=replace(value,elf_sha256='0'*64)
        with self.assertRaisesRegex(AssertionError,'different current ELF'):
            check_memory_layout.read_image_result(record,elf,eav,source_context=wrong)
        import check_stack_budget
        with self.assertRaisesRegex(AssertionError,'different current ELF'):
            check_stack_budget.check(elf,0x7fe00,source_context=wrong)
        # Only the narrower access reader remains source independent.
        with self.assertRaisesRegex(AssertionError,'explicit source'):
            check_memory_layout.read_image_result(record,elf,eav)
        from config_platform import current_image_policy
        self.assertEqual(current_image_policy(stack['policies'][0],elf,eav).to_record(),stack['policies'][0])

    def test_CLI_requires_explicit_source_path_and_rejects_bad_pair_before_output(self):
        from config_build_source import write_source_context
        from dataclasses import replace
        elf,eav,stack=self.fixture();value=self.source_context(elf)
        for explicit,bad in ((True,False),(True,True),(False,True)):
            with self.subTest(explicit=explicit,bad=bad),tempfile.TemporaryDirectory() as folder:
                source=Path(folder)/'source-context.json';output=Path(folder)/'memory.json'
                write_source_context(source,replace(value,elf_sha256='0'*64) if bad else value)
                args=['fixture.elf','--gate','experimental-full','--gate','experimental-full','--result-json',str(output)]
                if explicit:args+=['--source-context',str(source)]
                with patch.object(check_memory_layout,'ElfImage',return_value=elf),patch.object(
                        check_memory_layout,'check_stack_budget',return_value=stack),patch.object(check_memory_layout,'check_flash_lockout'):
                    if not explicit or bad:
                        with self.assertRaises(AssertionError):check_memory_layout.main(args)
                        self.assertFalse(output.exists())
                    else:
                        check_memory_layout.main(args);self.assertEqual(json.loads(output.read_bytes())['gate'],'full_memory_layout')


class NamedOrdinaryExecution(unittest.TestCase):
    @staticmethod
    def fixture():
        from config_platform import current_image_policy
        policy_test=MemoryQualification()
        elf,eav,stack=policy_test.fixture()
        return elf,current_image_policy(stack['policies'][0],elf,eav),policy_test.source_context(elf)

    def test_receipt_binds_current_ELF_input_and_exact_source_attempt(self):
        import config_platform as platform
        from dataclasses import replace
        elf,policy,source=self.fixture()
        receipt=platform._execution_receipt(policy,source,elf)
        record=receipt.to_record()
        self.assertEqual(platform.read_execution_scope(record,policy,source,elf=elf),receipt)
        self.assertEqual(record['schema'],1)
        self.assertEqual(record['scope_sha256'],'b55723348ef4ffab36786a763f1c0d8035a516a416bb79aa30e2cae6143fd9eb')
        self.assertEqual(record['exception_graph_limit'],['NMI','HardFault','reset_reentry'])
        self.assertFalse(record['unconditional_owner'])
        self.assertFalse(record['exception_safe_completion'])
        with self.assertRaisesRegex(AssertionError,'execution scope.*identity'):
            platform.read_execution_scope(record,policy,replace(source,attempt=source.attempt+'-other'),elf=elf)
        # Normative execution1 table, independently stated here. Hashes bind
        # this actual current fixture, not placeholder admission constants.
        self.assertEqual(record,dict(schema=1,kind='named_ordinary_execution',scope='h2-named-ordinary-continuation-v1',
            scope_sha256='b55723348ef4ffab36786a763f1c0d8035a516a416bb79aa30e2cae6143fd9eb',
            domain='named_ordinary_and_callback_paths',elf_sha256=hashlib.sha256(elf.data).hexdigest(),
            input_sha256=hashlib.sha256(elf.data[12:12+len(b'current payload')]).hexdigest(),
            source_context_sha256=hashlib.sha256(source.to_bytes()).hexdigest(),all_observers_compatible=False,
            exception_absence_established=False,exception_safe_completion=False,unconditional_owner=False,hardware_qualified=False,
            exception_graph_limit=['NMI','HardFault','reset_reentry'],pending_exception_frontier=[
                'routing_and_stack_selection_unproved','architectural_entry_memory_and_nested_transitions_unproved',
                'reset_reentry_requires_fresh_qualified_epoch'],external_qualifications=[
                'exact expected physical entry and successful ordinary accesses',
                'interrupt/DMA/core quiescence and exclusive-memory behavior conditional, not measured',
                'deprecated Rust pre_init source issue remains Maintenance M2; emitted byte proof does not define Rust pre-init'],
            requires_H8_consumed_input_closure=True,requires_M2_pre_init_source_reconciliation=True))

    def test_strict_receipt_rejects_lost_frontier_schema_aliases_and_stronger_use(self):
        import config_platform as platform
        from copy import deepcopy
        elf,policy,source=self.fixture();receipt=platform._execution_receipt(policy,source,elf)
        record=receipt.to_record()
        for fault in ('schema_bool','schema_float','missing','extra','scope','domain','elf','input','source',
                      'frontier_order','frontier_lost','exception_limit','hardware','observers','exception','owner','H8','M2'):
            bad=deepcopy(record)
            if fault=='schema_bool':bad['schema']=True
            elif fault=='schema_float':bad['schema']=1.0
            elif fault=='missing':bad.pop('pending_exception_frontier')
            elif fault=='extra':bad['approved']=True
            elif fault in ('elf','input','source'):bad[{'elf':'elf_sha256','input':'input_sha256','source':'source_context_sha256'}[fault]]='0'*64
            elif fault=='scope':bad['scope_sha256']='0'*64
            elif fault=='domain':bad['domain']='all_observers'
            elif fault=='frontier_order':bad['pending_exception_frontier'].reverse()
            elif fault=='frontier_lost':bad['pending_exception_frontier'].pop()
            elif fault=='exception_limit':bad['exception_graph_limit'].pop()
            else:bad[{'hardware':'hardware_qualified','observers':'all_observers_compatible','exception':'exception_safe_completion',
                      'owner':'unconditional_owner','H8':'requires_H8_consumed_input_closure','M2':'requires_M2_pre_init_source_reconciliation'}[fault]]=False if fault in ('H8','M2') else True
            with self.subTest(fault=fault),self.assertRaisesRegex(AssertionError,'execution scope'):
                platform.read_execution_scope(bad,policy,source,elf=elf)
        with self.assertRaisesRegex(AssertionError,'exception-safe'):
            platform.require_exception_safe_execution(receipt)
        with self.assertRaisesRegex(AssertionError,'global ownership'):
            platform.require_global_execution_owner(receipt)
        # Even a forged typed instance is revalidated before canonical hashing.
        from config_build_source import SourceContext
        forged=object.__new__(SourceContext)
        for key,value in source.__dict__.items():object.__setattr__(forged,key,value)
        object.__setattr__(forged,'sources',())
        with self.assertRaisesRegex(AssertionError,'ordered source'):
            platform.read_execution_scope(record,policy,forged,elf=elf)


if __name__ == '__main__':
    unittest.main()
