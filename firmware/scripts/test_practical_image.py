"""Practical producer and gate separation; synthetic ELF, no hardware.

Only exact flash instruction checking is replaced in this fixture. Program
map/boot block/EAV linkage, source contract, named frames and budgets run.
"""
from contextlib import redirect_stdout
from copy import deepcopy
from dataclasses import replace
import io,json,struct,tempfile,unittest,zlib
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import practical_image as practical
import check_memory_layout as image
from check_stack_budget import frames,FrameAdmission
from config_build_source import make_source_context,SOURCE_PATHS,write_source_context
import test_stack_budget


def fixture():
    B=0x10000000
    data=bytearray(4096)
    data[0x100:0x114]=struct.pack('<5I',0xffffded3,0x10210142,0x000001ff,0,0xab123579)
    payload=b'bounded fixture payload';eav=b'EAV1'+struct.pack('<HH',4,len(payload))+bytes(4)+payload
    eav=eav[:8]+struct.pack('<I',zlib.crc32(eav[:8]+payload))+payload
    data[512:512+len(eav)]=eav
    functions,sizes=test_stack_budget.StackBudget().slot_fixture()
    names=['TaskStorage4pollembassy_main_task','TaskStorage4polldashboard_task_task',
           'TaskStorage4pollacquisition_task_task','pico2view_configinitialize',
           'pico2publish_snapshot','AcquisitionOwner8snapshot','AcquisitionState8snapshot']
    for index,name in enumerate(names):
        address=B+2048+index*4;functions[name]=(address|1,4,1);sizes[address]=64
    metadata=bytearray()
    for address,size in sizes.items():
        metadata.extend(struct.pack('<I',address))
        while size>=128:metadata.append((size&127)|128);size>>=7
        metadata.append(size)
    data.extend(metadata)
    section=lambda kind,flags,address,offset,size:(0,kind,flags,address,offset,size,0,0,4,0)
    entries=[(1,'.start_block',section(1,2,B+256,256,20)),
             (2,'.dashboard_config',section(1,2,B+512,512,len(eav))),
             (3,'.data',section(1,3,0x20000000,768,128)),
             (4,'.bss',section(8,3,0x20000080,0,128)),
             (5,'.uninit',section(8,3,0x20000100,0,256)),
             (6,'.end_block',section(1,2,B+1024,1024,20)),
             (7,'.stack_sizes',section(1,0,0,4096,len(metadata)))]
    symbols=dict(__euninit=0x20000200,__sheap=0x20000200,_stack_end=0x20000200,
        _stack_start=0x20080000,SIO_IRQ_FIFO=0x20000101,sio_fifo_ram=0x20000109)
    elf=SimpleNamespace(data=bytes(data),sections=[s for _,_,s in entries],section_entries=entries,
        named={n:s for _,n,s in entries},symbols=symbols,functions=functions,
        segments=[(1,0,B,B,4096,4096,5,4)],symbol_entries=[
            ('DASHBOARD_EMBEDDED_CONFIG',B+512,len(eav),0x11,2),
            ('__dashboard_config_start',B+512,0,0x10,2),
            ('__dashboard_config_end',B+512+len(eav),0,0x10,2)])
    elf.section_bytes=lambda name:elf.data[elf.named[name][4]:elf.named[name][4]+elf.named[name][5]]
    repo=Path(__file__).resolve().parents[2]
    context=make_source_context(elf.data,'practical-fixture',b'{}',{name:(repo/name).read_bytes() for name in SOURCE_PATHS})
    return elf,eav,context


class PracticalImage(unittest.TestCase):
    def setUp(self):
        self.enterContext(redirect_stdout(io.StringIO()))
        self.flash=self.enterContext(patch.object(image,'check_flash_lockout'))

    def test_producer_rechecks_named_frames_geometry_source_and_linked_eav(self):
        elf,eav,context=fixture();record=practical.check(elf,eav,context)
        self.assertEqual(record['status'],'STATIC_IMAGE_CHECKS_PASS')
        self.assertEqual(record['schema'],3)
        self.assertFalse(record['complete_instruction_proof'])
        self.assertNotIn('execution_scope',record)
        self.assertEqual(record['stack']['core0_reserve'],8192)
        self.assertEqual(record['stack']['core1_reserve'],4096)
        self.assertEqual(image.read_selected_image_result(record,elf,eav,context),record)
        self.assertEqual(self.flash.call_count,2)
        for key in ('boot','constructor','runtime','dashboard'):
            self.assertGreater(record['stack']['bounds'][key],0)

    def test_mutated_receipt_gate_scope_bounds_and_identities_reject(self):
        elf,eav,context=fixture();record=practical.check(elf,eav,context)
        changes=[('schema',True),('schema',2),('gate','full_memory_layout'),
                 ('status','CONDITIONAL_IMAGE_PASS'),('hardware_qualified',True),
                 ('complete_instruction_proof',0),('elf_sha256','0'*64),
                 ('config_sha256','0'*64),('source_context_sha256','0'*64),
                 ('static_ram_bytes',0),('headroom',999999),('assumptions',[]),('limitations',[])]
        for key,value in changes:
            bad=deepcopy(record);bad[key]=value
            with self.subTest(key=key),self.assertRaises(AssertionError):
                image.read_selected_image_result(bad,elf,eav,context)
        for key in ('boot','constructor','runtime','dashboard'):
            bad=deepcopy(record);bad['stack']['bounds'][key]-=1
            with self.assertRaises(AssertionError):image.read_selected_image_result(bad,elf,eav,context)
        bad=deepcopy(record);bad['execution_scope']={}
        with self.assertRaises(AssertionError):image.read_selected_image_result(bad,elf,eav,context)
        with self.assertRaises(AssertionError):image.read_selected_image_result(record,elf,eav,context,'experimental-full')
        with self.assertRaises(AssertionError):image.read_selected_image_result(record,elf,eav,context,'unknown')

    def test_actual_inputs_and_required_checks_fail_closed(self):
        for fault in ('eav','source','geometry','frame','flash','load','envelope'):
            elf,eav,context=fixture()
            if fault=='eav':eav+=b'changed'
            elif fault=='source':context=replace(context,elf_sha256='0'*64)
            elif fault=='geometry':elf.symbols['_stack_start']-=8
            elif fault=='frame':del elf.functions['TaskStorage4polldashboard_task_task']
            elif fault=='flash':self.flash.side_effect=AssertionError('flash failed')
            elif fault=='load':elf.segments.append((1,0,0x101fe000,0x101fe000,1,1,5,1))
            elif fault=='envelope':
                raw=bytearray(elf.data);raw[512+8]^=1;elf.data=bytes(raw)
                context=replace(context,elf_sha256=practical.hashlib.sha256(elf.data).hexdigest())
            with self.subTest(fault=fault),self.assertRaises(AssertionError):practical.check(elf,eav,context)
            self.flash.side_effect=None

    def test_capture_default_and_frozen_result_recheck(self):
        import capture_target_bench as capture
        elf,eav,context=fixture()
        with tempfile.TemporaryDirectory() as folder:
            destination=Path(folder);context=replace(context,attempt=destination.name)
            source=destination/'source.json';write_source_context(source,context)
            frozen=destination/'pico2.elf';frozen.write_bytes(elf.data)
            result=practical.check(elf,eav,context)
            def child(command,**kwargs):
                self.assertEqual(command[command.index('--gate')+1],'practical')
                self.assertTrue(kwargs['check'])
                Path(command[command.index('--result-json')+1]).write_text(json.dumps(result))
            with patch.object(capture,'ElfImage',return_value=elf),patch.object(capture.subprocess,'run',side_effect=child):
                checked=capture.run_memory_gate(frozen,destination,eav,context,source)
                self.assertEqual(checked['record'],result)
                with self.assertRaises(AssertionError):
                    capture.read_memory_gate(destination/'memory-layout.json',frozen,eav,context,'experimental-full')
                result['stack']['bounds']['boot']=0
                (destination/'memory-layout.json').write_text(json.dumps(result))
                with self.assertRaises(AssertionError):
                    capture.read_memory_gate(destination/'memory-layout.json',frozen,eav,context)

    def test_cli_default_practical_and_explicit_experimental_never_downgrades(self):
        elf,eav,context=fixture()
        with tempfile.TemporaryDirectory() as folder,patch.object(image,'ElfImage',return_value=elf):
            source=Path(folder)/'source.json';write_source_context(source,context)
            output=Path(folder)/'practical.json'
            image.main(['fixture.elf','--source-context',str(source),'--result-json',str(output)])
            self.assertEqual(json.loads(output.read_bytes())['gate'],'practical_image')
            experimental=Path(folder)/'full.json'
            with self.assertRaisesRegex(AssertionError,'no retained stack policy'):
                image.main(['fixture.elf','--gate','experimental-full','--source-context',str(source),'--result-json',str(experimental)])
            self.assertFalse(experimental.exists())

if __name__=='__main__':unittest.main()
