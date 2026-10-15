"""Finite configuration stack roles and diagnostic root regressions."""
import unittest
from types import SimpleNamespace
import struct
import json
import re
from pathlib import Path
import config_stack
from llvm_stack import Instruction
from config_stack import error_root_role

VALIDATION = '_RNvXsk_NtCshxbT6mSCLYQ_16dashboard_config8validateNtB5_15ValidationErrorNtNtCs6m2Zc6X1WBq_4core3fmt5Debug3fmt'
WIDGET = '_RNvXsc_NtCshxbT6mSCLYQ_16dashboard_config5modelNtB5_6WidgetNtNtCs6m2Zc6X1WBq_4core3fmt5Debug3fmt'
CONTROL = '_RNvXs1g_NtCs123_4core3fmtRNtNtCs456_16dashboard_config8controls12ControlErrorNtB6_5Debug3fmtBA_'
U8 = '_RNvXs1g_NtCs123_4core3fmtRhNtB6_5Debug3fmtCs456_14dashboard_core'


class CompleteFormatBridges(unittest.TestCase):
    def fixture(self, code_shift=0, data_shift=0, fixture_file='config-format.json', mode=None):
        # Literal reviewed raw input bodies; expected grammars live only in the
        # production consumer. No firmware is built/executed by this fixture.
        fixture=json.loads((Path(__file__).parent/'fixtures'/fixture_file).read_text())
        if mode:fixture=[f for f in fixture if f.get('mode')==mode]
        functions={};sizes={};records={};segments=[]
        for f in fixture:
            start=f['address'];body=bytearray.fromhex(f['body_hex']) if 'body_hex' in f else bytearray(f['size']);rows=[]
            for p in f['address_producers']:
                if 'pool' in p:struct.pack_into('<I',body,p['pool']-start,p['address']+data_shift)
            producers={site:p for p in f['address_producers'] if 'low_site' in p for site in (p['low_site'],p['high_site'])}
            for line in f['lines']:
                match=re.match(r'([0-9a-f]+):\s+((?:(?:[0-9a-f]{4})\s+)+)(\S+)\s*(.*)',line)
                address=int(match[1],16);raw=b''.join(int(x,16).to_bytes(2,'little') for x in match[2].split())
                operands=match[4].split('@',1)[0].strip()
                if address in producers:
                    p=producers[address];value=p['address']+data_shift
                    value=(value&0xffff) if address==p['low_site'] else (value>>16)
                    operands=p['register']+', #'+hex(value)
                    first,second=struct.unpack('<HH',raw)
                    raw=struct.pack('<HH',(first&~0x040f)|((value>>12)&15)|((value>>1)&0x400),
                                    (second&~0x70ff)|((value&0x700)<<4)|(value&255))
                elif re.match(r'(?:b\w*|cb\w*)$',match[3].removesuffix('.w')) and '0x' in operands:
                    operands=re.sub(r'0x([0-9a-f]+)',lambda m:hex(int(m[1],16)+code_shift),operands,count=1)
                body[address-start:address-start+len(raw)]=raw
                rows.append(Instruction(address+code_shift,raw,match[3],operands))
            segments.append((start+code_shift,bytes(body)));records[start+code_shift]=rows
            for a in f['aliases']:functions[a['name']]=(a['address']+code_shift,a['size'],0)
            sizes[start+code_shift]=f['frame']
            for p in f['address_producers']:
                object_bytes=bytes.fromhex(p['first24_hex'])
                if 'vtable_words' in p:
                    words=p['vtable_words'];object_bytes=struct.pack('<6I',*words[:3],*(w+code_shift for w in words[3:]))
                elif p.get('targets'):
                    words=struct.unpack('<4I',object_bytes)
                    object_bytes=struct.pack('<4I',*words[:3],words[3]+code_shift)
                segments.append((p['address']+data_shift,object_bytes))
                for target in p.get('targets',[]):
                    for a in target['aliases']:functions[a['name']]=(a['address']+code_shift,a['size'],0)
                    sizes[(target['pointer']&~1)+code_shift]=target['frame']
                    for a in target['aliases']:segments.append(((a['address']&~1)+code_shift,bytes(a['size'])))
            for edge in f['ordinary_edges']:
                for a in edge['aliases']:
                    functions[a['name']]=(a['address']+code_shift,a['size'],0)
                    sizes[(a['address']&~1)+code_shift]=edge['frame']
                    # Only ownership is consumed here; callee body closure is
                    # separately checked by the complete bridge contract.
                    if not any(at<=((a['address']&~1)+code_shift)<at+len(b) for at,b in segments):
                        segments.append(((a['address']&~1)+code_shift,bytes(a['size'])))
        lo=min(a for a,_ in segments);ro=min(p['address']+data_shift for f in fixture for p in f['address_producers'])
        hi=max(a+len(b) for a,b in segments);data=bytearray(hi-lo)
        for a,b in segments:data[a-lo:a-lo+len(b)]=b
        mappings=[('$t',lo,0)]
        for f in fixture:
            mappings.extend(('$'+kind,a+code_shift,0) for a,b,kind in f.get('mapping',[]))
        elf=SimpleNamespace(data=bytes(data),sections=[(0,1,6,lo,0,ro-lo,0,0,2,0),
                (0,1,2,ro,ro-lo,hi-ro,0,0,4,0)],functions=functions,mapping_symbols=mappings)
        return config_stack.DiagnosticObjects(elf,sizes),records,fixture

    def test_independent_code_and_data_relocation_crosses_movw_half_boundary(self):
        for code_shift,data_shift in ((0x100,0x18000),(0x3400,0x21000)):
            objects,records,fixture=self.fixture(code_shift,data_shift)
            for f in fixture:
                result=config_stack.format_bridge(objects,f['aliases'][0]['name'],records[f['address']+code_shift])
                self.assertEqual(result['objects'],tuple(p['address']+data_shift for p in f['address_producers']))

    def test_readonly_delimiters_and_pad_typed_callbacks_reject_wrong_valid_objects(self):
        for change in ('delimiter','slot','header','callback','frame','alias'):
            objects,records,fixture=self.fixture();f=fixture[1]
            data=bytearray(objects.elf.data);section=objects.elf.sections[1]
            p=f['address_producers'][0 if change=='delimiter' else 4]
            at=section[4]+p['address']-section[3]
            if change=='delimiter':data[at]=ord('x')
            if change=='slot':data[at+12:at+16]=data[at+16:at+20]
            if change=='header':struct.pack_into('<I',data,at+4,16)
            if change=='callback':struct.pack_into('<I',data,at+12,p['vtable_words'][3]+2)
            if change=='frame':objects.sizes[p['vtable_words'][3]&~1]=0
            if change=='alias':
                a=p['targets'][0]['aliases'][0];objects.elf.functions[a['name']+'.2']=(a['address'],a['size']+2,0)
            objects.elf.data=bytes(data)
            with self.subTest(change=change),self.assertRaises(AssertionError):
                config_stack.format_bridge(objects,f['aliases'][0]['name'],records[f['address']])

    def test_unused_external_literal_bytes_do_not_define_bridge_identity(self):
        objects,records,fixture=self.fixture();f=fixture[1]
        # Append a separate unused external literal, outside every carried
        # table and consumed delimiter; no whole-section hash is required.
        section=list(objects.elf.sections[1]);section[5]+=1
        objects.elf.sections[1]=tuple(section)
        objects.elf.data+=b'x'
        result=config_stack.format_bridge(objects,f['aliases'][0]['name'],records[f['address']])
        self.assertEqual(result['pretty_sites'],(0xa0,0xd2,0xe4,0xee,0x116))

    def test_complete_current_bridges_inventory_all_transfer_sites(self):
        objects,records,fixture=self.fixture()
        for f in fixture:
            result=config_stack.format_bridge(objects,f['aliases'][0]['name'],records[f['address']])
            self.assertEqual(result['frame'],f['frame'])
            self.assertEqual(result['extent'],f['size'])
        field=config_stack.format_bridge(objects,fixture[1]['aliases'][0]['name'],records[fixture[1]['address']])
        self.assertEqual(field['raw_callbacks'],(0x84,0xee))
        self.assertEqual(field['ordinary_sites'],(0xd2,0xe4))

    def test_bridge_operand_changes_unknown_transfer_and_partial_coverage_reject(self):
        objects,records,fixture=self.fixture()
        for ordinal,offset,mnemonic,operands in [(0,0x54,'str','r1, [sp, #0xc]'),
                (0,0xcc,'ldr','r2, [r1, #0x8]'),(1,0x80,'ldr','r2, [r7, #0xc]'),
                (2,0x2c,'ldr','r1, [r1, #0x10]'),(3,0x4e,'ldr','r2, [r2, #0x10]'),
                (1,0x0a,'blx','r3')]:
            f=fixture[ordinal];rows=list(records[f['address']]);site=f['address']+offset
            index=next(i for i,r in enumerate(rows) if r.address==site)
            old=rows[index];rows[index]=Instruction(old.address,old.raw,mnemonic,operands)
            with self.subTest(ordinal=ordinal,offset=offset),self.assertRaises(AssertionError):
                config_stack.format_bridge(objects,f['aliases'][0]['name'],rows)
        f=fixture[0]
        with self.assertRaises(AssertionError):
            config_stack.format_bridge(objects,f['aliases'][0]['name'],records[f['address']][:-1])


class ValidationBuilder(unittest.TestCase):
    fixture=CompleteFormatBridges.fixture

    def test_complete_five_source_fields_and_reference_cell_all_modes(self):
        for mode in ('speed2','sizes','sizez'):
            objects,records,fixture=self.fixture(fixture_file='config-validation.json',mode=mode);f=fixture[0]
            result=config_stack.validation_builder(objects,f['aliases'][0]['name'],records[f['address']])
            self.assertEqual(result['fields'],(('code',7,'error_code'),('field',8,'field'),
                            ('view',4,'option_u8'),('widget',0,'option_u16'),('channel',6,'option_channel_ref')))
            self.assertEqual(result['channel_reference_cell'],0x34)

    def test_builder_labels_typed_callbacks_and_pool_relocate_independently(self):
        for mode in ('speed2','sizes','sizez'):
            objects,records,fixture=self.fixture(0x800,0x21000,'config-validation.json',mode);f=fixture[0]
            result=config_stack.validation_builder(objects,f['aliases'][0]['name'],records[f['address']+0x800])
            self.assertEqual(len(result['callbacks']),5)

    def test_same_layout_reference_callback_and_pool_pointer_substitution_reject(self):
        for mode in ('speed2','sizez'):
            objects,records,fixture=self.fixture(fixture_file='config-validation.json',mode=mode);f=fixture[0]
            p=next(p for p in f['address_producers'] if p['origin_role']=='option_channel_ref')
            a=p['targets'][0]['aliases'][0]
            del objects.elf.functions[a['name']]
            objects.elf.functions[CONTROL]=(a['address'],a['size'],0)
            with self.subTest(mode=mode),self.assertRaises(AssertionError):
                config_stack.validation_builder(objects,f['aliases'][0]['name'],records[f['address']])
        objects,records,fixture=self.fixture(fixture_file='config-validation.json',mode='sizez');f=fixture[0]
        first=f['address_producers'][0];wrong=next(p for p in f['address_producers'] if p['origin_role']=='widget')
        data=bytearray(objects.elf.data);section=objects.elf.sections[0]
        struct.pack_into('<I',data,first['pool']-section[3],wrong['address']);objects.elf.data=bytes(data)
        with self.assertRaises(AssertionError):
            config_stack.validation_builder(objects,f['aliases'][0]['name'],records[f['address']])

    def test_builder_swapped_cells_changed_origin_and_popped_owner_reject(self):
        for mode,offset,mnemonic,operands in [('speed2',0x38,'adds','r3, r0, #0x8'),
                ('speed2',0x62,'strd','r1, r5, [sp, #16]'),('speed2',0x88,'mov','r0, r0'),
                ('sizez',0x2a,'adds','r1, r0, #0x6'),('sizez',0x44,'str','r1, [sp, #0x4]'),
                ('sizez',0x4c,'add','sp, #0x34')]:
            objects,records,fixture=self.fixture(fixture_file='config-validation.json',mode=mode);f=fixture[0]
            rows=list(records[f['address']]);index=next(i for i,r in enumerate(rows) if r.address==f['address']+offset)
            old=rows[index];rows[index]=Instruction(old.address,old.raw,mnemonic,operands)
            with self.subTest(mode=mode,offset=offset),self.assertRaises(AssertionError):
                config_stack.validation_builder(objects,f['aliases'][0]['name'],rows)


class NumericSupport(unittest.TestCase):
    fixture=CompleteFormatBridges.fixture

    def test_complete_numeric_bodies_all_modes_retain_default_and_excluded_edges(self):
        import config_numeric
        for mode in ('speed2','sizes','sizez'):
            objects,records,fixture=self.fixture(fixture_file='config-numeric.json',mode=mode)
            for f in fixture:
                proof=config_numeric.numeric_format(objects,f['aliases'][0]['name'],records[f['address']])
                self.assertEqual(proof['frame'],f['frame'])
                self.assertEqual(proof['requires_options'],(0x60000020,0,0))

    def test_decimal_width_buffer_prefix_and_callback_operand_mutations_reject(self):
        import config_numeric
        objects,records,fixture=self.fixture(fixture_file='config-numeric.json',mode='sizes')
        choices=[('<u8 as core::fmt::Display>::fmt',0x6,'ldrh','r3, [r0]'),
                 ('<u16 as core::fmt::Display>::fmt',0x7a,'sub.w','r1, r7, #0x10'),
                 ('<core::fmt::Formatter>::pad_integral',0x4e,'ldrh.w','r8, [r0, #0xe]'),
                 ('<core::fmt::Formatter>::pad_integral::write_prefix',0x8,'ldr.w','r8, [sp]')]
        for role,offset,mnemonic,operands in choices:
            f=next(f for f in fixture if f['role']==role);rows=list(records[f['address']])
            index=next(i for i,r in enumerate(rows) if r.address==f['address']+offset)
            old=rows[index];rows[index]=Instruction(old.address,old.raw,mnemonic,operands)
            with self.subTest(role=role),self.assertRaises(AssertionError):
                config_numeric.numeric_format(objects,f['aliases'][0]['name'],rows)

    def test_numeric_table_endpoint_and_mapped_tbb_changes_reject(self):
        import config_numeric
        for mutation in ('decimal_endpoint','tbb_entry'):
            objects,records,fixture=self.fixture(fixture_file='config-numeric.json',mode='sizes')
            f=next(f for f in fixture if f['role']==('<u8 as core::fmt::Display>::fmt' if mutation=='decimal_endpoint' else '<core::fmt::Formatter>::pad'))
            data=bytearray(objects.elf.data)
            address=f['address_producers'][0]['address']+199 if mutation=='decimal_endpoint' else f['address']+0xb6
            section=next(s for s in objects.elf.sections if s[3]<=address<s[3]+s[5])
            data[section[4]+address-section[3]]^=1;objects.elf.data=bytes(data)
            with self.subTest(mutation=mutation),self.assertRaises(AssertionError):
                config_numeric.numeric_format(objects,f['aliases'][0]['name'],records[f['address']])

    def test_numeric_code_data_and_targets_relocate_without_erasing_receiver_context(self):
        import config_numeric
        for mode in ('speed2','sizes','sizez'):
            objects,records,fixture=self.fixture(0x1800,0x20000,'config-numeric.json',mode)
            for f in fixture:
                result=config_numeric.numeric_format(objects,f['aliases'][0]['name'],records[f['address']+0x1800])
                self.assertEqual(result['role'],config_stack.canonical_symbol(f['aliases'][0]['name']))
                if f['role']=='<core::fmt::Formatter>::pad_integral':
                    self.assertEqual(len(result['default_sites']),2)


class ControlBodies(unittest.TestCase):
    def fixture(self,mode,code_shift=0,data_shift=0,fixture_file='config-control.json',fixture_data=None):
        spec=[v for v in (fixture_data if fixture_data is not None else
              json.loads((Path(__file__).parent/'fixtures'/fixture_file).read_text())) if v['mode']==mode]
        if code_shift or data_shift:
            locations={a:a+data_shift for item in spec for a,b in item['objects']}
            locations.update({f['address']:f['address']+code_shift for item in spec for f in item['extra_functions']})
            locations.update({item['function']['address']:item['function']['address']+code_shift for item in spec})
            locations.update({a|1:v|1 for a,v in list(locations.items()) if a in {f['address'] for item in spec for f in item['extra_functions']}})
            for item in spec:
                f=item['function'];start=f['address'];body=bytearray.fromhex(item['body_hex']);by_site={r['address']:r for r in f['instructions']}
                for p in f['literals']:
                    value=locations[p['value']]
                    if p['kind']=='pc_literal':struct.pack_into('<I',body,p['pool']-start,value)
                    else:
                        for site,half in ((p['low'],value&65535),(p['high'],value>>16)):
                            row=by_site[site];raw=bytes.fromhex(row['bytes']);first,second=struct.unpack('<HH',raw)
                            raw=struct.pack('<HH',(first&~0x040f)|((half>>12)&15)|((half>>1)&0x400),
                                            (second&~0x70ff)|((half&0x700)<<4)|(half&255))
                            row['bytes']=raw.hex();row['text']=row['text'].split(None,1)[0]+' '+p['register']+', #'+hex(half)
                            body[site-start:site-start+len(raw)]=raw
                for row in f['instructions']:
                    mnemonic=row['text'].split(None,1)[0]
                    if re.fullmatch(r'(?:b(?:l|eq|ne|cs|cc|mi|pl|vs|vc|hi|ls|ge|lt|gt|le)?|cbz|cbnz)(?:\.w)?',mnemonic):
                        row['text']=re.sub(r'0x([0-9a-f]+)',lambda m:hex(int(m[1],16)+code_shift),row['text'],count=1)
                    row['address']+=code_shift
                f['address']+=code_shift;f['intervals']=[(a+code_shift,b+code_shift,k) for a,b,k in f['intervals']]
                for alias in f['aliases']:alias['address']+=code_shift
                item['body_hex']=body.hex();relocated=[]
                for address,encoded in item['objects']:
                    raw=bytes.fromhex(encoded)
                    if len(raw)%4==0:
                        words=list(struct.unpack('<'+'I'*(len(raw)//4),raw))
                        begin=3 if len(words) in (4,6) and words[:3] in ([0,1,1],[0,4,4],[0,12,4]) else 0
                        if all(v in locations for v in words[begin:]):
                            raw=struct.pack('<'+'I'*len(words),*words[:begin],*(locations[v] for v in words[begin:]))
                    for object_address,offset in item.get('pointer_offsets',()):
                        if object_address==address:
                            old=struct.unpack_from('<I',bytes.fromhex(encoded),offset)[0]
                            patch=bytearray(raw);struct.pack_into('<I',patch,offset,locations[old]);raw=bytes(patch)
                    relocated.append((address+data_shift,raw.hex()))
                item['objects']=relocated
                for child in item['extra_functions']:
                    child['address']+=code_shift
                    for alias in child['aliases']:alias['address']+=code_shift
        segments=[];functions={};sizes={};mappings=[];records={}
        for item in spec:
            f=item['function'];start=f['address'];segments.append((start,bytes.fromhex(item['body_hex'])))
            for alias in f['aliases']:functions[alias['name']]=(alias['address'],alias['size'],0)
            sizes[start]=f['frame'];mappings.extend(('$'+kind,a,0) for a,b,kind in f['intervals'])
            for row in f['instructions']:
                parts=row['text'].split(None,1);at=row['address']
                records[at]=Instruction(at,bytes.fromhex(row['bytes']),parts[0],parts[1] if len(parts)>1 else '')
        for item in spec:
            segments.extend((a,bytes.fromhex(b)) for a,b in item['objects'])
            for f in item['extra_functions']:
                for alias in f['aliases']:functions[alias['name']]=(alias['address'],alias['size'],0)
                sizes[f['address']]=f['frame']
                if not any(a<=f['address']<a+len(b) for a,b in segments):segments.append((f['address'],bytes(f['extent'])))
        lo=min(a for a,b in segments);ro=min(a for item in spec for a,b in item['objects']);hi=max(a+len(b) for a,b in segments)
        data=bytearray(hi-lo)
        for a,b in segments:data[a-lo:a-lo+len(b)]=b
        elf=SimpleNamespace(data=bytes(data),functions=functions,mapping_symbols=mappings,
            sections=[(0,1,6,lo,0,ro-lo,0,0,2,0),(0,1,2,ro,ro-lo,hi-ro,0,0,4,0)])
        return config_stack.DiagnosticObjects(elf,sizes),records,spec

    def test_complete_control_body_shapes_and_all_seven_source_cases(self):
        import config_control
        for mode in ('speed2','sizes','sizez'):
            objects,records,spec=self.fixture(mode)
            for item in spec:
                f=item['function'];outline=item['key'].split(':')[0] if item['key'].startswith('@') else None
                proof=config_control.control_body(objects,f['aliases'][0]['name'],records,outline)
                self.assertEqual((proof['extent'],proof['frame']),(f['size'],f['frame']))
                if '12ControlError' in proof['role']:
                    self.assertEqual(proof['source_tags'],(0,1,2,3,5,6,7,8))
                    self.assertEqual(len(proof['cases']),7)

    def test_control_reference_cells_shared_predecessors_result_and_outline_abi_mutations_reject(self):
        import config_control
        for mode,offset,mnemonic,operands in [('speed2',0x5e,'adds','r3, r2, #0x2'),
                ('speed2',0xfe,'str','r4, [sp, #0x4]'),('speed2',0x100,'b','0x10038c6c'),
                ('sizes',0x3c,'str','r0, [sp, #0x14]'),('sizes',0xa2,'mov','r0, r5'),
                ('sizez',0x6c,'str','r0, [sp, #0x10]'),('sizez',0xbc,'ldrb','r0, [r5, #0x2]'),
                ('sizez',0x98,'bpl','0x1001e83a')]:
            objects,records,spec=self.fixture(mode);item=next(v for v in spec if '12ControlError' in v['key'])
            f=item['function'];at=f['address']+offset;old=records[at]
            records[at]=Instruction(at,old.raw,mnemonic,operands)
            with self.subTest(mode=mode,offset=offset),self.assertRaises(AssertionError):
                config_control.control_body(objects,f['aliases'][0]['name'],records)

    def test_control_complete_objects_and_cases_relocate_independently(self):
        import config_control
        for mode in ('speed2','sizes','sizez'):
            objects,records,spec=self.fixture(mode,0x1800,0x21000)
            for item in spec:
                f=item['function'];outline=item['key'].split(':')[0] if item['key'].startswith('@') else None
                proof=config_control.control_body(objects,f['aliases'][0]['name'],records,outline)
                self.assertEqual(proof['function'],f['address'])

    def test_control_readonly_tables_tbb_pool_and_zero_frame_tails_reject(self):
        import config_control
        for mode,fault in [('speed2','vtable'),('sizes','lengths'),('sizez','pointers'),('sizes','tbb'),
                           ('sizez','pool'),('sizez','outline_return'),('sizez','outline_sink')]:
            objects,records,spec=self.fixture(mode);item=next(v for v in spec if '12ControlError' in v['key']);f=item['function']
            data=bytearray(objects.elf.data)
            if fault in ('outline_return','outline_sink'):
                name='@restore_control' if fault=='outline_return' else '@control_sink2'
                outline=next(v for v in spec if v['key'].startswith(name))['function']
                at=outline['instructions'][-1]['address'];old=records[at];records[at]=Instruction(at,old.raw,'bx','lr')
            else:
                if fault=='vtable':address=next(p['value'] for p in f['literals'] if p['first256'].startswith('000000000100000001000000'))+4
                elif fault=='lengths':address=next(p['value'] for p in f['literals'] if p['first256'].startswith('090504'))+2
                elif fault=='pointers':
                    leaf=next(v for v in spec if '11ControlPageNtNt' in v['key'])['function']
                    address=leaf['literals'][1]['value']
                elif fault=='tbb':address=f['data'][0]['start']
                else:address=next(p['pool'] for p in f['literals'] if p['kind']=='pc_literal')
                section=next(s for s in objects.elf.sections if s[3]<=address<s[3]+s[5])
                data[section[4]+address-section[3]]^=1;objects.elf.data=bytes(data)
                if fault=='pointers':f=leaf
            with self.subTest(mode=mode,fault=fault),self.assertRaises(AssertionError):
                config_control.control_body(objects,f['aliases'][0]['name'],records)


class PayloadBodies(unittest.TestCase):
    def fixture(self,mode,code_shift=0,data_shift=0):
        return ControlBodies.fixture(self,mode,code_shift,data_shift,'config-payload.json')

    def test_all_current_payload_clones_require_their_exact_raw_receiver_and_shape(self):
        import config_payload
        for mode in ('speed2','sizes','sizez'):
            objects,records,spec=self.fixture(mode)
            for item in spec:
                f=item['function'];outline=item['key'].rsplit(':',2)[0] if item['key'].startswith('@outline:') else None
                proof=config_payload.payload_body(objects,f['aliases'][0]['name'],records,outline)
                self.assertEqual(proof['shape'],item['shape'])
                self.assertTrue(proof['requires_value_source_and_live_sink'])

    def test_option_niche_reference_cells_struct2_and_outline_abi_mutations_reject(self):
        import config_payload
        for mode,role,offset,mnemonic,operands in [
                ('speed2','<core::option::Option<u16> as core::fmt::Debug>::fmt',0x10,'adds','r0, #0x1'),
                ('sizez','<&core::option::Option<dashboard_config::model::Channel> as core::fmt::Debug>::fmt',4,'cmp','r2, #0x7'),
                ('sizez','<core::option::Option<u8> as core::fmt::Debug>::fmt',0x18,'pop','{r5, r6, r7, pc}'),
                ('sizes','<core::str::error::Utf8Error as core::fmt::Debug>::fmt',0x3c,'strd','r0, r5, [sp]'),
                ('sizes','<core::fmt::Formatter>::debug_struct_field2_finish',0x34,'ldr','r3, [r7, #0xc]'),
                ('sizez','OUTLINED_FUNCTION_13',8,'mov','r1, r5'),
                ('sizez','OUTLINED_FUNCTION_20',0,'str','r0, [sp, #0x4]'),
                ('sizez','OUTLINED_FUNCTION_255',6,'bx','lr')]:
            objects,records,spec=self.fixture(mode);item=next(v for v in spec if v['function']['role']==role);f=item['function']
            at=f['address']+offset;old=records[at];records[at]=Instruction(at,old.raw,mnemonic,operands)
            outline=item['key'].rsplit(':',2)[0] if item['key'].startswith('@outline:') else None
            with self.subTest(mode=mode,role=role),self.assertRaises(AssertionError):
                config_payload.payload_body(objects,f['aliases'][0]['name'],records,outline)

    def test_payload_code_pools_tables_and_callbacks_relocate_independently(self):
        import config_payload
        for mode in ('speed2','sizes','sizez'):
            objects,records,spec=self.fixture(mode,0x1800,0x21000)
            for item in spec:
                f=item['function'];outline=item['key'].rsplit(':',2)[0] if item['key'].startswith('@outline:') else None
                proof=config_payload.payload_body(objects,f['aliases'][0]['name'],records,outline)
                self.assertEqual(proof['shape'],item['shape'])

    def test_payload_exact_enum_tables_same_layout_reference_callbacks_and_context_reject(self):
        import config_payload
        for mode in ('speed2','sizes','sizez'):
            for fault in ('enum_length','enum_pointer','debug_slot','pool_or_raw_context','partial'):
                objects,records,spec=self.fixture(mode)
                family='<dashboard_config::validate::ErrorCode as core::fmt::Debug>::fmt' if fault.startswith('enum_') else '<core::option::Option<u8> as core::fmt::Debug>::fmt'
                item=next(v for v in spec if v['function']['role']==family);f=item['function'];data=bytearray(objects.elf.data)
                if fault=='partial':del records[f['address']]
                elif fault=='pool_or_raw_context':
                    if mode=='sizez':
                        address=f['literals'][0]['pool'];section=objects.elf.sections[0];data[section[4]+address-section[3]]^=1
                    else:
                        name=f['aliases'][0]['name'];entry=objects.elf.functions.pop(name)
                        objects.elf.functions[name+'Cs123_4fake']=entry;f['aliases'][0]['name']=name+'Cs123_4fake'
                else:
                    if fault=='debug_slot':
                        # A real, same-layout reference callback of the wrong
                        # numeric type must not satisfy Option<u8>.
                        table=next(p for p in f['literals'] if p['first256'].startswith('000000000400000004000000'))
                        other=next(v['function'] for v in spec if v['function']['role']=='<core::option::Option<u16> as core::fmt::Debug>::fmt')
                        wrong=next(p for p in other['literals'] if p['first256'].startswith('000000000400000004000000'))
                        address=table['value']+12;section=objects.elf.sections[1]
                        pointer=int.from_bytes(bytes.fromhex(wrong['first256'])[12:16],'little')
                        struct.pack_into('<I',data,section[4]+address-section[3],pointer)
                    else:
                        p=next(p for p in f['literals'] if p['first256'].startswith('07050909')) if fault=='enum_length' else next(p for p in f['literals'] if not p['first256'].startswith('07050909'))
                        address=p['value'];section=objects.elf.sections[1]
                        at=section[4]+address-section[3]
                        if fault=='enum_length':data[at]^=1
                        else:data[at:at+4]=data[at+4:at+8]
                objects.elf.data=bytes(data)
                with self.subTest(mode=mode,fault=fault),self.assertRaises(AssertionError):
                    config_payload.payload_body(objects,f['aliases'][0]['name'],records)


class RootAdapters(unittest.TestCase):
    def fixture(self,mode,code_shift=0,data_shift=0):
        return ControlBodies.fixture(self,mode,code_shift,data_shift,'config-root-adapters.json')

    def test_all_raw_arguments_adapters_preserve_terminal_record_and_location_interfaces(self):
        import config_roots
        for mode in ('speed2','sizes','sizez'):
            objects,records,spec=self.fixture(mode)
            for item in spec:
                f=item['function'];outline=item['key'].rsplit(':',2)[0] if item['key'].startswith('@outline:') else None
                proof=config_roots.root_adapter(objects,f['aliases'][0]['name'],records,outline)
                self.assertEqual((proof['extent'],proof['frame']),(f['size'],f['frame']))
                self.assertTrue(proof['requires_typed_caller_arguments_and_location'])

    def test_root_adapter_raw_records_incoming_location_and_zero_frame_effect_mutations_reject(self):
        import config_roots
        for mode,part,offset,mnemonic,operands in [
                ('speed2','13unwrap_failed',0x10,'ldr.w','lr, [sp, #0x8]'),
                ('sizes','18panic_bounds_check',4,'strd','r1, r0, [sp]'),
                ('sizez','16slice_index_fail',8,'blo','0x10016cd4'),
                ('sizez','OUTLINED_FUNCTION_138',0,'movs','r1, #0x7'),
                ('sizez','OUTLINED_FUNCTION_81',0,'nop','')]:
            objects,records,spec=self.fixture(mode);item=next(v for v in spec if part in v['function']['aliases'][0]['name']);f=item['function']
            at=f['address']+offset;old=records[at];records[at]=Instruction(at,old.raw,mnemonic,operands)
            outline=item['key'].rsplit(':',2)[0] if item['key'].startswith('@outline:') else None
            with self.subTest(mode=mode,part=part),self.assertRaises(AssertionError):
                config_roots.root_adapter(objects,f['aliases'][0]['name'],records,outline)

    def test_root_adapter_code_pool_objects_and_callbacks_relocate_independently(self):
        import config_roots
        for mode in ('speed2','sizes','sizez'):
            objects,records,spec=self.fixture(mode,0x1800,0x21000)
            for item in spec:
                f=item['function'];outline=item['key'].rsplit(':',2)[0] if item['key'].startswith('@outline:') else None
                proof=config_roots.root_adapter(objects,f['aliases'][0]['name'],records,outline)
                self.assertEqual(proof['function'],f['address'])

    def test_root_adapter_actual_template_callback_metadata_and_pool_objects_reject(self):
        import config_roots
        for mode in ('speed2','sizes','sizez'):
            for fault in ('template','callback','frame','pool','tagged_literal'):
                objects,records,spec=self.fixture(mode);part='18panic_bounds_check' if fault=='pool' else 'panic_const_async_fn_resumed' if fault=='tagged_literal' else '13unwrap_failed'
                item=next(v for v in spec if part in v['function']['aliases'][0]['name']);f=item['function'];data=bytearray(objects.elf.data)
                if fault in ('frame','callback'):
                    p=next(p for p in f['literals'] if p['first256']=='');target=p['value']&~1
                    if fault=='frame':objects.sizes[target]+=8
                    else:
                        name=next(n for n,e in objects.elf.functions.items() if e[0]&~1==target);entry=objects.elf.functions.pop(name)
                        objects.elf.functions[name+'Cs123_4fake']=entry
                else:
                    address=next(p['pool'] for p in f['literals'] if p['kind']=='pc_literal') if fault=='pool' else item['objects'][0][0]
                    section=next(s for s in objects.elf.sections if s[3]<=address<s[3]+s[5]);data[section[4]+address-section[3]]^=1
                    objects.elf.data=bytes(data)
                with self.subTest(mode=mode,fault=fault),self.assertRaises(AssertionError):
                    config_roots.root_adapter(objects,f['aliases'][0]['name'],records)


class RootWindows(unittest.TestCase):
    def fixture(self,mode,code_shift=0,data_shift=0):
        spec=json.loads((Path(__file__).parent/'fixtures/config-root-windows.json').read_text())
        # Multiple windows share a caller. Each copied backing body must carry
        # every relocated pool producer, not just its own selected window.
        for item in spec:
            if item.get('support'):continue
            peers=[p for p in spec if p['mode']==item['mode'] and
                   p['function']['address']==item['function']['address']]
            literals={json.dumps(p,sort_keys=True):p for peer in peers for p in peer['function']['literals']}
            item['function']['literals']=list(literals.values())
            rows={p['address']:dict(p) for peer in peers for p in peer['function']['instructions']}
            item['function']['instructions']=[rows[a] for a in sorted(rows)]
        objects,records,spec=ControlBodies.fixture(self,mode,code_shift,data_shift,fixture_data=spec)
        for item in spec:
            if not item.get('support'):item['window']+=code_shift;item['site']+=code_shift
        return objects,records,spec

    def test_all_terminal_windows_bind_current_caller_source_and_concrete_objects(self):
        import config_roots
        for mode in ('speed2','sizes','sizez'):
            objects,records,spec=self.fixture(mode)
            for item in spec:
                if item.get('support'):continue
                proof=config_roots.root_window(objects,item['function']['aliases'][0]['name'],records,item['site'])
                self.assertEqual((proof['window'],proof['location_role']),(item['window'],item['source_role']))
                self.assertTrue(proof['requires_upstream_typed_value_and_all_ingress'])

    def test_terminal_code_pool_data_filename_and_callback_relocations_remain_typed(self):
        import config_roots
        for mode in ('speed2','sizes','sizez'):
            objects,records,spec=self.fixture(mode,0x1800,0x21000)
            for item in spec:
                if item.get('support'):continue
                proof=config_roots.root_window(objects,item['function']['aliases'][0]['name'],records,item['site'])
                self.assertEqual(proof['window'],item['window'])

    def test_terminal_pc_pool_requires_its_declared_caller_data_mapping(self):
        import config_roots
        objects,records,spec=self.fixture('sizez');item=next(v for v in spec if v.get('source_role')=='reviewed_defaults')
        objects.elf.mapping_symbols=[(n.replace('$d','$t'),a,s) for n,a,s in objects.elf.mapping_symbols]
        with self.assertRaises(AssertionError):
            config_roots.root_window(objects,item['function']['aliases'][0]['name'],records,item['site'])

    def test_terminal_caller_extent_alias_and_current_code_bytes_are_required(self):
        import config_roots
        for fault in ('extent','alias','bytes','missing'):
            objects,records,spec=self.fixture('sizes');item=next(v for v in spec if v.get('source_role')=='reviewed_defaults')
            name=item['function']['aliases'][0]['name'];value,extent,index=objects.elf.functions[name]
            if fault=='extent':objects.elf.functions[name]=(value,item['window']-(value&~1)+2,index)
            elif fault=='alias':objects.elf.functions['unreviewed_alias']=(value,extent,index)
            elif fault=='missing':del records[item['window']]
            else:
                at=item['window'];row=records[at];records[at]=Instruction(at,bytes(len(row.raw)),row.mnemonic,row.operands)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_roots.root_window(objects,name,records,item['site'])

    def test_terminal_copy_pointer_length_and_live_object_producers_cannot_be_skipped(self):
        import config_roots
        for mode in ('speed2','sizes','sizez'):
            for fault in ('copy','message','location','vtable','call'):
                objects,records,spec=self.fixture(mode)
                item=next(v for v in spec if v['source_role']=='reviewed_defaults')
                f=item['function'];at=item['window'];row=records[at]
                if fault=='copy':records[at]=Instruction(at,row.raw,'mov','r0, r0')
                elif fault=='call':
                    at=item['site'];row=records[at];records[at]=Instruction(at,row.raw,'bl',hex(at))
                else:
                    ordinal={'message':0,'location':8,'vtable':4}[fault]
                    obj=next((a,b) for a,b in item['objects'] if len(bytes.fromhex(b))==(30 if fault=='message' else 16))
                    if fault=='vtable':obj=next((a,b) for a,b in item['objects'] if bytes.fromhex(b)[:12]==struct.pack('<3I',0,12,2))
                    if fault=='location':obj=next((a,b) for a,b in item['objects'] if bytes.fromhex(b)[8:]==struct.pack('<2I',61,63))
                    data=bytearray(objects.elf.data);section=next(s for s in objects.elf.sections if s[3]<=obj[0]<s[3]+s[5])
                    data[section[4]+obj[0]-section[3]+ordinal]^=1;objects.elf.data=bytes(data)
                with self.subTest(mode=mode,fault=fault),self.assertRaises(AssertionError):
                    config_roots.root_window(objects,f['aliases'][0]['name'],records,item['site'])


class LoggerObjects(unittest.TestCase):
    def fixture(self,mode,code_shift=0,data_shift=0,ram_shift=0,fixture_file='config-logger-objects.json'):
        item=next(v for v in json.loads((Path(__file__).parent/'fixtures'/fixture_file).read_text()) if v['mode']==mode)
        sections=[list(v) for v in item['sections']];data=bytearray()
        for s in sections:
            s[4]=len(data)
            if s[1]==1:data.extend(bytes(s[5]))
        for index,address,encoded in item['segments']:
            s=sections[index];offset=s[4]+address-s[3];raw=bytes.fromhex(encoded);data[offset:offset+len(raw)]=raw
        shifts=[0 if not s[2]&2 else ram_shift if s[2]&1 else code_shift if s[2]&4 else data_shift for s in sections]
        for obj in item['objects']:
            address=obj['value'];kind=obj['type']['kind']
            if kind=='defmt_metadata_id':continue
            index=next(i for i,s in enumerate(sections) if s[2]&2 and s[3]<=address<s[3]+s[5]);s=sections[index];offset=s[4]+address-s[3]
            if kind in ('location','fmt_write_vtable','rtt_header'):
                pointer_offsets=(0,) if kind=='location' else (12,16,20) if kind=='fmt_write_vtable' else (24,28)
                for at in pointer_offsets:
                    old=struct.unpack_from('<I',data,offset+at)[0];target=old&~1 if kind=='fmt_write_vtable' else old
                    owner=next(i for i,s in enumerate(sections) if s[2]&2 and s[3]<=target<s[3]+s[5])
                    struct.pack_into('<I',data,offset+at,old+shifts[owner])
            obj['value']+=shifts[index]
        for row in item['symbol_entries']:row[1]+=shifts[row[4]]
        for row in item['functions'].values():row[0]+=shifts[row[2]]
        item['sizes']={str(int(a)+code_shift):v for a,v in item['sizes'].items()}
        for s,shift in zip(sections,shifts):s[3]+=shift
        sections=[tuple(v) for v in sections]
        elf=SimpleNamespace(data=bytes(data),sections=sections,named={n:sections[i] for n,i in item['named'].items()},
            symbol_entries=[tuple(v) for v in item['symbol_entries']],functions={n:tuple(v) for n,v in item['functions'].items()})
        return config_stack.DiagnosticObjects(elf,{int(a):n for a,n in item['sizes'].items()}),item

    def test_current_logger_objects_preserve_split_field_ordinals_and_writable_name(self):
        import config_logger
        for mode in ('speed2','sizes','sizez'):
            objects,item=self.fixture(mode)
            for obj in item['objects']:config_logger.logger_object(objects,obj['value'],obj['type'])

    def test_logger_code_readonly_ram_and_split_state_relocations_are_independent(self):
        import config_logger
        for mode in ('speed2','sizes','sizez'):
            objects,item=self.fixture(mode,0x1800,0x21000,0x4000)
            for obj in item['objects']:config_logger.logger_object(objects,obj['value'],obj['type'])

    def test_split_state_anchor_cannot_be_mislabelled_as_encoder_or_merge_field_ordinals(self):
        import config_logger
        for fault in ('anchor','field','duplicate','section','size'):
            objects,item=self.fixture('sizez');obj=next(v for v in item['objects'] if v['type']['kind']=='split_state_anchor')
            entries=objects.elf.symbol_entries
            at=next(i for i,v in enumerate(entries) if 'DISPLAY_PENDING' in v[0]) if fault=='anchor' else next(i for i,v in enumerate(entries) if 'RTT_ENCODER.2' in v[0])
            row=list(entries[at])
            if fault=='anchor':row[0]=row[0].replace('DISPLAY_PENDING','RTT_ENCODER')
            elif fault=='field':row[0]=row[0].replace('RTT_ENCODER.2','RTT_ENCODER.3')
            elif fault=='duplicate':entries.append(tuple(row))
            elif fault=='section':row[4]=0
            else:row[2]=2
            entries[at]=tuple(row)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_logger.logger_object(objects,obj['value'],obj['type'])

    def test_rtt_header_pointer_size_signature_and_typed_writable_name_reject(self):
        import config_logger
        for fault in ('signature','buffer_pointer','buffer_size','name','name_section','buffer_extent'):
            objects,item=self.fixture('sizez');obj=next(v for v in item['objects'] if v['type']['kind']=='rtt_header')
            data=bytearray(objects.elf.data);address=obj['value'];section=objects.elf.named['.data'];offset=section[4]+address-section[3]
            if fault=='signature':data[offset]^=1
            elif fault=='buffer_pointer':struct.pack_into('<I',data,offset+28,struct.unpack_from('<I',data,offset+28)[0]+1)
            elif fault=='buffer_size':struct.pack_into('<I',data,offset+32,1023)
            elif fault=='name':data[section[4]+obj['resolved']['name']['address']-section[3]]^=1
            else:
                ordinal=next(i for i,v in enumerate(objects.elf.symbol_entries) if ('4NAME' if fault=='name_section' else '6BUFFER') in v[0]);row=list(objects.elf.symbol_entries[ordinal])
                row[4 if fault=='name_section' else 2]=0 if fault=='name_section' else 1023;objects.elf.symbol_entries[ordinal]=tuple(row)
            objects.elf.data=bytes(data)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_logger.logger_object(objects,address,obj['type'])

    def test_rtt_buffer_extent_cannot_wrap_the_32_bit_address_space(self):
        import config_logger
        objects,item=self.fixture('sizez');obj=next(v for v in item['objects'] if v['type']['kind']=='rtt_header')
        elf=objects.elf;index=next(i for i,v in enumerate(elf.symbol_entries) if '6BUFFER' in v[0]);entry=list(elf.symbol_entries[index])
        entry[1]=0xffffff00;elf.symbol_entries[index]=tuple(entry)
        section=list(elf.sections[entry[4]]);section[3]=entry[1];section[5]=1024
        elf.sections[entry[4]]=tuple(section);elf.named['.uninit']=tuple(section)
        data=bytearray(elf.data);section=elf.named['.data'];struct.pack_into('<I',data,section[4]+obj['value']-section[3]+28,entry[1]);elf.data=bytes(data)
        with self.assertRaises(AssertionError):config_logger.logger_object(objects,obj['value'],obj['type'])


    def test_distinct_logger_state_objects_cannot_alias_one_physical_byte(self):
        import config_logger
        for role in (config_logger.LOCK_OWNER,config_logger.ENCODER+'1'):
            objects,item=self.fixture('speed2')
            moved=next(v for v in item['objects'] if v['type'].get('roles')==[role])
            encoder=next(v for v in item['objects'] if v['type'].get('roles')==[config_logger.ENCODER+'0'])
            index=next(i for i,v in enumerate(objects.elf.symbol_entries) if re.sub(r'Cs[A-Za-z0-9]+_','Cs_',v[0])==role)
            entry=list(objects.elf.symbol_entries[index]);entry[1]=encoder['value'];objects.elf.symbol_entries[index]=tuple(entry)
            with self.subTest(role=role),self.assertRaises(AssertionError):
                config_logger.logger_object(objects,encoder['value'],moved['type'])

    def test_logger_object_rejects_partial_allocated_section_overlap(self):
        import config_logger
        for delta in (-32,128,1000):
            objects,item=self.fixture('sizez');obj=next(v for v in item['objects'] if v['type']['kind']=='rtt_header')
            address=obj['resolved']['buffer']['address'];objects.elf.sections.append((0,8,3,address+delta,0,64,0,0,1,0))
            with self.subTest(delta=delta),self.assertRaises(AssertionError):config_logger.logger_object(objects,obj['value'],obj['type'])

    def test_allocated_section_adjacency_remains_valid(self):
        import config_logger
        for delta in (-64,1024):
            objects,item=self.fixture('sizez');obj=next(v for v in item['objects'] if v['type']['kind']=='rtt_header')
            address=obj['resolved']['buffer']['address'];objects.elf.sections.append((0,8,3,address+delta,0,64,0,0,1,0))
            config_logger.logger_object(objects,obj['value'],obj['type'])

    def test_logger_owning_section_extent_cannot_wrap_even_if_object_fits(self):
        import config_logger
        objects,item=self.fixture('sizez');obj=next(v for v in item['objects'] if v['type']['kind']=='rtt_header')
        index=next(i for i,s in enumerate(objects.elf.sections) if s==objects.elf.named['.uninit'])
        section=list(objects.elf.sections[index]);section[5]=0xffffffff;objects.elf.sections[index]=tuple(section);objects.elf.named['.uninit']=tuple(section)
        with self.assertRaises(AssertionError):config_logger.logger_object(objects,obj['value'],obj['type'])


class LoggerBodies(unittest.TestCase):
    OUTLINES={'OUTLINED_FUNCTION_272':'@outline:logger_channel_restore',
              'OUTLINED_FUNCTION_98':'@outline:logger_byte_to_channel',
              'OUTLINED_FUNCTION_229':'@outline:logger_write_restore'}

    def fixture(self,mode):
        objects,item=LoggerObjects.fixture(self,mode,fixture_file='config-logger-bodies.json')
        objects.elf.mapping_symbols=[tuple(v) for v in item['mapping_symbols']]
        records={}
        for row in item['records']:
            op=row['text'].split(None,1);at=row['address'];records[at]=Instruction(at,bytes.fromhex(row['bytes']),op[0],op[1] if len(op)>1 else '')
        return objects,records,item

    def test_complete_logger_and_builtin_bodies_bind_every_current_instruction(self):
        import config_logger
        for mode in ('speed2','sizes','sizez'):
            objects,records,item=self.fixture(mode)
            for instance in item['instances']:
                name=next(a['name'] for a in instance['aliases'] if a['size'])
                proof=config_logger.logger_body(objects,name,records,self.OUTLINES.get(name))
                self.assertEqual((proof['extent'],proof['frame']),(instance['extent'],instance['own_frame']))
                self.assertTrue(proof['requires_runtime_ownership_entry_and_guard_closure'])

    def test_timestamp_alias_contract_cannot_hide_wrong_size_binding_duplicate_or_section(self):
        import config_logger
        for fault in ('size','binding','duplicate','elsewhere','section','alias'):
            objects,records,item=self.fixture('sizez');instance=next(v for v in item['instances'] if v['role']=='_defmt_timestamp')
            ordinal=next(i for i,v in enumerate(objects.elf.symbol_entries) if v[0]=='_defmt_timestamp');entry=list(objects.elf.symbol_entries[ordinal])
            if fault=='size':entry[2]=6
            elif fault=='binding':entry[3]=2
            elif fault=='duplicate':objects.elf.symbol_entries.append(tuple(entry))
            elif fault=='elsewhere':
                other=list(entry);other[1]+=0x100;objects.elf.symbol_entries.append(tuple(other))
            elif fault=='section':entry[4]=0
            else:entry[0]='unreviewed_timestamp_alias'
            objects.elf.symbol_entries[ordinal]=tuple(entry)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_logger.logger_body(objects,'__defmt_default_timestamp',records)

    def test_builtin_frame_fallback_requires_exact_body_extent_and_matching_metadata(self):
        import config_logger
        for fault in ('extent','frame','instruction','metadata_duplicate'):
            objects,records,item=self.fixture('sizez');instance=next(v for v in item['instances'] if v['role']=='compiler_builtins::mem::memcpy')
            name=instance['aliases'][0]['name'];start=instance['entry']
            if fault=='extent':
                value,extent,index=objects.elf.functions[name];objects.elf.functions[name]=(value,extent-2,index)
            elif fault=='frame':objects.sizes[start]=instance['own_frame']-8
            elif fault=='metadata_duplicate':
                entry=list(next(v for v in objects.elf.symbol_entries if v[0]==name));entry[1]+=0x100;entry[2]=0;objects.elf.symbol_entries.append(tuple(entry))
            else:
                row=records[start];records[start]=Instruction(start,row.raw,'push','{r7, lr}')
            with self.subTest(fault=fault),self.assertRaises(AssertionError):config_logger.logger_body(objects,name,records)

    def test_fixed_pc_scalar_mask_and_parent_owned_outline_effects_cannot_change(self):
        import config_logger
        for fault in ('scalar_mask','outline_byte_cell','outline_restore'):
            objects,records,item=self.fixture('sizez')
            role='<defmt::export::FmtWrite as core::fmt::Write>::write_char' if fault=='scalar_mask' else '_defmt_write'
            instance=next(v for v in item['instances'] if v['role']==role);name=next(a['name'] for a in instance['aliases'] if a['size'])
            if fault=='scalar_mask':
                target=instance['entry']+124;section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=target<s[3]+s[5])
                data=bytearray(objects.elf.data);data[section[4]+target-section[3]]^=1;objects.elf.data=bytes(data)
            else:
                outline=next(v for v in item['instances'] if v['role']==('OUTLINED_FUNCTION_98' if fault=='outline_byte_cell' else 'OUTLINED_FUNCTION_229'))
                at=outline['entry'];row=records[at]
                records[at]=Instruction(at,row.raw,'strb' if fault=='outline_byte_cell' else 'pop.w',
                    'r0, [r7, #-2]' if fault=='outline_byte_cell' else '{r3, r8, r9, r10}')
                if fault=='outline_byte_cell':name='OUTLINED_FUNCTION_98'
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_logger.logger_body(objects,name,records,self.OUTLINES.get(name))


class EntryBodies(unittest.TestCase):
    def fixture(self,mode):
        objects,item=LoggerObjects.fixture(self,mode,fixture_file='config-entry-bodies.json')
        objects.elf.mapping_symbols=[tuple(v) for v in item['mapping_symbols']]
        records={}
        for row in item['records']:
            op=row['text'].split(None,1);at=row['address']
            records[at]=Instruction(at,bytes.fromhex(row['bytes']),op[0],op[1] if len(op)>1 else '')
        return objects,records,item

    def coherent_initializer_mix(self,phase):
        import config_entry
        objects,records,item=self.fixture('speed2');other,other_records,other_item=self.fixture('sizez')
        old=next(v for v in item['instances'] if config_entry.PHASES[v['shape']]==phase)
        new=next(v for v in other_item['instances'] if config_entry.PHASES[v['shape']]==phase)
        self.assertEqual(old['role'],new['role']);self.assertEqual((old['frame'],new['frame']),(8,8))
        donor=config_entry.entry_body(other,new['name'],other_records)
        contract=config_entry.ENTRY_GRAMMARS[donor['shape']]
        blob=bytearray(other.read_only(new['entry'],new['extent']))
        # Only the Dashboard donor has a PC pool. Rebind its exact Config
        # object to this ELF; preserve every instruction and its relative pool.
        for offset,width,value in contract['data']:
            if not value.startswith('@object'):continue
            ordinal=int(value[7:]);descriptor=contract['objects'][ordinal]
            self.assertEqual(descriptor['kind'],'entry_static');self.assertEqual(width,4)
            self.assertEqual(struct.unpack_from('<I',blob,offset)[0],donor['objects'][ordinal])
            target=next(v[1] for v in objects.elf.symbol_entries if config_entry.static_role(v[0])==descriptor['role'])
            struct.pack_into('<I',blob,offset,target)
        start=old['entry'];pointer,_,index=objects.elf.functions[old['name']];section=objects.elf.sections[index]
        data=bytearray(objects.elf.data);offset=section[4]+start-section[3];data[offset:offset+len(blob)]=blob;objects.elf.data=bytes(data)
        objects.elf.functions[old['name']]=(pointer,new['extent'],index)
        objects.elf.symbol_entries=[(n,v,new['extent'],info,ix) if n==old['name'] else (n,v,z,info,ix)
                                    for n,v,z,info,ix in objects.elf.symbol_entries]
        objects.elf.mapping_symbols=[v for v in objects.elf.mapping_symbols if not (v[2]==index and start<=v[1]<start+old['extent'])]
        objects.elf.mapping_symbols.extend(('$'+kind,start+a,index) for a,b,kind in contract['mapping'])
        for at in list(records):
            if start<=at<start+old['extent']:del records[at]
        for at,row in other_records.items():
            if new['entry']<=at<new['entry']+new['extent']:
                target=start+at-new['entry'];records[target]=Instruction(target,row.raw,row.mnemonic,row.operands)
        # Prove the mutation reaches composition: its complete foreign body,
        # current bytes, mappings, typed objects and metadata are still valid.
        self.assertEqual(config_entry.entry_body(objects,old['name'],records)['shape'],new['shape'])
        return objects,records

    def test_coherent_approved_boot_and_dashboard_initializer_abi_mixes_reject(self):
        import config_entry
        for phase in ('boot_initializer','dashboard_initializer'):
            objects,records=self.coherent_initializer_mix(phase)
            with self.subTest(phase=phase),self.assertRaises(AssertionError):config_entry.boot_entry(objects,records)

    def test_complete_boot_bodies_bind_all_queue_callback_and_retirement_instructions(self):
        import config_entry
        count=0
        for mode in ('speed2','sizes','sizez'):
            objects,records,item=self.fixture(mode)
            for instance in item['instances']:
                proof=config_entry.entry_body(objects,instance['name'],records,
                    instance['role'] if instance['role'].startswith('@outline:') else None)
                self.assertEqual((proof['extent'],proof['frame']),(instance['extent'],instance['frame']))
                self.assertTrue(proof['requires_first_batch_source_and_task_ownership'])
                count+=1
        self.assertEqual(count,25)

    def test_entry_callback_pool_field_and_queue_instructions_cannot_change(self):
        import config_entry
        for fault in ('callback_field','retained_next','completion','claim'):
            objects,records,item=self.fixture('sizez')
            instance=next(v for v in item['instances'] if v['role']==('@outline:entry_install_boot_callback' if fault=='callback_field'
                else '_RNvMs8_NtCs_16embassy_executor3rawNtB5_8Executor4poll' if fault=='retained_next'
                else next(v['role'] for v in item['instances'] if v['frame']==32) if fault=='completion'
                else next(v['role'] for v in item['instances'] if 'Executor3run' in v['role'])))
            rows=[r for a,r in sorted(records.items()) if instance['entry']<=a<instance['entry']+instance['extent']]
            row=next(r for r in rows if r.mnemonic.startswith('str') and '#0x24' in r.operands) if fault=='callback_field' else (
                next(r for r in rows if r.mnemonic=='ldr' and '#0x1c' in r.operands) if fault=='retained_next' else
                next(r for r in rows if r.mnemonic.startswith('strb') and '#0x2c' in r.operands) if fault=='completion' else
                next(r for r in rows if r.mnemonic.startswith('strex')))
            records[row.address]=Instruction(row.address,row.raw,row.mnemonic,row.operands.replace('#0x24','#0x20') if fault=='callback_field' else
                row.operands.replace('#0x1c','#0x20') if fault=='retained_next' else row.operands.replace('#0x2c','#0x2d') if fault=='completion' else 'r2, r2, [r1]')
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_entry.entry_body(objects,instance['name'],records,instance['role'] if instance['role'].startswith('@outline:') else None)

    def test_task_storage_is_complete_zero_initialized_disjoint_and_typed(self):
        import config_entry
        for fault in ('size','binding','section','duplicate','alias','config_taken'):
            objects,records,item=self.fixture('sizez');role=config_entry.CONFIG if fault=='config_taken' else config_entry.BOOT_POOL
            ordinal=next(i for i,v in enumerate(objects.elf.symbol_entries) if config_entry.static_role(v[0])==role)
            entry=list(objects.elf.symbol_entries[ordinal])
            if fault=='size':entry[2]-=1
            elif fault=='binding':entry[3]=17
            elif fault=='section':entry[4]=0
            elif fault=='duplicate':objects.elf.symbol_entries.append(tuple(entry))
            elif fault=='alias':objects.elf.symbol_entries.append(('other_allocated_object',entry[1]+24,4,1,entry[4]))
            else:
                section=objects.elf.sections[entry[4]];data=bytearray(objects.elf.data)
                data[section[4]+entry[1]-section[3]+14600]=1;objects.elf.data=bytes(data)
            objects.elf.symbol_entries[ordinal]=tuple(entry)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_entry.entry_object(objects,entry[1],dict(kind='entry_static',role=role))

    def test_producer_selected_callback_cannot_be_another_approved_task_or_an_interior(self):
        import config_entry
        for fault in ('different_task','interior','non_thumb','wrong_frame'):
            objects,records,item=self.fixture('sizez')
            descriptors=[v for g in config_entry.ENTRY_GRAMMARS.values() for v in g['objects'] if v['kind']=='entry_callback']
            descriptor=next(v for v in descriptors if '____embassy_main_task' in v['role'] and v['extent']==176)
            name=next(n for n in objects.elf.functions if config_stack.canonical_symbol(n)==descriptor['role'])
            address=objects.elf.functions[name][0]
            if fault=='different_task':
                address=next(e[0] for n,e in objects.elf.functions.items() if 'dashboard_task' in n and '4poll' in n)
            elif fault=='interior':address+=2
            elif fault=='non_thumb':address&=~1
            else:objects.sizes[address&~1]+=8
            with self.subTest(fault=fault),self.assertRaises(AssertionError):config_entry.entry_object(objects,address,descriptor)

    def test_contextual_entry_binds_one_first_batch_callback_and_retains_actual_ancestors(self):
        import config_entry
        for mode in ('speed2','sizes','sizez'):
            objects,records,item=self.fixture(mode);proof=config_entry.boot_entry(objects,records)
            self.assertEqual(proof['entry_frame'],56 if mode=='sizez' else 40)
            self.assertEqual(proof['boot_poll_frame'],32)
            self.assertEqual(proof['initialize_frame'],8)
            self.assertEqual(proof['callback_edge']['scope'],'first detached boot batch')
            self.assertNotEqual(proof['callback_edge']['target'],proof['later_dashboard_callback'])
            self.assertTrue(proof['requires_configuration_and_diagnostic_memory_closure'])

    def test_contextual_entry_cannot_ignore_duplicate_phase_or_changed_dequeue_callback(self):
        import config_entry
        for fault in ('duplicate_phase','changed_callback_load','changed_queue_head'):
            objects,records,item=self.fixture('sizez')
            if fault=='duplicate_phase':
                raw=next(v for v in objects.elf.symbol_entries if config_stack.canonical_symbol(v[0])=='_RNvCs_5pico218___cortex_m_rt_main')
                other=list(raw);other[1]+=0x200;other[2]=0;objects.elf.symbol_entries.append(tuple(other))
            else:
                instance=next(v for v in item['instances'] if v['role']=='_RNvMs8_NtCs_16embassy_executor3rawNtB5_8Executor4poll')
                row=next(r for a,r in sorted(records.items()) if instance['entry']<=a<instance['entry']+instance['extent'] and
                    (r.mnemonic=='ldr' and '#0x24' in r.operands if fault=='changed_callback_load' else r.mnemonic.startswith(('strex','stlex'))))
                records[row.address]=Instruction(row.address,row.raw,row.mnemonic,
                    row.operands.replace('#0x24','#0x20') if fault=='changed_callback_load' else 'r3, r5, [r4]')
            with self.subTest(fault=fault),self.assertRaises(AssertionError):config_entry.boot_entry(objects,records)

    def test_initialize_once_boundary_must_be_complete_before_its_loader_edge_is_used(self):
        import config_entry
        for mode in ('speed2','sizes','sizez'):
            objects,records,item=self.fixture(mode)
            name=next(n for n in objects.elf.functions if config_stack.canonical_symbol(n)==config_entry.INITIALIZE)
            proof=config_entry.entry_body(objects,name,records)
            self.assertEqual(proof['frame'],8)
            self.assertTrue(any('load_at_boot' in edge[3] for edge in proof['ordinary_edges']))
            context=config_entry.boot_entry(objects,records)
            self.assertEqual(context['phases']['configuration_initialize']['function'],proof['function'])

    def test_initialize_once_atomic_tag_and_clrex_child_cannot_be_skipped_or_changed(self):
        import config_entry
        for fault in ('atomic_acquire','atomic_store','tagged_length','clrex_child'):
            objects,records,item=self.fixture('sizez')
            name=next(n for n in objects.elf.functions if config_stack.canonical_symbol(n)==config_entry.INITIALIZE)
            start,extent,_=objects.elf.functions[name];start&=~1
            if fault=='clrex_child':
                child=next(n for n in objects.elf.functions if n=='OUTLINED_FUNCTION_81');at=objects.elf.functions[child][0]&~1
                row=records[at];records[at]=Instruction(at,row.raw,'dmb','sy')
            else:
                row=next(r for a,r in sorted(records.items()) if start<=a<start+extent and
                    (r.mnemonic=='ldaexb' if fault=='atomic_acquire' else r.mnemonic=='strexb' if fault=='atomic_store'
                     else r.mnemonic=='movs' and r.operands=='r1, #0x77'))
                records[row.address]=Instruction(row.address,row.raw,'ldrexb' if fault=='atomic_acquire' else row.mnemonic,
                    'r3, r1, [r0]' if fault=='atomic_store' else 'r1, #0x76' if fault=='tagged_length' else row.operands)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):config_entry.boot_entry(objects,records)


class IssuerRegions(unittest.TestCase):
    def fixture(self,mode):
        objects,item=LoggerObjects.fixture(self,mode,fixture_file='config-issuer-regions.json')
        objects.elf.mapping_symbols=[tuple(v) for v in item['mapping_symbols']]
        records={}
        for row in item['records']:
            op=row['text'].split(None,1);at=row['address']
            records[at]=Instruction(at,bytes.fromhex(row['bytes']),op[0],op[1] if len(op)>1 else '')
        return objects,records,item

    def test_issuer_regions_bind_every_retry_lane_and_independently_typed_failure_suffix(self):
        import config_upstream
        for mode in ('speed2','sizes','sizez'):
            objects,records,item=self.fixture(mode);proof=config_upstream.issuer_region(objects,item['issuer']['name'],records)
            self.assertEqual(proof['window'],item['issuer']['start'])
            self.assertEqual(proof['attempts'],4 if mode=='speed2' else 1)
            self.assertEqual(proof['previous_register'],'r1' if mode=='speed2' else 'r0')
            self.assertEqual(proof['failure_suffix']['location_role'],'identity_first')
            self.assertEqual(proof['local_write_ranges'],())
            self.assertEqual(proof['writes'][0]['extent'],4)
            self.assertEqual([v['tag_value'] for v in proof['state_alternatives']],[0,2])
            self.assertTrue(proof['requires_parent_phase_all_ingress_and_coverage'])

    def test_issuer_previous_value_status_retry_and_static_identity_cannot_change(self):
        import config_upstream
        for fault in ('status','previous_lane','retry','static_initial','failure_target'):
            objects,records,item=self.fixture('speed2');start=item['issuer']['start'];end=item['issuer']['end']
            rows=[r for a,r in sorted(records.items()) if start<=a<end]
            if fault=='static_initial':
                entry=next(v for v in objects.elf.symbol_entries if __import__('config_entry').static_role(v[0])==__import__('config_entry').ISSUER)
                section=objects.elf.sections[entry[4]];data=bytearray(objects.elf.data);data[section[4]+entry[1]-section[3]]=0;objects.elf.data=bytes(data)
            else:
                row=next(r for r in rows if r.mnemonic.startswith('strex')) if fault=='status' else (
                    next(r for r in rows if r.mnemonic=='mov' and r.operands=='r1, r2') if fault=='previous_lane' else
                    next(r for r in rows if r.mnemonic=='beq') if fault=='retry' else
                    next(r for r in rows if r.mnemonic=='beq.w'))
                records[row.address]=Instruction(row.address,row.raw,row.mnemonic,
                    'r2, r2, [r0]' if fault=='status' else 'r1, r3' if fault=='previous_lane' else
                    '0x'+format(end,'x'))
            with self.subTest(fault=fault),self.assertRaises(AssertionError):config_upstream.issuer_region(objects,item['issuer']['name'],records)

    def test_semantic_record_foreign_direct_ingress_cannot_skip_static_or_cas_producers(self):
        import config_upstream
        objects,records,item=self.fixture('speed2');start=item['issuer']['start']
        row=records[max(a for a in records if a<start)];records[row.address]=Instruction(row.address,row.raw,'b','0x'+format(start+14,'x'))
        with self.assertRaises(AssertionError):config_upstream.issuer_region(objects,item['issuer']['name'],records)


class OrdinaryHelperBodies(unittest.TestCase):
    def test_child_threshold_save_and_nonreturn_adapter_changes_reject(self):
        import config_helpers
        cases=(('range_to_inclusive_index_mut',8,'itttt','hi'),('array5_index_mut',12,'movs','r2, #0x4'),
               ('deserialize_u16',58,'cmp','r0, #0x4'),('restore_r11_low20',0,'ldr','r8, [sp], #4'),
               ('deserialize_seed_f32',20,'ldrd','r1, r3, [r2]'))
        for role,offset,operation,operands in cases:
            objects,records,item=self.fixture();instance=next(v for v in item['instances'] if v['role']==role)
            at=instance['address']+offset;row=records[at];records[at]=Instruction(at,row.raw,operation,operands)
            outline=instance['typed_role'] if instance['typed_role'].startswith('@outline:') else None
            with self.subTest(role=role),self.assertRaises(AssertionError):
                config_helpers.helper_body(objects,instance['name'],records,outline)
        objects,records,item=self.fixture();instance=next(v for v in item['instances'] if v['role']=='range_to_inclusive_index_mut')
        boundary=next(n for n in objects.elf.functions if config_stack.canonical_symbol(n)=='_RNvNtNtCs_4core5slice5index16slice_index_fail')
        at=objects.elf.functions[boundary][0]&~1;row=records[at];records[at]=Instruction(at,row.raw,'mov','r7, sp')
        # The parent and target metadata still match. Nonreturn inventory must
        # also consume the complete current raw-Arguments adapter body.
        config_helpers.helper_body(objects,instance['name'],records)
        with self.assertRaises(AssertionError):
            config_helpers.helper_coverage(objects,instance['name'],records,(instance['address'],'range','source-bound error edge'))

    def test_new_child_inventories_relocate_with_their_nonreturn_adapter(self):
        import config_helpers
        objects,records,item=self.fixture(0x2800,0x19000)
        for instance in item['instances']:
            if instance['role'] not in ('range_to_inclusive_index_mut','array5_index_mut','deserialize_u16','deserialize_seed_f32'):continue
            proof=config_helpers.helper_coverage(objects,instance['name'],records,(instance['address'],'child','relocated input'))
            self.assertEqual(proof['function'],instance['address'])

    def test_four_new_child_bodies_keep_f32_policy_and_pending_restore_requirements(self):
        import config_helpers
        objects,records,item=self.fixture()
        for role in ('deserialize_seed_f32','range_to_inclusive_index_mut','array5_index_mut','deserialize_u16','restore_r11_low20'):
            instance=next(v for v in item['instances'] if v['role']==role);outline=instance['typed_role'] if instance['typed_role'].startswith('@outline:') else None
            with self.subTest(role=role):
                proof=config_helpers.helper_body(objects,instance['name'],records,outline)
                self.assertEqual(proof['requires_unaligned_single_ldr_policy'],role=='deserialize_seed_f32')
                self.assertTrue(proof['requires_source_inputs_owned_effects_and_child_closure'])
                if role=='restore_r11_low20':
                    self.assertEqual(proof['requires_parent_frame_and_abi'],'restore_r11_low20')
                    self.assertEqual(proof['requires_parent_outline_abi'],'@outline:ordinary_restore_r11_low20')

    def test_ordinary_identity_cannot_be_supplied_as_outline_override(self):
        import config_helpers
        for consumer in (config_helpers.helper_body,config_helpers.helper_coverage):
            objects,records,item=self.fixture();instance=next(v for v in item['instances'] if v['role']=='serialize_u8')
            ordinary=instance['name'];anonymous='OUTLINED_FUNCTION_999999'
            objects.elf.functions[anonymous]=objects.elf.functions.pop(ordinary)
            objects.elf.symbol_entries=[(anonymous if v[0]==ordinary else v[0],*v[1:]) for v in objects.elf.symbol_entries]
            with self.assertRaises(AssertionError):config_helpers.helper_body(objects,anonymous,records)
            with self.subTest(consumer=consumer.__name__),self.assertRaises(AssertionError):
                if consumer is config_helpers.helper_body:
                    consumer(objects,anonymous,records,instance['typed_role'])
                else:
                    consumer(objects,anonymous,records,(instance['address'],'helper','anonymous context'),instance['typed_role'])

    def test_invalid_outline_override_values_do_not_mean_no_override(self):
        import config_helpers
        for invalid in ('',0,False,[],{},1):
            objects,records,item=self.fixture();instance=next(v for v in item['instances'] if v['role']=='serialize_u8')
            with self.subTest(value=invalid),self.assertRaises(AssertionError):
                config_helpers.helper_body(objects,instance['name'],records,invalid)

    def test_bound_helper_frames_keep_exact_scratch_and_save_cells(self):
        import config_helpers
        from config_regions import Storage,require_writes
        objects,records,item=self.fixture()
        expected={'serialize_u16':(4,3),'serialize_u8':(7,1),'varint_usize':(0,5),
                  'varint_u32_small':(3,5),'serialize_f32':(4,4),'seq_f32':(0,8)}
        for instance in item['instances']:
            if not instance['frame']:continue
            proof=config_helpers.helper_body(objects,instance['name'],records)
            frame=Storage('stack:boot_call','active:'+instance['role'],-instance['frame'],instance['frame'],8,'parse',True)
            facts=config_helpers.helper_frame(proof,frame,())
            self.assertEqual([(v.offset,v.extent) for v in facts['scratch']],
                             [expected[instance['role']]] if instance['role'] in expected else [])
            self.assertTrue(facts['requires_caller_sp_inputs_child_effects_and_initialization'])
            for view in facts['scratch']:
                require_writes(facts['scratch'],((view,0,view.extent),),facts['protected'])
                with self.assertRaises(AssertionError):
                    require_writes(facts['scratch'],((view,0,view.extent+1),),facts['protected'])

    def test_helper_frame_cannot_overlap_ancestor_or_invent_outline_storage(self):
        import config_helpers
        from config_regions import Storage
        objects,records,item=self.fixture();instance=next(v for v in item['instances'] if v['role']=='varint_u32_small')
        proof=config_helpers.helper_body(objects,instance['name'],records)
        frame=Storage('stack:boot_call','active',-24,24,8,'parse',True)
        alias=Storage('stack:boot_call','ancestor_saved_r8',-20,4,4,'parse',False)
        with self.assertRaises(AssertionError):config_helpers.helper_frame(proof,frame,iter((alias,)))
        for extent,alignment,writable in ((32,8,True),(24,4,True),(24,8,False)):
            invalid=Storage('stack:boot_call','active',-32,extent,alignment,'parse',writable)
            with self.subTest(extent=extent,alignment=alignment,writable=writable),self.assertRaises(AssertionError):
                config_helpers.helper_frame(proof,invalid,())
        outline=next(v for v in item['instances'] if v['role']=='restore_high12_low20')
        bound=config_helpers.helper_body(objects,outline['name'],records,outline['typed_role'])
        with self.assertRaises(AssertionError):config_helpers.helper_frame(bound,frame,())

    def fixture(self,code_shift=0,data_shift=0):
        objects,item=LoggerObjects.fixture(self,'sizez',code_shift,data_shift,fixture_file='config-small-helpers.json')
        objects.elf.mapping_symbols=[(n,a+code_shift,i) for n,a,i in item['mapping_symbols']]
        for instance in item['instances']:instance['address']+=code_shift
        data=bytearray(objects.elf.data)
        for role,offset in (('serialize_u16',84),('varint_usize',68),('varint_u32_small',40)):
            instance=next(v for v in item['instances'] if v['role']==role);at=instance['address']+offset
            section=objects.elf.sections[objects.elf.functions[instance['name']][2]];index=section[4]+at-section[3]
            struct.pack_into('<I',data,index,struct.unpack_from('<I',data,index)[0]+data_shift)
        objects.elf.data=bytes(data)
        records={}
        for row in item['records']:
            operation=row['text'].split(None,1);at=row['address']+code_shift;operands=operation[1] if len(operation)>1 else ''
            if config_stack.transfer_family(operation[0].removesuffix('.w')) in ('b','bl','cbz','cbnz'):
                operands=re.sub(r'0x([0-9a-f]+)',lambda m:hex(int(m[1],16)+code_shift),operands)
            records[at]=Instruction(at,bytes.fromhex(row['bytes']),operation[0],operands)
        data=bytearray(objects.elf.data)
        for pair in item.get('mov_objects',()):
            shift=code_shift if objects.elf.sections[pair['target_section']][2]&4 else data_shift
            value=pair['value']+shift
            for site,half in ((pair['low'],value&65535),(pair['high'],value>>16)):
                at=site+code_shift;row=records[at];first,second=struct.unpack('<HH',row.raw)
                raw=struct.pack('<HH',(first&~0x040f)|((half>>12)&15)|((half>>1)&0x400),
                                (second&~0x70ff)|((half&0x700)<<4)|(half&255))
                records[at]=Instruction(at,raw,row.mnemonic,row.operands.split(',')[0]+', #'+hex(half))
                section=next(s for s in objects.elf.sections if s[2]&4 and s[3]<=at<at+4<=s[3]+s[5])
                index=section[4]+at-section[3];data[index:index+4]=raw
        objects.elf.data=bytes(data)
        return objects,records,item

    def test_all_complete_helpers_and_parent_owned_outlines_bind_exact_bodies(self):
        import config_helpers
        objects,records,item=self.fixture()
        self.assertEqual(len(item['instances']),26)
        for instance in item['instances']:
            role=instance['typed_role'];outline=role if role.startswith('@outline:') else None
            proof=config_helpers.helper_body(objects,instance['name'],records,outline)
            self.assertEqual((proof['extent'],proof['frame']),(instance['extent'],instance['frame']))
            self.assertTrue(proof['requires_source_inputs_owned_effects_and_child_closure'])

    def test_code_child_roles_and_source_location_objects_relocate_independently(self):
        import config_helpers
        objects,records,item=self.fixture(0x2800,0x19000)
        for instance in item['instances']:
            role=instance['typed_role'];outline=role if role.startswith('@outline:') else None
            proof=config_helpers.helper_body(objects,instance['name'],records,outline)
            self.assertEqual(proof['function'],instance['address'])

    def test_selector_loop_store_flags_and_save_restoration_mutations_reject(self):
        import config_helpers
        cases=(('slice_pop',0,'ldr','r2, [r0]'),('varint_usize',18,'cmp','r1, #0x4'),
               ('take_varint_u32',56,'cmp','r1, #0x10'),('map_compare_status_flags',4,'movs','r0, #0x2'),
               ('restore_high12_low20',0,'pop.w','{r8, r9, r10}'),('seq_decrement_tail_pop',2,'str','r1, [r0]'))
        for role,offset,operation,operands in cases:
            objects,records,item=self.fixture();instance=next(v for v in item['instances'] if v['role']==role);at=instance['address']+offset
            row=records[at];records[at]=Instruction(at,row.raw,operation,operands)
            outline=instance['typed_role'] if instance['typed_role'].startswith('@outline:') else None
            with self.subTest(role=role),self.assertRaises(AssertionError):
                config_helpers.helper_body(objects,instance['name'],records,outline)

    def test_location_roles_and_current_child_frames_cannot_be_swapped(self):
        import config_helpers
        for fault in ('location','frame'):
            objects,records,item=self.fixture();instance=next(v for v in item['instances'] if v['role']=='serialize_u16')
            if fault=='frame':objects.sizes[next(v['address'] for v in item['instances'] if v['role']=='compare_extend')]+=8
            else:
                at=instance['address']+84;section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=at<at+4<=s[3]+s[5])
                data=bytearray(objects.elf.data);offset=section[4]+at-section[3]
                other=next(v for v in item['instances'] if v['role']=='varint_usize')['address']+68
                child_section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=other<other+4<=s[3]+s[5])
                data[offset:offset+4]=data[child_section[4]+other-child_section[3]:child_section[4]+other-child_section[3]+4]
                objects.elf.data=bytes(data)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_helpers.helper_body(objects,instance['name'],records)


    def test_complete_helper_coverage_includes_post_return_padding_and_typed_pool(self):
        import config_helpers
        objects,records,item=self.fixture()
        for instance in item['instances']:
            role=instance['typed_role'];outline=role if role.startswith('@outline:') else None
            proof=config_helpers.helper_coverage(objects,instance['name'],records,
                (instance['address'],'helper inventory','source inputs still required'),outline)
            coverage=proof['coverage']
            self.assertEqual(coverage['code_bytes']+coverage['data_bytes'],instance['extent'])
            self.assertTrue(coverage['requires_authored_semantics_source_and_child_effects'])


class OrdinaryIndexEffects(unittest.TestCase):
    def fixture(self,wrapper=False):
        import config_helpers
        from config_regions import Storage,borrow,Argument
        objects,records,item=OrdinaryHelperBodies().fixture()
        role='array5_index_mut' if wrapper else 'range_to_inclusive_index_mut'
        instance=next(v for v in item['instances'] if v['role']==role)
        proof=config_helpers.helper_coverage(objects,instance['name'],records,(instance['address'],'index','caller transport still required'))
        parent=Storage('stack:call','serializer',0,24,8,'active-varint',True)
        buffer=borrow(parent,0 if wrapper else 4,5 if wrapper else 3,'active-varint',writable=True)
        frame=Storage('stack:call','index',-8 if wrapper else -16,8 if wrapper else 16,8,'active-index',True)
        saved=Storage('stack:call','serializer_saved',8,16,4,'active-varint',False)
        producer=next(v for v in item['instances'] if v['role']==('varint_usize' if wrapper else 'serialize_u16'))
        location=next(iter(config_helpers.helper_body(objects,producer['name'],records)['objects'].values()))
        argument=Argument('usize',None,0,32,frozenset((0,4,5,0xffffffff) if wrapper else (0,2,3,0xffffffff)))
        return objects,records,proof,buffer,frame,(saved,),location,argument

    def test_inclusive_prefix_normal_and_failure_preserve_the_original_endpoint(self):
        import config_helpers
        values=self.fixture();proof=config_helpers.index_prefix(*values,location_role='postcard_serialize_u16')
        outcomes={v['endpoint']:v for v in proof['outcomes']}
        self.assertEqual([(i,outcomes[i]['prefix'].extent) for i in (0,2)],[(0,1),(2,3)])
        for i in (3,0xffffffff):
            self.assertEqual(outcomes[i]['diagnostic_arguments'],(0,i,3,values[6]))
            self.assertNotIn('prefix',outcomes[i])
        self.assertEqual(proof['payload_writes'],())
        self.assertTrue(proof['requires_machine_arguments_child_graph_and_source_lifetime'])

    def test_array_restores_own_storage_before_range_but_retains_both_metadata_frames(self):
        import config_helpers
        values=self.fixture(True);proof=config_helpers.index_prefix(*values,location_role='postcard_varint_usize')
        self.assertEqual(proof['conservative_frame_metadata'],(8,16))
        self.assertTrue(proof['restored_tail_storage'])
        outcomes={v['endpoint']:v for v in proof['outcomes']}
        self.assertEqual((outcomes[0]['prefix'].extent,outcomes[4]['prefix'].extent),(1,5))
        self.assertEqual(outcomes[5]['diagnostic_arguments'],(0,5,5,values[6]))

    def test_index_borrow_cannot_grant_protected_or_readonly_memory_or_foreign_location(self):
        import config_helpers
        from config_regions import Storage,borrow,Argument
        values=list(self.fixture())
        for fault in ('protected','ancestor','readonly','extent','domain','location'):
            current=list(values);role='postcard_serialize_u16'
            if fault=='protected':current[3]=borrow(Storage('stack:call','alias',-16,3,1,'active',True),0,3,'active',writable=True)
            if fault=='ancestor':current[5]=iter(values[5]+(Storage('stack:call','ancestor_alias',4,3,1,'active',False),))
            if fault=='readonly':current[3]=borrow(values[3].storage,4,3,'active-varint')
            if fault=='extent':current[3]=borrow(values[3].storage,4,4,'active-varint',writable=True)
            if fault=='domain':current[7]=Argument('u32',None,0,32,frozenset((0,)))
            if fault=='location':role='postcard_varint_usize'
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_helpers.index_prefix(*current,location_role=role)

    def test_array_effects_do_not_inherit_an_unmatched_child_body(self):
        import config_helpers
        values=list(self.fixture(True));objects,records=values[:2]
        child=next(n for n in objects.elf.functions if 'RangeToInclusivejEINtB5_10SliceIndexSh' in config_stack.canonical_symbol(n))
        at=(objects.elf.functions[child][0]&~1)+8;row=records[at];records[at]=Instruction(at,row.raw,'itttt','hi')
        with self.assertRaises(AssertionError):
            config_helpers.index_prefix(*values,location_role='postcard_varint_usize')


class OrdinaryDecodeEffects(unittest.TestCase):
    def fixture(self,semantic,raw,cursor=0):
        import config_decode_effects,config_helpers
        from config_regions import Storage,borrow
        objects,records,item=OrdinaryHelperBodies.fixture(self)
        payload_base=0x10070000;blob=bytes(12)+(raw or bytes(1));at=len(objects.elf.data)
        objects.elf.sections.append((0,1,2,payload_base,at,len(blob),0,0,1,0));objects.elf.data+=blob
        owner=Storage('address','embedded_eav',payload_base,len(blob),1,'decode',False)
        payload=borrow(owner,12,len(blob)-12,'decode')
        header_owner=Storage('stack:decode','D',128,8,4,'decode',True)
        header=borrow(header_owner,0,8,'decode',writable=True)
        state=config_decode_effects.DecoderState(header,payload,cursor)
        instance=next(v for v in item['instances'] if v['role']==semantic)
        proof=config_helpers.helper_coverage(objects,instance['name'],records,(instance['address'],'fixed decoder','caller still required'))
        frame=Storage('stack:decode',semantic,-proof['frame'],proof['frame'],8,'decode',True)
        return objects,records,proof,state,frame,()

    def run_effect(self,semantic,raw,cursor=0,**kwargs):
        import config_decode_effects
        args=self.fixture(semantic,raw,cursor)
        if semantic in ('take_varint_u32','deserialize_seed_f32'):
            from config_regions import Storage,borrow
            output=Storage('stack:decode','R8',144,8,4,'decode',True)
            kwargs.setdefault('output',borrow(output,0,8,'decode',writable=True))
        return config_decode_effects.decode_effect(*args,**kwargs)

    def test_slice_empty_and_byte_have_distinct_reads_writes_and_registers(self):
        empty=self.run_effect('slice_pop',b'\xff',1)
        self.assertEqual((empty['registers'],empty['state'].cursor,empty['input_reads'],empty['writes']),
                         ({'r0':1,'r1':4},1,(),()))
        success=self.run_effect('slice_pop',b'\xff')
        self.assertEqual((success['registers'],success['state'].cursor),({'r0':0,'r1':255},1))
        self.assertEqual([(off,data) for view,off,data in success['writes']],[(0,struct.pack('<I',0x1007000d))])
        self.assertEqual(success['input_reads'],((0x1007000c,1),))

    def test_u16_errors_retain_consumed_prefix_and_never_read_a_fourth_byte(self):
        cases=((b'\x00',0,1),(b'\x80\x00',0,2),(b'\xff\xff\x03',65535,3),
               (b'\xff\xff\x04',None,3),(b'\x80\x80\x80\x00',None,3),
               (b'\x80',None,1),(b'\x80\x80',None,2))
        for raw,value,consumed in cases:
            result=self.run_effect('deserialize_u16',raw)
            expected=(value<<16) if value is not None else ((4 if len(raw)<3 else 5)<<8)|1
            with self.subTest(raw=raw):
                self.assertEqual(result['registers'],{'r0':expected})
                self.assertEqual(result['state'].cursor,consumed)
                self.assertEqual(result['value'],value)
                self.assertEqual(len(result['input_reads']),consumed)

    def test_u32_result_only_initializes_its_actual_tag_and_payload_fields(self):
        for raw,value,error,consumed in ((b'\xff\xff\xff\xff\x0f',0xffffffff,None,5),
                                        (b'\x80\x00',0,None,2),(b'\x80',None,4,1),
                                        (b'\xff\xff\xff\xff\x10',None,5,5),
                                        (b'\x80\x80\x80\x80\x80\x00',None,5,5)):
            result=self.run_effect('take_varint_u32',raw)
            with self.subTest(raw=raw):
                self.assertEqual((result['value'],result['error'],result['state'].cursor),(value,error,consumed))
                self.assertEqual(result['initialized_result_fields'],((0,1),(4,4)) if error is None else ((0,1),(1,1)))
                self.assertEqual([(off,len(data)) for view,off,data in result['writes'] if view==result['output']],
                                 [(4,4),(0,1)] if error is None else [(1,1),(0,1)])

    def test_decoder_ordering_alias_and_child_mutations_reject(self):
        import config_decode_effects
        from config_regions import Storage,borrow
        for fault in ('past_end','header_save','ancestor','child'):
            objects,records,proof,state,frame,protected=self.fixture('deserialize_u16',b'\x01')
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                if fault=='past_end':state=config_decode_effects.DecoderState(state.header,state.payload,2)
                elif fault=='header_save':
                    owner=Storage(frame.space,'D-alias',frame.start,8,4,frame.phase,True)
                    state=config_decode_effects.DecoderState(borrow(owner,0,8,frame.phase,writable=True),state.payload,0)
                elif fault=='ancestor':protected=(Storage(frame.space,'saved-r8',state.header.start,4,4,frame.phase,False),)
                else:
                    child=next(row[2] for row in proof['ordinary_edges'] if row[3].endswith('6Flavor3pop'))
                    row=records[child];records[child]=Instruction(child,row.raw,'ldrd','r2, r3, [r1]')
                config_decode_effects.decode_effect(objects,records,proof,state,frame,protected)

    def test_empty_varints_keep_cursor_and_never_initialize_success_values(self):
        for semantic in ('deserialize_u16','take_varint_u32'):
            result=self.run_effect(semantic,b'\x80',1)
            self.assertEqual((result['error'],result['value'],result['state'].cursor,result['input_reads']),(4,None,1,()))
            self.assertFalse(any(view==result['state'].header for view,off,data in result['writes']))

    def test_header_result_alignment_mutability_and_full_aliases_reject(self):
        import config_decode_effects
        from config_regions import Storage,borrow
        for fault in ('misaligned_header','readonly_header','result_save','result_end_header','readonly_result','misaligned_result'):
            args=list(self.fixture('take_varint_u32',b'\x01'));state,frame=args[3:5]
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                if 'header' in fault and fault!='result_end_header':
                    owner=Storage(frame.space,'D',130 if fault=='misaligned_header' else 128,8,1,frame.phase,fault!='readonly_header')
                    args[3]=config_decode_effects.DecoderState(borrow(owner,0,8,frame.phase,writable=owner.writable),state.payload,0)
                start=frame.start if fault=='result_save' else 132 if fault=='result_end_header' else 146 if fault=='misaligned_result' else 144
                owner=Storage(frame.space,'R8',start,8,1 if fault=='misaligned_result' else 4,frame.phase,fault!='readonly_result')
                output=borrow(owner,0,8,frame.phase,writable=owner.writable)
                config_decode_effects.decode_effect(*args,output=output)

    def test_f32_keeps_all_bits_and_policy_with_unaligned_input_and_partial_errors(self):
        import config_decode_effects,config_platform
        from config_regions import Storage,borrow
        for residue in range(4):
            args=self.fixture('deserialize_seed_f32',bytes(residue)+bytes.fromhex('0100c07f'),residue)
            objects,records,proof,state,frame,protected=args
            policy=config_platform.reviewed_eav_policy(objects,state.payload,dict(target=config_platform.TARGET,core=0,
                security='secure',boot_context=config_platform.BOOT_CONTEXT),contradictory_effects=())
            owner=Storage(frame.space,'R8',144,8,4,frame.phase,True);output=borrow(owner,0,8,frame.phase,writable=True)
            result=config_decode_effects.decode_effect(*args,output=output,platform_policy=policy)
            self.assertEqual((result['value'],result['state'].cursor,result['platform_policy']),(0x7fc00001,residue+4,policy))
            with self.assertRaises(AssertionError):config_decode_effects.decode_effect(*args,output=output)
            changed=bytearray(objects.elf.data);changed[-1]^=1;objects.elf.data=bytes(changed)
            with self.assertRaises(AssertionError):config_decode_effects.decode_effect(*args,output=output,platform_policy=policy)
        for remaining in range(4):
            args=self.fixture('deserialize_seed_f32',b'\x01\x02\x03',3-remaining)
            objects,records,proof,state,frame,protected=args
            policy=config_platform.reviewed_eav_policy(objects,state.payload,dict(target=config_platform.TARGET,core=0,
                security='secure',boot_context=config_platform.BOOT_CONTEXT),contradictory_effects=())
            owner=Storage(frame.space,'R8',144,8,4,frame.phase,True);output=borrow(owner,0,8,frame.phase,writable=True)
            result=config_decode_effects.decode_effect(*args,output=output,platform_policy=policy)
            self.assertEqual((result['error'],result['state'].cursor,result['input_reads']),(4,3-remaining,()))
            self.assertEqual(result['initialized_result_fields'],((0,1),(1,1)))
            with self.assertRaises(AssertionError):
                config_decode_effects.decode_effect(*args,output=state.header,platform_policy=policy)


class OrdinarySequenceEffects(unittest.TestCase):
    def fixture(self,semantic,raw,count,cursor=0):
        import config_decode_effects
        from config_regions import Storage,borrow
        args=list(OrdinaryDecodeEffects.fixture(self,semantic,raw,cursor));state=args[3]
        owner=Storage('stack:decode','Q',160,8,4,'decode',True)
        args[3]=config_decode_effects.SequenceState(borrow(owner,0,8,'decode',writable=True),state,count)
        return args

    def run_effect(self,semantic,raw,count,cursor=0):
        import config_decode_effects,config_platform
        from config_regions import Storage,borrow
        args=self.fixture(semantic,raw,count,cursor);kwargs={}
        if semantic=='seq_f32':
            objects,records,proof,state,frame,protected=args
            owner=Storage(frame.space,'RO8',184,8,4,frame.phase,True)
            kwargs['output']=borrow(owner,0,8,frame.phase,writable=True)
            kwargs['platform_policy']=config_platform.reviewed_eav_policy(objects,state.decoder.payload,dict(target=config_platform.TARGET,
                core=0,security='secure',boot_context=config_platform.BOOT_CONTEXT),contradictory_effects=())
        return config_decode_effects.sequence_effect(*args,**kwargs)

    def test_zero_count_has_no_child_or_cursor_write_and_uses_each_none_abi(self):
        for semantic,registers in (('seq_u8',{'r0':0}),('seq_u16',{'r0':0}),('seq_bool',{'r0':0,'r1':2}),('seq_f32',{})):
            result=self.run_effect(semantic,b'\xff',0)
            self.assertEqual((result['registers'],result['state'].count,result['state'].decoder.cursor,result['children']),
                             (registers,0,0,()))
            self.assertFalse(any(view in (result['state'].header,result['state'].decoder.header) for view,off,data in result['writes']))
            if semantic=='seq_f32':self.assertEqual(result['initialized_result_fields'],((0,4),))

    def test_positive_count_decrements_before_empty_input_error_without_rollback(self):
        for semantic,registers in (('seq_u8',{'r0':0x402}),('seq_u16',{'r0':0x40002}),('seq_bool',{'r0':1,'r1':4})):
            result=self.run_effect(semantic,b'\xff',2,1)
            self.assertEqual((result['registers'],result['state'].count,result['state'].decoder.cursor),(registers,1,1))
            self.assertEqual((result['writes'][0][1],result['writes'][0][2]),(4,struct.pack('<I',1)))
        result=self.run_effect('seq_u16',b'\x80',1)
        self.assertEqual((result['state'].count,result['state'].decoder.cursor,result['registers']),(0,1,{'r0':0x40002}))

    def test_some_and_bad_bool_keep_distinct_register_layouts(self):
        for semantic,raw,registers in (('seq_u8',b'\xff',{'r0':0xff01}),('seq_u16',b'\xff\xff\x03',{'r0':0xffff0001}),
                                      ('seq_bool',b'\x00',{'r0':0,'r1':0}),('seq_bool',b'\x01',{'r0':0,'r1':1}),
                                      ('seq_bool',b'\x02',{'r0':1,'r1':6})):
            result=self.run_effect(semantic,raw,1)
            self.assertEqual(result['registers'],registers)
            self.assertEqual((result['state'].count,result['state'].decoder.cursor),(0,len(raw)))

    def test_f32_only_reads_initialized_child_value_and_preserves_conditional_policy(self):
        for raw,tag,fields,cursor in ((bytes.fromhex('0000803f'),1,((0,4),(4,4)),4),(b'\x01\x02',2,((0,4),(4,1)),0)):
            result=self.run_effect('seq_f32',raw,1)
            self.assertEqual((result['tag'],result['initialized_result_fields'],result['state'].decoder.cursor),(tag,fields,cursor))
            self.assertEqual(result['platform_policy'],result['children'][0]['platform_policy'])
            self.assertEqual(result['state'].count,0)

    def test_q_alias_saved_protection_and_wrong_count_domains_reject(self):
        import config_decode_effects
        from config_regions import Storage,borrow
        for fault in ('D_alias','saved_alias','ancestor','negative','bool_count'):
            args=self.fixture('seq_u8',b'\x01',1);state,frame=args[3:5]
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                if fault in ('negative','bool_count'):
                    args[3]=config_decode_effects.SequenceState(state.header,state.decoder,-1 if fault=='negative' else True)
                elif fault=='ancestor':args[5]=(Storage(frame.space,'saved_r11',state.header.start+4,4,4,frame.phase,False),)
                else:
                    owner=Storage(frame.space,'Q-alias',state.decoder.header.start if fault=='D_alias' else frame.start,8,4,frame.phase,True)
                    args[3]=config_decode_effects.SequenceState(borrow(owner,0,8,frame.phase,writable=True),state.decoder,1)
                config_decode_effects.sequence_effect(*args)

    def test_entire_live_D_header_must_survive_parent_and_child_prologue(self):
        import config_decode_effects
        from config_regions import Storage,borrow
        for semantic,start in (('slice_pop',-20),('deserialize_u16',-44)):
            args=list(OrdinaryDecodeEffects.fixture(self,semantic,b'\x01'));state,frame=args[3:5]
            owner=Storage(frame.space,'D-end-alias',start,8,4,frame.phase,True)
            args[3]=config_decode_effects.DecoderState(borrow(owner,0,8,frame.phase,writable=True),state.payload,0)
            with self.subTest(semantic=semantic),self.assertRaises(AssertionError):
                config_decode_effects.decode_effect(*args)

    def test_f32_external_Q_D_and_RO8_cannot_borrow_pushed_scratch_even_on_none(self):
        import config_decode_effects,config_platform
        from config_regions import Storage,borrow
        for fault in ('Q','D','RO8'):
            args=self.fixture('seq_f32',bytes(4),0);objects,records,proof,state,frame,protected=args
            policy=config_platform.reviewed_eav_policy(objects,state.decoder.payload,dict(target=config_platform.TARGET,
                core=0,security='secure',boot_context=config_platform.BOOT_CONTEXT),contradictory_effects=())
            alias=Storage(frame.space,'prologue-scratch',frame.start,8,4,frame.phase,True)
            view=borrow(alias,0,8,frame.phase,writable=True)
            out_owner=Storage(frame.space,'RO8',184,8,4,frame.phase,True);output=borrow(out_owner,0,8,frame.phase,writable=True)
            if fault=='Q':args[3]=config_decode_effects.SequenceState(view,state.decoder,0)
            elif fault=='D':args[3]=config_decode_effects.SequenceState(state.header,
                config_decode_effects.DecoderState(view,state.decoder.payload,0),0)
            else:output=view
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_decode_effects.sequence_effect(*args,output=output,platform_policy=policy)


class OrdinaryCompareEffects(unittest.TestCase):
    def fixture(self,raw=b'abc',source=b'ab',cursor=0):
        import config_compare_effects
        from config_regions import Storage,borrow
        objects,records,proof,state,frame,protected=OrdinaryDecodeEffects.fixture(self,'compare_extend',raw,cursor)
        donor,other_records,item=LoggerBodies.fixture(self,'sizez');start=donor.elf.functions['memcmp'][0]&~1
        blob=donor.read_only(start,116);section=next(s for s in objects.elf.sections if s[2]&4 and s[3]<=start<start+116<=s[3]+s[5])
        at=section[4]+start-section[3];data=bytearray(objects.elf.data);data[at:at+116]=blob;objects.elf.data=bytes(data)
        records.update({at:row for at,row in other_records.items() if start<=at<start+116})
        source_owner=Storage(frame.space,'serialized-bytes',176,max(len(source),1),4,frame.phase,True)
        encoded=config_compare_effects.SourceBytes(borrow(source_owner,0,source_owner.extent,frame.phase),source)
        compare=config_compare_effects.CompareState(state.header,state.payload,state.cursor)
        return objects,records,proof,compare,encoded,frame,protected

    def test_equal_prefix_advances_but_mismatch_or_insufficient_length_does_not(self):
        import config_compare_effects
        for raw,source,cursor,status,writes in ((b'abc',b'ab',2,255,1),(b'abc',b'ax',0,2,0),(b'a',b'ab',0,2,0)):
            result=config_compare_effects.compare_effect(*self.fixture(raw,source))
            self.assertEqual((result['state'].cursor,result['registers'],len(result['writes'])),(cursor,{'r0':status},writes))
            self.assertEqual(result['input_read_envelopes'],() if len(raw)<len(source) else ((0x1007000c,len(source)),(176,len(source))))

    def test_empty_extension_keeps_valid_slice_and_writes_unchanged_header(self):
        import config_compare_effects
        result=config_compare_effects.compare_effect(*self.fixture(b'a',b'',1))
        self.assertEqual((result['state'].cursor,result['registers'],result['input_read_envelopes']),(1,{'r0':255},()))
        self.assertEqual(result['writes'][0][2],struct.pack('<II',0x1007000d,0))
        self.assertEqual(result['conservative_activation_frames'],(24,32))

    def test_relative_stack_zero_is_a_valid_source_coordinate_not_a_null_pointer(self):
        import config_compare_effects
        from config_regions import Storage,borrow
        args=list(self.fixture());frame=args[5]
        owner=Storage(frame.space,'relative-source',0,2,4,frame.phase,True)
        args[4]=config_compare_effects.SourceBytes(borrow(owner,0,2,frame.phase),b'ab')
        self.assertEqual(config_compare_effects.compare_effect(*args)['registers'],{'r0':255})

    def test_full_C_header_and_source_bytes_survive_own_and_memcmp_frames(self):
        import config_compare_effects
        from config_regions import Storage,borrow
        for fault in ('C_end_save','C_source_alias','source_memcmp_save','C_memcmp_save'):
            args=list(self.fixture());state,source,frame=args[3:6]
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                if fault.startswith('C_'):
                    start=-28 if fault=='C_end_save' else -60 if fault=='C_memcmp_save' else source.view.start
                    owner=Storage(frame.space,'C-alias',start,8,4,frame.phase,True)
                    args[3]=config_compare_effects.CompareState(borrow(owner,0,8,frame.phase,writable=True),state.payload,0)
                else:
                    owner=Storage(frame.space,'source-alias',-56,2,4,frame.phase,True)
                    args[4]=config_compare_effects.SourceBytes(borrow(owner,0,2,frame.phase),b'ab')
                config_compare_effects.compare_effect(*args)

    def test_memcmp_body_and_bound_readonly_source_identity_are_required(self):
        import config_compare_effects
        from config_regions import borrow
        for fault in ('memcmp','source_bytes'):
            args=list(self.fixture());objects,records=args[:2]
            if fault=='memcmp':
                start=objects.elf.functions['memcmp'][0]&~1;row=records[start];records[start]=Instruction(start,row.raw,'bx','lr')
            else:args[4]=config_compare_effects.SourceBytes(borrow(args[3].payload.storage,12,2,args[3].payload.storage.phase),b'zz')
            with self.subTest(fault=fault),self.assertRaises(AssertionError):config_compare_effects.compare_effect(*args)


class OrdinarySerializeEffects(unittest.TestCase):
    def fixture(self,semantic,raw,kind,bits,value):
        import config_helpers,config_compare_effects
        from config_regions import Argument,Storage
        objects,records,unused,state,source,unused_frame,protected=OrdinaryCompareEffects.fixture(self,raw,b'')
        item=json.loads((Path(__file__).parent/'fixtures/config-small-helpers.json').read_text())[0]
        instance=next(v for v in item['instances'] if v['role']==semantic)
        proof=config_helpers.helper_coverage(objects,instance['name'],records,(instance['address'],'serializer','source field still required'))
        frame=Storage(state.header.storage.space,semantic,-proof['frame'],proof['frame'],8,state.header.storage.phase,True)
        argument=Argument(kind,None,0,bits,frozenset((value,)))
        return objects,records,proof,state,argument,frame,protected

    def test_scalar_varint_and_hard_float_bytes_have_exact_distinct_status_ABIs(self):
        import config_serialize_effects
        cases=(('serialize_u8',b'\xff','u8',8,255,0xffffffff),('serialize_u16',b'\xff\xff\x03','u16',16,65535,0xffffffff),
               ('varint_usize',b'\xff\xff\xff\xff\x0f','usize',32,0xffffffff,255),
               ('varint_u32_small',b'\x7f','u32',32,127,255),('serialize_f32',bytes.fromhex('0100c07f'),'f32_bits',32,0x7fc00001,0xffffffff))
        for semantic,raw,kind,bits,value,status in cases:
            result=config_serialize_effects.serialize_effect(*self.fixture(semantic,raw,kind,bits,value))
            self.assertEqual((result['registers'],result['state'].cursor,result['encoded']),({'r0':status},len(raw),raw))
            self.assertTrue(result['requires_machine_source_field_arguments_lifetime_ingress_and_enclosing_graph'])

    def test_compare_error_never_advances_current_C_for_one_scalar(self):
        import config_serialize_effects
        result=config_serialize_effects.serialize_effect(*self.fixture('serialize_u16',b'\x00','u16',16,128))
        self.assertEqual((result['registers'],result['state'].cursor),({'r0':2},0))

    def test_u16_group_boundaries_and_nonminimal_decoded_zero_stay_canonical(self):
        import config_serialize_effects
        for number,raw in ((0,b'\x00'),(127,b'\x7f'),(128,b'\x80\x01'),(16383,b'\xff\x7f'),(16384,b'\x80\x80\x01')):
            result=config_serialize_effects.serialize_effect(*self.fixture('serialize_u16',raw,'u16',16,number))
            self.assertEqual((result['encoded'],result['state'].cursor),(raw,len(raw)))
        import config_decode_effects
        decoded=config_decode_effects.decode_effect(*OrdinaryDecodeEffects.fixture(self,'deserialize_u16',b'\x80\x00'))
        result=config_serialize_effects.serialize_effect(*self.fixture('serialize_u16',b'\x80\x00','u16',16,decoded['value']))
        self.assertEqual((decoded['value'],decoded['state'].cursor,result['error'],result['state'].cursor),(0,2,2,0))

    def test_specialized_u32_and_scalar_or_hard_float_argument_domains_reject(self):
        import config_serialize_effects
        from config_regions import Argument
        for semantic,kind,bits,value in (('varint_u32_small','u32',32,128),('serialize_f32','u32',32,0),
                                        ('serialize_u8','u16',16,256)):
            with self.subTest(semantic=semantic),self.assertRaises(AssertionError):
                config_serialize_effects.serialize_effect(*self.fixture(semantic,b'\x00',kind,bits,value))
        args=list(self.fixture('serialize_u8',b'\x00','u8',8,0));args[4]=Argument('u8',None,0,8,frozenset((0,1)))
        with self.assertRaises(AssertionError):config_serialize_effects.serialize_effect(*args)

    def test_string_prefix_success_remains_consumed_when_body_mismatches(self):
        import config_serialize_effects,config_helpers,config_compare_effects
        from config_regions import Storage,borrow
        objects,records,unused,state,source,unused_frame,protected=OrdinaryCompareEffects.fixture(self,b'\x02ax',b'ab')
        item=json.loads((Path(__file__).parent/'fixtures/config-small-helpers.json').read_text())[0]
        instance=next(v for v in item['instances'] if v['role']=='serialize_str')
        proof=config_helpers.helper_coverage(objects,instance['name'],records,(instance['address'],'string serializer','source field still required'))
        frame=Storage(state.header.storage.space,'serialize_str',-24,24,8,state.header.storage.phase,True)
        result=config_serialize_effects.serialize_effect(objects,records,proof,state,source,frame,())
        self.assertEqual((result['registers'],result['state'].cursor),({'r0':2},1))
        self.assertEqual(len(result['children']),2)
        invalid=config_compare_effects.SourceBytes(source.view,b'\xff')
        with self.assertRaises(AssertionError):config_serialize_effects.serialize_effect(objects,records,proof,state,invalid,frame,())
        alias=Storage(frame.space,'live-string-under-prefix',-88,2,4,frame.phase,True)
        threatened=config_compare_effects.SourceBytes(borrow(alias,0,2,frame.phase),b'ab')
        with self.assertRaises(AssertionError):config_serialize_effects.serialize_effect(objects,records,proof,state,threatened,frame,())


class OrdinaryHeaderConstructors(unittest.TestCase):
    def fixture(self,code_shift=0,data_shift=0):
        from config_regions import Storage
        objects,item=LoggerObjects.fixture(self,'sizez',code_shift,data_shift,fixture_file='config-headers.json')
        objects.elf.mapping_symbols=[(n,a+code_shift,i) for n,a,i in item['mapping_symbols']]
        records={}
        for row in item['records']:
            op=row['text'].split(None,1);at=row['address']+code_shift;operands=op[1] if len(op)>1 else ''
            if config_stack.transfer_family(op[0].removesuffix('.w')) in ('b','bl','cbz','cbnz'):
                operands=re.sub(r'0x([0-9a-f]+)',lambda m:hex(int(m[1],16)+code_shift),operands)
            records[at]=Instruction(at,bytes.fromhex(row['bytes']),op[0],operands)
        data=bytearray(objects.elf.data);pool=0x10020d74+code_shift
        section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=pool<pool+4<=s[3]+s[5])
        offset=section[4]+pool-section[3];struct.pack_into('<I',data,offset,struct.unpack_from('<I',data,offset)[0]+data_shift)
        objects.elf.data=bytes(data)
        root=next(n for n in objects.elf.functions if 'ConfigSlot12load_at_boot' in n)
        visitor=next(n for n in objects.elf.functions if '10ConfigSeed' in n)
        frame=Storage('stack:header','load_at_boot',0,456,8,'decode',True)
        child=Storage(frame.space,'ConfigSeed',-1608,1608,8,frame.phase,True)
        return objects,records,root,visitor,frame,child

    def test_current_D_words_and_child_Q_count_bind_source_payload_and_actual_owner(self):
        import config_headers
        objects,records,root,visitor,frame,child=self.fixture()
        boot=config_headers.root_decoder_header(objects,root,records,frame,())
        state=boot['decoder_state']
        self.assertEqual((state.header.offset,state.header.extent,state.payload.start,state.payload.extent,state.cursor),
                         (236,8,0x10032b2a,1145,0))
        nested=config_headers.visitor_sequence_header(objects,visitor,records,child,state,boot['protected'])
        self.assertEqual((nested['sequence_state'].header.offset,nested['sequence_state'].count),(288,13))
        self.assertEqual(nested['sequence_state'].decoder,state)
        self.assertEqual((boot['state_coordinate'],nested['state_coordinate']),('before_call','before_call'))
        self.assertTrue(boot['requires_first_consumer_effect_before_exit'])
        self.assertTrue(nested['requires_first_consumer_effect_before_exit'])
        self.assertTrue(boot['checksum_failure_requires_fault_mapping_and_default_successor'])
        self.assertTrue(boot['requires_parent_entry_all_ingress_source_effects_and_coverage'])

    def test_code_and_EAV_relocations_do_not_change_constructor_meaning(self):
        import config_headers
        objects,records,root,visitor,frame,child=self.fixture(0x2800,0x19000)
        boot=config_headers.root_decoder_header(objects,root,records,frame,())
        nested=config_headers.visitor_sequence_header(objects,visitor,records,child,boot['decoder_state'],boot['protected'])
        self.assertEqual(nested['sequence_state'].decoder.payload.start,0x10032b2a+0x19000)

    def test_actual_root_call_constructs_its_child_frame_and_same_D_pointer(self):
        import config_headers
        objects,records,root,visitor,frame,child=self.fixture()
        bridge=config_headers.root_visitor_headers(objects,root,records,frame,())
        self.assertEqual(bridge['visitor_frame'],child)
        self.assertEqual(bridge['visitor']['sequence_state'].decoder,bridge['root']['decoder_state'])
        self.assertEqual(bridge['state_coordinate'],'before_first_next_call')
        start=objects.elf.functions[root][0]&~1
        records[start+0x100]=Instruction(start+0x100,bytes(2),'b',hex(start+120))
        with self.assertRaises(AssertionError):config_headers.root_visitor_headers(objects,root,records,frame,())

    def test_changed_valid_envelope_bytes_rebind_crc_without_hash_allowlist(self):
        import config_headers
        import zlib
        objects,records,root,visitor,frame,child=self.fixture();data=bytearray(objects.elf.data)
        def offset(address):
            section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=address<s[3]+s[5])
            return section[4]+address-section[3]
        eav=offset(0x10032b1e);data[eav+100]^=1
        raw=data[eav:eav+1157];checksum=zlib.crc32(raw[:8]+raw[12:])
        struct.pack_into('<I',data,eav+8,checksum);struct.pack_into('<I',data,offset(0x100200bc),checksum^0xffffffff)
        objects.elf.data=bytes(data)
        result=config_headers.root_decoder_header(objects,root,records,frame,())
        self.assertEqual(result['decoder_state'].payload.extent,1145)
        self.assertTrue(result['requires_parent_entry_all_ingress_source_effects_and_coverage'])

    def test_changed_EAV_crc_pool_or_frame_cannot_materialize_valid_D(self):
        import config_headers
        from config_regions import Storage
        for fault in ('eav','polynomial','checksum_pool','frame'):
            objects,records,root,visitor,frame,child=self.fixture()
            if fault=='frame':frame=Storage(frame.space,frame.identity,0,448,8,frame.phase,True)
            else:
                site={'eav':0x10032b2a,'polynomial':0x100200b8,'checksum_pool':0x100200bc}[fault]
                section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=site<s[3]+s[5]);offset=section[4]+site-section[3]
                data=bytearray(objects.elf.data);data[offset]^=1;objects.elf.data=bytes(data)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):config_headers.root_decoder_header(objects,root,records,frame,())

    def test_D_end_register_Q_field_count_or_layout_mutations_reject(self):
        import config_headers
        for fault in ('D_end','Q_count','Q_offset','Q_pointer'):
            objects,records,root,visitor,frame,child=self.fixture()
            boot=config_headers.root_decoder_header(objects,root,records,frame,())
            if fault=='D_end':site=0x1001fedc;operation,operands='strd','r8, r1, [sp, #236]'
            elif fault=='Q_count':site=0x1001c664;operation,operands='movs','r1, #0xc'
            elif fault=='Q_offset':site=0x1001c666;operation,operands='strd','r0, r1, [sp, #292]'
            else:site=0x1001c66a;operation,operands='add','r0, sp, #0x124'
            row=records[site];records[site]=Instruction(site,row.raw,operation,operands)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                if fault=='D_end':config_headers.root_decoder_header(objects,root,records,frame,())
                else:config_headers.visitor_sequence_header(objects,visitor,records,child,boot['decoder_state'],boot['protected'])


class OrdinaryFirstFieldBridge(unittest.TestCase):
    def fixture(self):
        objects,records,root,visitor,frame,child=OrdinaryHeaderConstructors.fixture(self)
        donor,other,item=OrdinaryHelperBodies.fixture(self)
        data=bytearray(objects.elf.data)
        for index,address,encoded in item['segments']:
            blob=bytes.fromhex(encoded);section=objects.elf.sections[index];offset=section[4]+address-section[3]
            data[offset:offset+len(blob)]=blob
        objects.elf.data=bytes(data);objects.elf.functions.update(donor.elf.functions)
        objects.elf.symbol_entries=list(dict.fromkeys(objects.elf.symbol_entries+donor.elf.symbol_entries))
        objects.sizes.update(donor.sizes);records.update(other)
        return objects,records,root,visitor,frame,child

    def test_actual_first_next_return_updates_same_D_Q_and_exact_wrapper_ABI(self):
        import config_headers
        objects,records,root,visitor,frame,child=self.fixture()
        result=config_headers.root_first_u16(objects,root,records,frame,())
        returned=result['first_return']
        self.assertEqual((returned['value'],returned['error'],returned['registers']),(4,None,{'r0':0x40e00}))
        self.assertEqual((returned['state'].count,returned['state'].decoder.cursor),(12,1))
        self.assertEqual(result['state_coordinate'],'after_first_next_return')

    def test_none_and_error_keep_outer_result_distinct_from_inner_sequence(self):
        import config_headers
        from dataclasses import replace
        from config_regions import Storage
        objects,records,root,visitor,frame,child=self.fixture()
        before=config_headers.root_visitor_headers(objects,root,records,frame,())
        sequence=before['visitor']['sequence_state']
        name=next(n for n in objects.elf.functions if config_stack.canonical_symbol(n)==config_headers.NEXT_U16)
        active=Storage(frame.space,'next_u16',child.start-8,8,8,frame.phase,True)
        for count,cursor,error,packed in ((0,0,14,0xe01),(1,sequence.decoder.payload.extent,4,0x401)):
            state=replace(sequence,count=count,decoder=replace(sequence.decoder,cursor=cursor))
            result=config_headers.next_u16_effect(objects,name,records,state,active,before['visitor']['protected'])
            self.assertEqual((result['error'],result['registers']),(error,{'r0':packed}))
            self.assertEqual(result['state'].count,max(0,count-1))
        from config_regions import borrow
        base=0x10070000;offset=len(objects.elf.data);objects.elf.data+=b'\x80\x80\x80'
        objects.elf.sections.append((0,1,2,base,offset,3,0,0,1,0))
        owner=Storage('address','bounded-malformed-input',base,3,1,frame.phase,False)
        state=replace(sequence,count=1,decoder=replace(sequence.decoder,payload=borrow(owner,0,3,frame.phase),cursor=0))
        result=config_headers.next_u16_effect(objects,name,records,state,active,before['visitor']['protected'])
        self.assertEqual((result['error'],result['registers'],result['state'].count,result['state'].decoder.cursor),
                         (5,{'r0':0x501},0,3))

    def test_changed_wrapper_tag_mask_or_nested_decoder_body_rejects(self):
        import config_headers
        for fault in ('tag_mask','decoder'):
            objects,records,root,visitor,frame,child=self.fixture()
            if fault=='tag_mask':site=0x1001bd9e;mnemonic,operands='addne.w','r1, r2, #0xf00'
            else:site=0x1001b844;mnemonic,operands='and','r0, r1, #0x3f'
            row=records[site];records[site]=Instruction(site,row.raw,mnemonic,operands)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):config_headers.root_first_u16(objects,root,records,frame,())


class OrdinaryScalarFields(unittest.TestCase):
    def fixture(self):
        objects,records,root,visitor,frame,child=OrdinaryFirstFieldBridge.fixture(self)
        donor,item=LoggerObjects.fixture(self,'sizez',fixture_file='config-entry-bodies.json')
        import config_entry
        entry=next(e for e in donor.elf.symbol_entries if config_entry.static_role(e[0])==config_entry.CONFIG)
        index,address,size=entry[4],entry[1],entry[2];section=objects.elf.sections[index]
        source=donor.elf.sections[index];data=bytearray(objects.elf.data)
        offset=section[4]+address-section[3];other=source[4]+address-source[3]
        data[offset:offset+size]=donor.elf.data[other:other+size];objects.elf.data=bytes(data)
        objects.elf.symbol_entries.append(entry)
        return objects,records,root,visitor,frame,child

    def test_two_source_ordered_fields_remain_partial_and_unpublished(self):
        import config_headers
        objects,records,root,visitor,frame,child=self.fixture()
        result=config_headers.root_u16_fields(objects,root,records,frame,())
        self.assertEqual([(f['field'],f['offset'],f['extent']) for f in result['fields']],
                         [('version',14570,2),('catalogue_version',14572,2)])
        self.assertEqual(result['fields'][0]['value'],4)
        self.assertEqual((result['state'].count,result['state'].decoder.cursor),(11,2))
        self.assertEqual(result['state_coordinate'],'before_boot_view_call')
        self.assertTrue(result['requires_exclusive_initialized_Config_source_owner_and_all_remaining_phases'])
        self.assertNotIn('validated_config',result)

    def test_store_width_field_offset_source_register_and_call_argument_mutations_reject(self):
        import config_headers
        for site,mnemonic,operands in ((0x1001c67e,'str','r0, [r4, r1]'),
            (0x1001c67a,'movw','r1, #0x38ec'),(0x1001c6a2,'strh','r1, [r4, r1]'),
            (0x1001c680,'add','r0, sp, #0x124')):
            objects,records,root,visitor,frame,child=self.fixture();row=records[site]
            records[site]=Instruction(site,row.raw,mnemonic,operands)
            with self.subTest(site=site),self.assertRaises(AssertionError):config_headers.root_u16_fields(objects,root,records,frame,())

    def test_named_destination_not_arbitrary_writable_memory_or_aliased_stack(self):
        import config_headers,config_entry
        for fault in ('identity','taken','extent'):
            objects,records,root,visitor,frame,child=self.fixture()
            entry=next(e for e in objects.elf.symbol_entries if config_entry.static_role(e[0])==config_entry.CONFIG)
            if fault=='taken':
                data=bytearray(objects.elf.data);section=objects.elf.sections[entry[4]]
                data[section[4]+entry[1]-section[3]+14600]=1;objects.elf.data=bytes(data)
            else:
                altered=list(entry);altered[0 if fault=='identity' else 2]='other' if fault=='identity' else 14600
                objects.elf.symbol_entries[objects.elf.symbol_entries.index(entry)]=tuple(altered)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):config_headers.root_u16_fields(objects,root,records,frame,())

    def test_each_decode_error_stops_before_store_or_next_field_without_publishing(self):
        import config_headers,zlib
        for prefix,fields,count in ((b'\x80\x80\x80',0,12),(b'\x04\x80\x80\x80',1,11)):
            objects,records,root,visitor,frame,child=self.fixture();data=bytearray(objects.elf.data)
            def position(address):
                section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=address<s[3]+s[5])
                return section[4]+address-section[3]
            at=position(0x10032b1e);data[at+12:at+12+len(prefix)]=prefix
            raw=data[at:at+1157];checksum=zlib.crc32(raw[:8]+raw[12:])
            struct.pack_into('<I',data,at+8,checksum);struct.pack_into('<I',data,position(0x100200bc),checksum^0xffffffff)
            objects.elf.data=bytes(data)
            result=config_headers.root_u16_fields(objects,root,records,frame,())
            self.assertEqual((len(result['fields']),result['error'],result['state'].count,result['state'].decoder.cursor),
                             (fields,5,count,len(prefix)))
            self.assertEqual(result['state_coordinate'],'before_shared_error_suffix')
            self.assertTrue(result['requires_shared_error_restore_default_and_remaining_success_continuations'])


class AbstractPrimitiveInterfaces(unittest.TestCase):
    def decoder(self,semantic,raw=b'\x00\x01\x02'):
        import config_abstract
        args=list(OrdinaryDecodeEffects.fixture(self,semantic,raw))
        args[3]=config_abstract.DecoderInterface(args[3].header,args[3].payload,config_abstract.Span(0,len(raw)))
        return args

    def sequence(self,semantic,raw=b'\x00\x01\x02'):
        import config_abstract
        args=list(OrdinarySequenceEffects.fixture(self,semantic,raw,2))
        source=args[3]
        decoder=config_abstract.DecoderInterface(source.decoder.header,source.decoder.payload,config_abstract.Span(0,len(raw)))
        args[3]=config_abstract.SequenceInterface(source.header,decoder,config_abstract.Span(0,2))
        return args

    def test_unknown_scalars_union_all_guarded_outcomes_without_reading_value_bytes(self):
        import config_abstract
        signatures=[]
        for raw in (b'\x00\x00\x00',b'\xff\xff\xff'):
            result=config_abstract.decoder_interface(*self.decoder('deserialize_u16',raw))
            self.assertEqual({o['kind'] for o in result['outcomes']},{'success','error4','error5'})
            success=next(o for o in result['outcomes'] if o['kind']=='success')
            self.assertEqual((success['value_kind'],success['value_domain']),('u16',(0,65535)))
            self.assertNotIn('value',success)
            signatures.append([(o['kind'],o['cursor']) for o in result['outcomes']])
            self.assertTrue(result['requires_actual_stage_receiver_transport_and_complete_parent_graph'])
        self.assertEqual(signatures[0],signatures[1])

    def test_sequence_retains_none_some_and_both_bool_errors_as_paired_alternatives(self):
        import config_abstract
        result=config_abstract.sequence_interface(*self.sequence('seq_bool'))
        self.assertEqual({o['kind'] for o in result['outcomes']},{'none','some','error4','error6'})
        for outcome in result['outcomes']:
            self.assertEqual(outcome['count'],config_abstract.Span(0,0) if outcome['kind']=='none' else config_abstract.Span(0,1))
            if outcome['kind']=='some':self.assertEqual(outcome['value_domain'],(0,1))
        self.assertEqual(result['conservative_activation_frames'],(8,16))

    def test_remaining_fixed_primitive_families_keep_unknown_widths_and_result_initialization(self):
        import config_abstract
        from config_regions import Storage,borrow
        args=self.decoder('take_varint_u32',bytes(5));frame=args[4]
        output=borrow(Storage(frame.space,'R8',144,8,4,frame.phase,True),0,8,frame.phase,writable=True)
        result=config_abstract.decoder_interface(*args,output=output)
        self.assertEqual(result['conservative_activation_frames'],(32,16))
        for alternative in result['outcomes']:
            self.assertEqual(alternative['initialized_result_fields'],((0,1),(4,4)) if alternative['kind']=='success' else ((0,1),(1,1)))
        with self.assertRaises(AssertionError):config_abstract.decoder_interface(*args,output=args[3].header)
        for semantic,domain,frames in (('seq_u8',(0,255),(8,16)),('seq_u16',(0,65535),(8,24,16))):
            result=config_abstract.sequence_interface(*self.sequence(semantic))
            self.assertEqual(next(o['value_domain'] for o in result['outcomes'] if o['kind']=='some'),domain)
            self.assertEqual(result['conservative_activation_frames'],frames)

    def test_f32_union_preserves_policy_on_none_error_and_success_without_alignment4(self):
        import config_abstract,config_platform
        from config_regions import Storage,borrow
        args=self.sequence('seq_f32',bytes(range(7)));state,frame=args[3:5]
        output=borrow(Storage(frame.space,'RO8',184,8,4,frame.phase,True),0,8,frame.phase,writable=True)
        policy=config_platform.reviewed_eav_policy(args[0],state.decoder.payload,dict(target=config_platform.TARGET,
            core=0,security='secure',boot_context=config_platform.BOOT_CONTEXT),contradictory_effects=())
        with self.assertRaises(AssertionError):config_abstract.sequence_interface(*args,output=output)
        result=config_abstract.sequence_interface(*args,output=output,platform_policy=policy)
        self.assertEqual({o['kind'] for o in result['outcomes']},{'none','some','error4'})
        self.assertEqual(result['platform_policy'],policy)
        some=next(o for o in result['outcomes'] if o['kind']=='some')
        self.assertEqual(some['cursor'],config_abstract.Span(4,7))
        self.assertEqual(some['initialized_result_fields'],((0,4),(4,4)))
        args[0].elf.data+=b'changed'
        with self.assertRaises(AssertionError):config_abstract.sequence_interface(*args,output=output,platform_policy=policy)

    def test_grouped_effects_protect_full_headers_outputs_and_save_cells(self):
        import config_abstract
        from config_regions import Storage,borrow
        for source in ('D_end','Q_save'):
            args=self.decoder('slice_pop') if source=='D_end' else self.sequence('seq_u8')
            state,frame=args[3:5]
            owner=Storage(frame.space,'alias',frame.start-4 if source=='D_end' else frame.start,8,4,frame.phase,True)
            if source=='D_end':args[3]=config_abstract.DecoderInterface(borrow(owner,0,8,frame.phase,writable=True),state.payload,state.cursor)
            else:args[3]=config_abstract.SequenceInterface(borrow(owner,0,8,frame.phase,writable=True),state.decoder,config_abstract.Span(0,0))
            with self.subTest(source=source),self.assertRaises(AssertionError):
                (config_abstract.decoder_interface if source=='D_end' else config_abstract.sequence_interface)(*args)

    def test_current_child_mutation_and_invalid_domains_reject(self):
        import config_abstract
        for lower,upper in ((-1,2),(0,True),(3,2)):
            with self.assertRaises(AssertionError):config_abstract.Span(lower,upper)
        args=self.decoder('deserialize_u16');row=args[1][0x1001b844]
        args[1][row.address]=Instruction(row.address,row.raw,'and','r0, r1, #0x3f')
        with self.assertRaises(AssertionError):config_abstract.decoder_interface(*args)

    def test_exact_helper_does_not_silently_accept_abstract_state(self):
        import config_decode_effects
        with self.assertRaises(AssertionError):config_decode_effects.decode_effect(*self.decoder('slice_pop'))

    def test_narrow_cursor_and_count_endpoints_keep_exact_paired_exit_spans(self):
        import config_abstract,config_platform
        from dataclasses import replace
        from config_regions import Storage,borrow
        cases=(('slice_pop',8,8,{'error4':(8,8)}),('slice_pop',7,7,{'success':(8,8)}),
            ('deserialize_u16',8,8,{'error4':(8,8)}),('deserialize_u16',7,7,{'success':(8,8),'error4':(8,8)}),
            ('deserialize_u16',5,5,{'success':(6,8),'error5':(8,8)}),
            ('deserialize_u16',5,6,{'success':(6,8),'error4':(8,8),'error5':(8,8)}),
            ('take_varint_u32',3,3,{'success':(4,8),'error5':(8,8)}),
            ('take_varint_u32',4,4,{'success':(5,8),'error4':(8,8)}),
            ('deserialize_seed_f32',8,8,{'error4':(8,8)}),('deserialize_seed_f32',7,7,{'error4':(7,7)}),
            ('deserialize_seed_f32',5,5,{'error4':(5,5)}),('deserialize_seed_f32',4,4,{'success':(8,8)}))
        for semantic,lower,upper,expected in cases:
            args=self.decoder(semantic,bytes(8));args[3]=replace(args[3],cursor=config_abstract.Span(lower,upper));kwargs={}
            if semantic in ('take_varint_u32','deserialize_seed_f32'):
                frame=args[4];kwargs['output']=borrow(Storage(frame.space,'R8',144,8,4,frame.phase,True),0,8,frame.phase,writable=True)
            if semantic=='deserialize_seed_f32':
                kwargs['platform_policy']=config_platform.reviewed_eav_policy(args[0],args[3].payload,
                    dict(target=config_platform.TARGET,core=0,security='secure',boot_context=config_platform.BOOT_CONTEXT),contradictory_effects=())
            result=config_abstract.decoder_interface(*args,**kwargs)
            with self.subTest(semantic=semantic,cursor=(lower,upper)):
                self.assertEqual({o['kind']:(o['cursor'].lower,o['cursor'].upper) for o in result['outcomes']},expected)
                self.assertTrue(all(o['state'].cursor==o['cursor'] for o in result['outcomes']))
        for count,kind,after in ((0,'none',0),(0xffffffff,'error4',0xfffffffe)):
            args=self.sequence('seq_u16',bytes(8))
            args[3]=replace(args[3],count=config_abstract.Span(count,count),decoder=replace(args[3].decoder,cursor=config_abstract.Span(8,8)))
            result=config_abstract.sequence_interface(*args)
            self.assertEqual([(o['kind'],o['count'],o['cursor']) for o in result['outcomes']],
                             [(kind,config_abstract.Span(after,after),config_abstract.Span(8,8))])
            self.assertTrue(all(o['state'].count==o['count'] and o['state'].decoder.cursor==o['cursor'] for o in result['outcomes']))


class ConstructionInterfaces(unittest.TestCase):
    def fixture(self,semantic='vec_extend14',shift=0):
        from config_regions import Storage
        objects,item=LoggerObjects.fixture(self,'sizez',shift,0x19000 if shift else 0,fixture_file='config-construction.json')
        objects.elf.mapping_symbols=[(n,a+shift,i) for n,a,i in item['mapping_symbols']]
        records={}
        for row in item['records']:
            op=row['text'].split(None,1);at=row['address']+shift;operand=op[1] if len(op)>1 else ''
            if config_stack.transfer_family(op[0].removesuffix('.w')) in ('b','bl','cbz','cbnz'):
                operand=re.sub(r'0x([0-9a-f]+)',lambda m:hex(int(m[1],16)+shift),operand)
            records[at]=Instruction(at,bytes.fromhex(row['bytes']),op[0],operand)
        instance=next(v for v in item['instances'] if v['role']==semantic)
        frame=Storage('stack:construction','active:'+semantic,-instance['frame'],instance['frame'],8,'refill',True) if instance['frame'] else None
        return objects,records,item,instance,frame

    def arguments(self,capacity=14,length=(0,0),source_length=(0,8)):
        from config_abstract import Span
        from config_abstract_compare import InitializedPrefix
        from config_construction import VecInterface
        from config_regions import Storage,borrow
        owner=Storage('stack:construction','Vec',128,((4+capacity+3)//4)*4,4,'refill',True)
        state=VecInterface(borrow(owner,0,owner.extent,owner.phase,writable=True),capacity,Span(*length))
        source=InitializedPrefix(borrow(Storage(owner.space,'input',256,32,1,owner.phase,False),0,32,owner.phase),Span(*source_length))
        return state,source

    def test_all_construction_bodies_and_parent_outlines_have_complete_coverage(self):
        import config_helpers
        for shift in (0,0x2800):
            objects,records,item,_,_=self.fixture(shift=shift)
            for instance in item['instances']:
                outline=instance['typed_role'] if instance['typed_role'].startswith('@outline:') else None
                proof=config_helpers.helper_coverage(objects,instance['name'],records,
                    (instance['address']+shift,'construction','supplied parent'),outline)
                self.assertEqual((proof['extent'],proof['frame']),(instance['extent'],instance['frame']))
                if outline:self.assertEqual(proof['requires_parent_outline_abi'],outline)

    def test_unknown_append_unions_keep_prefix_count_capacity_and_no_write_error(self):
        import config_construction,config_helpers
        from config_abstract import Span
        for capacity in (14,20,24):
            args=self.fixture('vec_extend'+str(capacity));objects,records,item,instance,frame=args
            proof=config_helpers.helper_coverage(objects,instance['name'],records,(instance['address'],'extend','actual parent owed'))
            state,source=self.arguments(capacity,(2,4),(1,capacity))
            result=config_construction.extend_interface(objects,records,proof,state,source,frame,())
            self.assertEqual([(o['kind'],o['length']) for o in result['outcomes']],[('success',Span(3,capacity)),('capacity_error',Span(2,4))])
            self.assertEqual(result['outcomes'][1]['possible_writes'],())
            self.assertEqual(result['conservative_activation_frames'],(16,24))
            self.assertTrue(all(o['state'].length==o['length'] for o in result['outcomes']))
            self.assertTrue(result['requires_actual_stage_receiver_transport_and_complete_parent_graph'])

    def test_capacity_error_and_zero_append_do_not_initialize_slack(self):
        import config_construction,config_helpers
        from config_abstract import Span
        objects,records,item,instance,frame=self.fixture()
        proof=config_helpers.helper_coverage(objects,instance['name'],records,(instance['address'],'extend','source owner'))
        for length,count,expected in (((14,14),(1,1),['capacity_error']),((3,3),(0,0),['success','capacity_error'])):
            state,source=self.arguments(length=length,source_length=count)
            result=config_construction.extend_interface(objects,records,proof,state,source,frame,())
            self.assertEqual([o['kind'] for o in result['outcomes']],expected)
            self.assertTrue(all(not o['possible_writes'] for o in result['outcomes']))
            self.assertTrue(all(o['length']==Span(*length) for o in result['outcomes']))

    def test_header_full_owner_source_and_deeper_save_aliases_reject(self):
        import config_construction,config_helpers
        from dataclasses import replace
        from config_regions import Storage,borrow
        objects,records,item,instance,frame=self.fixture()
        proof=config_helpers.helper_coverage(objects,instance['name'],records,(instance['address'],'extend','alias negatives'))
        for start in (-4,-24,-40):
            state,source=self.arguments();owner=replace(state.view.storage,start=start)
            bad=replace(state,view=borrow(owner,0,owner.extent,owner.phase,writable=True))
            with self.subTest(start=start),self.assertRaises(AssertionError):config_construction.extend_interface(objects,records,proof,bad,source,frame,())
        state,source=self.arguments();source=replace(source,view=borrow(state.view.storage,4,16,'refill'))
        with self.assertRaises(AssertionError):config_construction.extend_interface(objects,records,proof,state,source,frame,())
        state,source=self.arguments();protected=(Storage(frame.space,'live carrier',132,4,4,frame.phase,False),)
        with self.assertRaises(AssertionError):config_construction.extend_interface(objects,records,proof,state,source,frame,protected)

    def test_try_from_keeps_tag_only_error_and_separate_opaque_success_transport(self):
        import config_construction,config_helpers
        from config_regions import Storage,borrow
        objects,records,item,instance,frame=self.fixture('string_try14');_,source=self.arguments(source_length=(0,24))
        proof=config_helpers.helper_coverage(objects,instance['name'],records,(instance['address'],'try_from','typed str caller owed'))
        output=borrow(Storage(frame.space,'Result<String14>',128,24,4,frame.phase,True),0,24,frame.phase,writable=True)
        result=config_construction.string_interface(objects,records,proof,source,output,frame,())
        self.assertEqual([o['kind'] for o in result['outcomes']],['success','capacity_error'])
        self.assertEqual(result['conservative_activation_frames'],(48,16,24))
        self.assertEqual(result['outcomes'][0]['opaque_copy'],(4,20))
        self.assertEqual(result['outcomes'][0]['semantic_string_header'],(4,4))
        self.assertEqual(result['outcomes'][0]['initialized_prefix'].length.upper,14)
        self.assertEqual(result['outcomes'][1]['possible_writes'],((output,0,4),))
        self.assertNotIn('initialized_prefix',result['outcomes'][1])
        self.assertTrue(result['requires_source_valid_str'])

    def test_try_result_and_source_must_survive_local_scratch_and_every_child_prologue(self):
        import config_construction,config_helpers
        from dataclasses import replace
        from config_regions import Storage,borrow
        objects,records,item,instance,frame=self.fixture('string_try14');_,source=self.arguments()
        proof=config_helpers.helper_coverage(objects,instance['name'],records,(instance['address'],'try_from','protected cells'))
        output=borrow(Storage(frame.space,'Result',128,24,4,frame.phase,True),0,24,frame.phase,writable=True)
        for start in (-48,-64,-88):
            bad=borrow(replace(output.storage,start=start),0,24,frame.phase,writable=True)
            with self.subTest(start=start),self.assertRaises(AssertionError):config_construction.string_interface(objects,records,proof,source,bad,frame,())
        with self.assertRaises(AssertionError):config_construction.string_interface(objects,records,proof,
            replace(source,view=borrow(output.storage,4,20,frame.phase)),output,frame,())

    def test_changed_capacity_outgoing_cell_copy_count_and_restore_register_reject(self):
        import config_helpers
        for semantic,offset,operand in (('vec_extend14',8,'r2, #0xf'),('construction_prepare_slice',2,'r2, [sp, #0x4]'),
            ('vec_extend_inner',10,'r6, [r7, #0xc]'),('string_try14',34,'r0, r8, #0x8'),
            ('construction_restore_r11_low20',0,'r8, [sp], #4')):
            objects,records,item,instance,frame=self.fixture(semantic);at=instance['address']+offset;row=records[at]
            records[at]=Instruction(at,row.raw,row.mnemonic,operand)
            outline=instance['typed_role'] if instance['typed_role'].startswith('@outline:') else None
            with self.subTest(semantic=semantic),self.assertRaises(AssertionError):config_helpers.helper_body(objects,instance['name'],records,outline)

    def test_construction_fact_constructors_reject_unbound_capacity_or_length(self):
        from config_abstract import Span
        from dataclasses import replace
        state,source=self.arguments()
        for change in (dict(capacity=15),dict(length=Span(0,15)),dict(view=replace(state.view,extent=19))):
            with self.assertRaises(AssertionError):replace(state,**change)

    def test_wrapper_capacity_sum_overflow_and_stale_inventory_reject(self):
        import config_construction,config_helpers
        from config_abstract import Span
        from config_abstract_compare import InitializedPrefix
        from config_regions import Storage,borrow
        objects,records,item,instance,frame=self.fixture()
        proof=config_helpers.helper_coverage(objects,instance['name'],records,(instance['address'],'extend','bounded sum'))
        state,source=self.arguments(20)
        with self.assertRaises(AssertionError):config_construction.extend_interface(objects,records,proof,state,source,frame,())
        # A huge source prefix is structurally bounded but makes the machine
        # addition wrap; reject before a capacity comparison can authorize it.
        state,_=self.arguments(length=(1,1))
        huge=Storage(frame.space,'huge-source',-0x100000000,0xffffffff,1,frame.phase,False)
        source=InitializedPrefix(borrow(huge,0,huge.extent,frame.phase),Span(0xffffffff,0xffffffff))
        with self.assertRaisesRegex(AssertionError,'sum may wrap'):
            config_construction.extend_interface(objects,records,proof,state,source,frame,())
        state,source=self.arguments();stale=dict(proof,ordinary_edges=())
        with self.assertRaises(AssertionError):config_construction.extend_interface(objects,records,stale,state,source,frame,())

    def test_anonymous_constructor_cannot_borrow_ordinary_identity(self):
        import config_helpers
        objects,records,item,instance,frame=self.fixture('string_try14')
        name='OUTLINED_FUNCTION_999999';old=instance['name']
        objects.elf.functions[name]=objects.elf.functions.pop(old)
        objects.elf.symbol_entries=[(name if v[0]==old else v[0],*v[1:]) for v in objects.elf.symbol_entries]
        for override in (None,instance['typed_role'],'@outline:construction_unknown'):
            with self.subTest(override=override),self.assertRaises(AssertionError):config_helpers.helper_body(objects,name,records,override)


class DefaultPreparationInterfaces(unittest.TestCase):
    def test_payload_borrow_keeps_protected_enclosing_suffix_and_rejects_crossing(self):
        from config_default import default_preparation
        from config_regions import Storage,borrow,require_writes
        for offset in (14576,14600,14601,14575):
            objects,rows,name,frame=self.fixture();B=0x2000031c
            guard=Storage('address','retained enclosing suffix',B+offset,14604-offset,1,frame.phase,False)
            if offset==14575:
                with self.assertRaises(AssertionError):default_preparation(objects,name,rows,frame,(guard,))
            else:
                try:proof=default_preparation(objects,name,rows,frame,(guard,))
                except AssertionError:proof=None
                with self.subTest(offset=offset):self.assertIsNotNone(proof,'a contained readonly enclosing suffix must not reject disjoint Config payload')
                if proof is not None:
                    self.assertEqual(proof['config'].storage.extent,14604)
                    self.assertIn(guard,proof['protected'])
                    wide=borrow(proof['config'].storage,0,14604,frame.phase,writable=True)
                    with self.assertRaises(AssertionError):require_writes((wide,),((wide,offset-1,2),),(guard,))

    def fixture(self,code_shift=0,data_shift=0):
        from dataclasses import replace
        objects,records,root,visitor,frame,child=OrdinaryHeaderConstructors.fixture(self,code_shift,data_shift)
        donor,item=LoggerObjects.fixture(self,'sizez',code_shift,data_shift,fixture_file='config-default-preparation.json')
        data=bytearray(objects.elf.data)
        for index,original,encoded in item['segments']:
            source=donor.elf.sections[index];dest=objects.elf.sections[index]
            shift=0 if dest[2]&1 else code_shift if dest[2]&4 else data_shift
            at=original+shift;extent=len(bytes.fromhex(encoded));a=source[4]+at-source[3];b=dest[4]+at-dest[3]
            data[b:b+extent]=donor.elf.data[a:a+extent]
        def position(address):
            section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=address<address+4<=s[3]+s[5])
            return section[4]+address-section[3]
        for original in item['relocations']:
            owner=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=original+(code_shift if s[2]&4 else data_shift)<s[3]+s[5])
            at=original+(code_shift if owner[2]&4 else data_shift);offset=position(at);value=struct.unpack_from('<I',data,offset)[0]
            target=next(s for s in objects.elf.sections if s[2]&2 and
                s[3]-(0 if s[2]&1 else code_shift if s[2]&4 else data_shift)<=value&~1<
                s[3]-(0 if s[2]&1 else code_shift if s[2]&4 else data_shift)+s[5])
            struct.pack_into('<I',data,offset,value+(0 if target[2]&1 else code_shift if target[2]&4 else data_shift))
        objects.elf.data=bytes(data);objects.elf.functions.update(donor.elf.functions)
        objects.elf.symbol_entries.extend(donor.elf.symbol_entries);objects.sizes.update(donor.sizes)
        for row in item['records']:
            op=row['text'].split(None,1);at=row['address']+code_shift;operand=op[1] if len(op)>1 else ''
            if config_stack.transfer_family(op[0].removesuffix('.w')) in ('b','bl','cbz','cbnz'):
                operand=re.sub(r'0x([0-9a-f]+)',lambda m:hex(int(m[1],16)+code_shift),operand)
            records[at]=Instruction(at,bytes.fromhex(row['bytes']),op[0],operand)
        return objects,records,root,replace(frame,phase='default_refill')

    def test_view_child_and_widget_handoff_keep_distinct_fault_result_and_retired_epochs(self):
        import config_default
        for code,data in ((0,0),(0x2800,0x19000)):
            objects,records,root,frame=self.fixture(code,data)
            result=config_default.default_preparation(objects,root,records,frame,())
            self.assertEqual(result['config'].extent,14576)
            self.assertEqual(result['view']['initialized_name'].length.lower,8)
            self.assertEqual((result['view']['count'],result['view']['element_offset'],result['view']['name_capacity']),(1,248,14))
            self.assertEqual(result['widget_handoff']['channels'],(0,7,6,2,1,3,4,5))
            self.assertEqual((result['widget_handoff']['signed_offset'],result['widget_handoff']['count']),(-1760,0))
            self.assertEqual((result['fault'].start,result['fault'].extent),(frame.start+8,16))
            self.assertEqual(result['capacity_failure']['root']['location_role'],'view_capacity')
            self.assertEqual(result['capacity_failure']['error_payload_bytes'],0)
            self.assertEqual(result['capacity_failure']['argument_address'],frame.start+351)
            self.assertEqual(result['conservative_activation_frames'],(456,48,16,24))
            access=result['target_access_requirements'][0]
            self.assertEqual((access['address'],access['width'],access['unaligned']),
                (result['config'].start+14570,4,True))
            self.assertEqual(access['status'],'unresolved_object_store_policy')
            self.assertTrue(result['requires_incoming_typed_fault_exclusive_slot_and_all_remaining_phases'])
            self.assertNotIn('validated_config',result)

    def test_result_pointer_tag_view_offset_and_channel_pair_mutations_reject(self):
        import config_default
        for site,operand in ((0x1001ff10,'r9, sp, #0x74'),(0x1001ff14,'r2, #0x9'),
            (0x1001ff2e,'r0, #0x2'),(0x1001ff76,'r0, r8, #0xfc'),(0x1001ffb2,'r0, #0xc')):
            objects,records,root,frame=self.fixture();row=records[site]
            records[site]=Instruction(site,row.raw,row.mnemonic,operand)
            with self.subTest(site=site),self.assertRaises(AssertionError):config_default.default_preparation(objects,root,records,frame,())

    def test_view_literal_channel_array_and_static_identity_are_actual_data(self):
        import config_default,config_entry
        for address in (0x1002bfc8,0x100200e8,0x100200ec,0x100200f0):
            objects,records,root,frame=self.fixture();section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=address<s[3]+s[5])
            data=bytearray(objects.elf.data);data[section[4]+address-section[3]]^=1;objects.elf.data=bytes(data)
            with self.subTest(address=address),self.assertRaises(AssertionError):config_default.default_preparation(objects,root,records,frame,())
        objects,records,root,frame=self.fixture();entry=next(v for v in objects.elf.symbol_entries if config_entry.static_role(v[0])==config_entry.CONFIG)
        objects.elf.symbol_entries[objects.elf.symbol_entries.index(entry)]=('different_static',*entry[1:])
        with self.assertRaises(AssertionError):config_default.default_preparation(objects,root,records,frame,())

    def test_interior_ingress_and_changed_capacity_diagnostic_reject(self):
        import config_default
        objects,records,root,frame=self.fixture();site=0x10020e76;row=records[site]
        records[site]=Instruction(site,row.raw,'b','0x1001ff10')
        with self.assertRaises(AssertionError):config_default.default_preparation(objects,root,records,frame,())
        objects,records,root,frame=self.fixture();site=0x1002099a;row=records[site]
        records[site]=Instruction(site,row.raw,row.mnemonic,'r2, r7, #0x60')
        with self.assertRaises(AssertionError):config_default.default_preparation(objects,root,records,frame,())

    def test_live_carrier_or_saved_cell_cannot_alias_result_and_local_copies(self):
        import config_default
        from config_regions import Storage
        for start in (112,236,352):
            objects,records,root,frame=self.fixture();guard=Storage(frame.space,'live parent carrier',start,4,4,frame.phase,False)
            with self.subTest(start=start),self.assertRaises(AssertionError):config_default.default_preparation(objects,root,records,frame,(guard,))

    def test_unrelated_float_bits_stay_unknown_but_parent_ingress_cannot_bypass_the_group(self):
        import config_default
        objects,records,root,frame=self.fixture();address=0x100200c4
        section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=address<s[3]+s[5])
        data=bytearray(objects.elf.data);data[section[4]+address-section[3]]^=1;objects.elf.data=bytes(data)
        result=config_default.default_preparation(objects,root,records,frame,())
        self.assertIn(address,result['readonly_float_pools'])
        for operation,operand,raw in (('mov','pc, r4',b'\0\0\0\0'),('bxeq','r4',b'\0\0\0\0'),
            ('bxns','r4',b'\0\0\0\0'),('svcne','#0x1',b'\0\0\0\0'),
            ('ldmdb','r4!, {r0, pc}',b'\0\0\0\0'),('tbb','[pc, r0]',struct.pack('<HH',0xe8df,0xf000))):
            objects,records,root,frame=self.fixture();at=(objects.elf.functions[root][0]&~1)+18
            records[at]=Instruction(at,raw,operation,operand)
            with self.subTest(operation=operation),self.assertRaises(AssertionError):config_default.default_preparation(objects,root,records,frame,())


class RefillChildInterfaces(unittest.TestCase):
    def fixture(self,mode,code_shift=0,data_shift=0):
        objects,item=LoggerObjects.fixture(self,mode,code_shift,data_shift,fixture_file='config-refill-children.json')
        objects.elf.mapping_symbols=[(n,a+code_shift,i) for n,a,i in item['mapping_symbols']]
        records={};data=bytearray(objects.elf.data)
        for row in item['records']:
            op=row['text'].split(None,1);at=row['address']+code_shift;operand=op[1] if len(op)>1 else ''
            if config_stack.transfer_family(op[0].removesuffix('.w')) in ('b','bl','cbz','cbnz'):
                operand=re.sub(r'0x([0-9a-f]+)',lambda m:hex(int(m[1],16)+code_shift),operand)
            records[at]=Instruction(at,bytes.fromhex(row['bytes']),op[0],operand)
        def place(at,raw):
            section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=at<at+len(raw)<=s[3]+s[5])
            index=section[4]+at-section[3];data[index:index+len(raw)]=raw
        for pool in item['pool_objects']:
            target=objects.elf.sections[pool['target_section']]
            place(pool['pool']+code_shift,struct.pack('<I',pool['value']+(code_shift if target[2]&4 else data_shift)))
        for pair in item['mov_objects']:
            target=objects.elf.sections[pair['target_section']];value=pair['value']+(code_shift if target[2]&4 else data_shift)
            for site,half in ((pair['low'],value&65535),(pair['high'],value>>16)):
                at=site+code_shift;row=records[at];first,second=struct.unpack('<HH',row.raw)
                raw=struct.pack('<HH',(first&~0x040f)|((half>>12)&15)|((half>>1)&0x400),
                    (second&~0x70ff)|((half&0x700)<<4)|(half&255))
                records[at]=Instruction(at,raw,row.mnemonic,row.operands.split(',')[0]+', #'+hex(half));place(at,raw)
        objects.elf.data=bytes(data)
        return objects,records,item

    def args(self,mode,shape):
        import config_refill_children
        from config_regions import Storage,borrow
        objects,records,item=self.fixture(mode);instance=next(v for v in item['instances'] if v['shape']==shape)
        proof=config_refill_children.child_coverage(objects,instance['name'],records,(instance['address'],'refill child','caller owed'))
        size=proof['frame'];frame=Storage('stack:refill-child',shape,-size,size,4 if size==52 else 8,'refill',True)
        extent=36 if shape.startswith('catalog') else 64
        output=borrow(Storage(frame.space,'output',128,extent,4,frame.phase,True),0,extent,frame.phase,writable=True)
        return objects,records,proof,frame,output

    def test_six_complete_families_preserve_tables_lr_pools_padding_and_outline_context(self):
        import config_refill_children
        for mode in ('sizes','sizez'):
            for shift,data in ((0,0),(0x2800,0x19000)):
                objects,records,item=self.fixture(mode,shift,data)
                for instance in item['instances']:
                    role=instance['role'];outline=role if role.startswith('@outline:') else None
                    proof=config_refill_children.child_coverage(objects,instance['name'],records,
                        (instance['address']+shift,'refill child','source origin still owed'),outline)
                    self.assertEqual(proof['shape'],instance['shape'])
                    if outline:self.assertEqual(proof['requires_parent_frame_and_abi'],instance['shape'])

    def test_definition_union_keeps_all_source_variants_paired_strings_and_inactive_payloads(self):
        import config_refill_children
        from config_regions import Argument
        for mode,shape in (('sizes','catalog_definition_s'),('sizez','catalog_definition_z')):
            objects,records,proof,frame,output=self.args(mode,shape)
            result=config_refill_children.definition_interface(objects,records,proof,Argument('DefinitionId',None,0,8,frozenset(range(16))),output,frame,())
            self.assertEqual(len(result['outcomes']),16)
            self.assertEqual(result['possible_writes'],((output,0,35),))
            disabled=result['outcomes'][0];self.assertEqual(disabled['initialized_ranges'],((0,25),(30,5)))
            empty=disabled['strings'][2];self.assertEqual((empty['length'],empty['pointers']),(0,(1,)))
            self.assertTrue(all(p['length']==len(bytes.fromhex(p['utf8_hex'])) for o in result['outcomes'] for p in o['strings']))
            self.assertTrue(result['requires_actual_typed_origin_receiver_transport_and_parent_graph'])

    def test_enum_kind_domain_and_full_output_save_aliases_reject(self):
        import config_refill_children
        from config_regions import Argument,borrow
        from dataclasses import replace
        objects,records,proof,frame,output=self.args('sizez','catalog_definition_z')
        for kind,bits,domain in (('u8',8,(0,)),('DefinitionId',8,(16,)),('DefinitionId',16,(0,))):
            with self.subTest(kind=kind,domain=domain),self.assertRaises(AssertionError):
                config_refill_children.definition_interface(objects,records,proof,Argument(kind,None,0,bits,frozenset(domain)),output,frame,())
        arg=Argument('DefinitionId',None,0,8,frozenset((0,)))
        bad=borrow(replace(output.storage,start=-52),0,36,frame.phase,writable=True)
        with self.assertRaises(AssertionError):config_refill_children.definition_interface(objects,records,proof,arg,bad,frame,())
        with self.assertRaises(AssertionError):config_refill_children.definition_interface(objects,records,proof,arg,output,replace(frame,start=-56),())

    def test_period_pairs_source_channel_with_typed_readonly_table(self):
        import config_refill_children
        from config_regions import Argument
        objects,records,proof,frame,output=self.args('sizez','minimum_period_z')
        result=config_refill_children.period_interface(objects,records,proof,Argument('Channel',None,0,8,frozenset(range(8))),frame,())
        self.assertEqual([(o['channel'],o['period_ms']) for o in result['outcomes']],list(enumerate((500,1500,1000,1000,1000,1000,500,500))))
        self.assertEqual(result['possible_writes'],())
        with self.assertRaises(AssertionError):config_refill_children.period_interface(objects,records,proof,Argument('Channel',None,0,8,frozenset((8,))),frame,())

    def test_advanced_output_bands_and_readout_alternatives_keep_full_copy_chain_and_store_obligation(self):
        import config_refill_children
        from config_regions import Argument
        objects,records,proof,frame,output=self.args('sizez','advanced_for_channel_z')
        result=config_refill_children.advanced_interface(objects,records,proof,Argument('Channel',None,0,8,frozenset(range(8))),
            Argument('bool',None,0,8,frozenset((0,1))),output,frame,())
        self.assertEqual(result['conservative_activation_frames'],(72,8,72))
        self.assertEqual(result['physical_activation_paths'],((72,8),(72,72)))
        self.assertTrue(result['restored_tail_storage'])
        self.assertEqual(result['initialized_ranges'],((0,61),))
        self.assertEqual(result['local_band_ranges'],((0,16),(16,16),(32,16)))
        self.assertEqual({(o['channel'],o['afr'],o['readout']) for o in result['outcomes'] if o['channel']==7},{(7,0,7),(7,1,9)})
        self.assertEqual(result['target_access_requirements'][0]['address'],output.start+57)
        self.assertEqual(result['target_access_requirements'][0]['status'],'unresolved_object_store_policy')

    def test_advanced_bool_domain_and_child_copy_or_parent_spill_context_cannot_change(self):
        import config_refill_children
        from config_regions import Argument
        objects,records,proof,frame,output=self.args('sizez','advanced_for_channel_z')
        with self.assertRaises(AssertionError):config_refill_children.advanced_interface(objects,records,proof,
            Argument('Channel',None,0,8,frozenset((7,))),Argument('bool',None,0,8,frozenset((2,))),output,frame,())
        objects,records,item=self.fixture('sizez');instance=next(v for v in item['instances'] if v['shape']=='refill_store_parent_word_and_advanced_false')
        at=instance['address']+2;row=records[at];records[at]=Instruction(at,row.raw,row.mnemonic,'r0, [sp, #0x28]')
        with self.assertRaises(AssertionError):config_refill_children.child_body(objects,instance['name'],records,instance['role'])
        objects,records,proof,frame,output=self.args('sizez','advanced_for_channel_z');name='__aeabi_memcpy'
        at=objects.elf.functions[name][0]&~1;row=records[at];records[at]=Instruction(at,row.raw,'push','{r4, r7, lr}')
        with self.assertRaises(AssertionError):config_refill_children.advanced_interface(objects,records,proof,
            Argument('Channel',None,0,8,frozenset((0,))),Argument('bool',None,0,8,frozenset((0,))),output,frame,())

    def test_advanced_output_and_live_parent_guard_survive_deeper_builtin_frames(self):
        import config_refill_children
        from dataclasses import replace
        from config_regions import Argument,Storage,borrow
        channel=Argument('Channel',None,0,8,frozenset((0,)));afr=Argument('bool',None,0,8,frozenset((0,)))
        for fault in ('output','guard'):
            objects,records,proof,frame,output=self.args('sizez','advanced_for_channel_z');protected=()
            if fault=='output':output=borrow(replace(output.storage,start=frame.start-64),0,64,frame.phase,writable=True)
            else:protected=(Storage(frame.space,'retained caller object',frame.start-48,4,4,frame.phase,False),)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_refill_children.advanced_interface(objects,records,proof,channel,afr,output,frame,protected)

    def test_catalog_table_source_bytes_and_lr_destination_remain_typed(self):
        import config_refill_children
        from config_refill_contracts import REFILL_GRAMMARS
        for fault in ('table','source','lr'):
            objects,records,item=self.fixture('sizez');instance=next(v for v in item['instances'] if v['shape']=='catalog_definition_z')
            proof=config_refill_children.child_body(objects,instance['name'],records)
            if fault=='lr':
                row=next(r for a,r in records.items() if instance['address']<=a<instance['address']+772 and r.operands.startswith('lr, [pc,'))
                records[row.address]=Instruction(row.address,row.raw,row.mnemonic,row.operands.replace('lr,','r4,',1))
            else:
                at=instance['address']+48 if fault=='table' else proof['objects'][0]
                section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=at<s[3]+s[5]);data=bytearray(objects.elf.data)
                data[section[4]+at-section[3]]^=1;objects.elf.data=bytes(data)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):config_refill_children.child_body(objects,instance['name'],records)
        from unittest.mock import patch
        objects,records,item=self.fixture('sizez');instance=next(v for v in item['instances'] if v['shape']=='catalog_definition_z')
        wrong=dict(config_refill_children.CATALOG_DISPATCH['catalog_definition_z']);wrong['target_offsets']=list(wrong['target_offsets'])
        wrong['target_offsets'][0]=wrong['target_offsets'][1]
        with patch.dict(config_refill_children.CATALOG_DISPATCH,{'catalog_definition_z':wrong}),self.assertRaises(AssertionError):
            config_refill_children.child_coverage(objects,instance['name'],records,(instance['address'],'catalog','incorrect target vector'))

    def test_readonly_source_ownership_rejects_partial_alias_and_overflow_and_preserves_adjacency(self):
        import config_refill_children
        from config_refill_contracts import REFILL_GRAMMARS
        for fault in ('partial','overflow','adjacent'):
            objects,records,item=self.fixture('sizez');instance=next(v for v in item['instances'] if v['shape']=='catalog_definition_z')
            proof=config_refill_children.child_body(objects,instance['name'],records);address=proof['objects'][0]
            descriptor=REFILL_GRAMMARS[proof['shape']]['objects'][0]
            old=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=address<address+descriptor['extent']<=s[3]+s[5])
            section=list(old)
            if fault=='overflow':section[5]=0xffffffff;objects.elf.sections[objects.elf.sections.index(old)]=tuple(section)
            else:
                section[3]=address+(descriptor['extent'] if fault=='adjacent' else 2);section[5]=1
                objects.elf.sections.append(tuple(section))
            if fault=='adjacent':config_refill_children._object(objects,address,descriptor)
            else:
                with self.subTest(fault=fault),self.assertRaises(AssertionError):config_refill_children._object(objects,address,descriptor)


class AbstractCanonicalInterfaces(unittest.TestCase):
    def compare(self):
        import config_abstract,config_abstract_compare
        args=list(OrdinaryCompareEffects.fixture(self,b'abcdef',b'abc'))
        state=args[3];source=args[4]
        args[3]=config_abstract_compare.CompareInterface(state.header,state.payload,config_abstract.Span(0,state.payload.extent))
        args[4]=config_abstract_compare.InitializedPrefix(source.view,config_abstract.Span(0,3))
        return args

    def serialize(self,semantic):
        import config_abstract,config_abstract_compare
        source=OrdinarySerializeEffects.fixture(self,semantic,b'abcdef','u8',8,0)
        args=list(source[:4]+source[5:])
        state=args[3];args[3]=config_abstract_compare.CompareInterface(state.header,state.payload,config_abstract.Span(0,state.payload.extent))
        return args

    def test_unknown_compare_bytes_retain_match_mismatch_and_short_guarded_exits(self):
        import config_abstract_compare
        args=self.compare();result=config_abstract_compare.compare_interface(*args)
        self.assertEqual({o['kind'] for o in result['outcomes']},{'success','error2'})
        self.assertEqual(result['conservative_activation_frames'],(24,32))
        self.assertTrue(result['requires_actual_stage_receiver_transport_and_complete_parent_graph'])
        self.assertFalse(any('data' in o for o in result['outcomes']))

    def test_scalar_families_keep_all_prefix_lengths_without_executing_values(self):
        import config_abstract,config_abstract_compare
        for semantic,kind,bits,maximum in (('serialize_u8','u8',8,1),('serialize_u16','u16',16,3),
            ('varint_usize','usize',32,5),('varint_u32_small','u32',7,1),('serialize_f32','f32_bits',32,4)):
            args=self.serialize(semantic);value=config_abstract_compare.ScalarInterface(kind,config_abstract.Span(0,(1<<bits)-1))
            result=config_abstract_compare.serialize_interface(*args[:4],value,*args[4:])
            self.assertEqual(result['encoded_length'].upper,maximum)
            self.assertEqual({o['kind'] for o in result['outcomes']},{'success','error2'})
            self.assertNotIn('encoded',result)

    def test_specialized_u32_and_wrong_width_cannot_borrow_an_abstract_type_name(self):
        import config_abstract,config_abstract_compare
        args=self.serialize('varint_u32_small')
        for kind,upper in (('u32',128),('u16',127)):
            value=config_abstract_compare.ScalarInterface(kind,config_abstract.Span(0,upper))
            with self.subTest(kind=kind),self.assertRaises(AssertionError):config_abstract_compare.serialize_interface(*args[:4],value,*args[4:])

    def test_string_prefix_and_body_failures_keep_paired_partial_progress(self):
        import config_abstract,config_abstract_compare
        from config_regions import Storage,borrow
        args=self.serialize('serialize_str');frame=args[4]
        view=borrow(Storage(frame.space,'typed-string',176,20,1,frame.phase,True),0,20,frame.phase)
        source=config_abstract_compare.InitializedPrefix(view,config_abstract.Span(0,20))
        result=config_abstract_compare.serialize_interface(*args[:4],source,*args[4:])
        self.assertEqual({o['phase'] for o in result['outcomes']},{'prefix_failed','body_failed','complete'})
        body=next(o for o in result['outcomes'] if o['phase']=='body_failed')
        self.assertGreaterEqual(body['state'].cursor.lower,1)
        self.assertEqual(result['conservative_activation_frames'],(24,24,24,32))

    def test_source_and_header_must_survive_helper_and_memcmp_prologues(self):
        import config_abstract,config_abstract_compare
        from config_regions import Storage,borrow
        for fault in ('C_end','source'):
            args=self.compare();state,source,frame=args[3:6]
            if fault=='C_end':
                owner=Storage(frame.space,'bad-C',frame.start-4,8,4,frame.phase,True)
                args[3]=config_abstract_compare.CompareInterface(borrow(owner,0,8,frame.phase,writable=True),state.payload,state.cursor)
            else:
                owner=Storage(frame.space,'bad-source',frame.start-32,3,1,frame.phase,True)
                args[4]=config_abstract_compare.InitializedPrefix(borrow(owner,0,3,frame.phase),config_abstract.Span(0,3))
            with self.subTest(fault=fault),self.assertRaises(AssertionError):config_abstract_compare.compare_interface(*args)

    def test_declared_policy_survives_nested_prefix_and_body_and_cannot_become_stale(self):
        import config_abstract,config_abstract_compare,config_platform
        from dataclasses import replace
        from config_regions import Storage,borrow
        args=self.serialize('serialize_str');state,frame=args[3:5]
        policy=config_platform.reviewed_eav_policy(args[0],state.payload,dict(target=config_platform.TARGET,
            core=0,security='secure',boot_context=config_platform.BOOT_CONTEXT),contradictory_effects=())
        args[3]=replace(state,platform_policy=policy)
        source=config_abstract_compare.InitializedPrefix(borrow(Storage(frame.space,'string',176,20,1,frame.phase,True),
            0,20,frame.phase),config_abstract.Span(0,20))
        result=config_abstract_compare.serialize_interface(*args[:4],source,*args[4:])
        self.assertEqual(result['platform_policy'],policy)
        self.assertTrue(all(o['state'].platform_policy==policy for o in result['outcomes']))
        self.assertTrue(all(child['platform_policy']==policy for child in result['children']))
        args[0].elf.data+=b'changed'
        with self.assertRaises(AssertionError):config_abstract_compare.serialize_interface(*args[:4],source,*args[4:])


class ConditionalStackPolicy(unittest.TestCase):
    def fixture(self):
        from config_regions import Storage,borrow
        base=0x10000000;raw=bytes(12)+bytes(range(32))
        elf=SimpleNamespace(data=raw,sections=[(0,1,2,base,0,len(raw),0,0,1,0)])
        objects=config_stack.DiagnosticObjects(elf,{})
        owner=Storage('address','embedded_eav',base,len(raw),1,'boot_configuration',False)
        view=borrow(owner,12,32,'boot_configuration')
        context=dict(target='thumbv8m.main-none-eabihf',core=0,security='secure',boot_context='fresh_normal_core0_image_def')
        return objects,view,context

    def test_reviewed_policy_is_external_and_accepts_all_four_cursor_residues(self):
        import config_platform
        objects,view,context=self.fixture()
        policy=config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=())
        self.assertEqual(policy.status,'assumed_external')
        self.assertFalse(policy.hardware_qualified)
        for residue in range(4):self.assertIs(policy.for_f32_load(view.start+residue),policy)
        for cursor in (view.start-1,view.start+view.extent-3):
            with self.assertRaises(AssertionError):policy.for_f32_load(cursor)

    def test_absent_unresolved_mismatched_or_contradictory_policy_rejects(self):
        import config_platform
        objects,view,context=self.fixture()
        expected=config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=())
        with self.assertRaises(AssertionError):config_platform.read_policy(None,expected)
        for key,value in (('status','unresolved'),('status','established'),('security','nonsecure'),
                          ('target','thumbv6m-none-eabi'),('input_end',view.start+view.extent-1),('hardware_qualified',True),
                          ('schema',True),('core',False),('core',1),('scope','unversioned'),('scope_sha256','0'*64),
                          ('assumptions',[]),('input_role','arbitrary_input'),('boot_context','debugger_pc_sp')):
            record=expected.to_record();record[key]=value
            with self.subTest(key=key,value=value),self.assertRaises(AssertionError):config_platform.read_policy(record,expected)
        for key,value in (('security','nonsecure'),('target','thumbv6m-none-eabi'),('boot_context','debugger_pc_sp')):
            current=dict(context);current[key]=value
            with self.subTest(context=key),self.assertRaises(AssertionError):
                config_platform.reviewed_eav_policy(objects,view,current,contradictory_effects=())
        with self.assertRaises(AssertionError):
            config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=('secure_ccr_unalign_trp_enabled',))
        for effects in (None,[],('unknown_register_effect',)):
            with self.subTest(effects=effects),self.assertRaises(AssertionError):
                config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=effects)
        with self.assertRaises(AssertionError):
            config_platform.reviewed_eav_policy(objects,view,None,contradictory_effects=())

    def test_policy_cannot_hide_writable_or_partial_input_ownership(self):
        import config_platform
        from config_regions import Storage,borrow
        for fault in ('section','owner','view','partial','identity'):
            objects,view,context=self.fixture()
            if fault=='section':
                section=list(objects.elf.sections[0]);section[2]|=1;objects.elf.sections[0]=tuple(section)
            else:
                owner=Storage('address','other_input' if fault=='identity' else 'embedded_eav',view.storage.start,
                              view.storage.extent,1,'boot_configuration',fault in ('owner','view'))
                view=borrow(owner,12,28 if fault=='partial' else 32,'boot_configuration',writable=fault=='view')
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=())

    def test_policy_propagates_nested_and_serialized_results_without_qualifying_hardware(self):
        import config_platform
        objects,view,context=self.fixture();policy=config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=())
        nested=config_platform.compose_policy(policy,(policy,config_platform.compose_policy(policy,(policy,))))
        result=config_platform.qualified_stack_result(dict(boot=4096,constructor=12000,runtime=13000,dashboard=18000),65536,nested)
        encoded=json.loads(json.dumps(result));read=config_platform.read_stack_result(encoded,policy,65536)
        self.assertEqual(read['status'],'CONDITIONAL_STACK_PASS')
        self.assertEqual(read['platform_policy'],'assumed_external')
        self.assertIs(read['hardware_qualified'],False)
        with self.assertRaises(AssertionError):config_platform.read_stack_result(encoded,policy,65536.0)
        for child in (None,{}):
            with self.assertRaises(AssertionError):config_platform.compose_policy(policy,(policy,child))

    def test_consumer_rejects_lost_qualifiers_or_changed_identity_and_keeps_budget_gate(self):
        import config_platform
        objects,view,context=self.fixture();policy=config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=())
        bounds=dict(boot=4096,constructor=12000,runtime=13000,dashboard=18000)
        result=config_platform.qualified_stack_result(bounds,65536,policy)
        for fault in ('status','platform_policy','hardware_qualified','policies','headroom'):
            record=json.loads(json.dumps(result))
            if fault=='status':record[fault]='PASS'
            elif fault=='hardware_qualified':record[fault]=True
            elif fault=='policies':record[fault]=[]
            elif fault=='headroom':record[fault]+=8
            else:record.pop(fault)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):config_platform.read_stack_result(record,policy,65536)
        altered=bytearray(objects.elf.data);altered[-1]^=1;objects.elf.data=bytes(altered)
        changed=config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=())
        with self.assertRaises(AssertionError):config_platform.read_stack_result(result,changed,65536)
        with self.assertRaises(AssertionError):
            config_platform.qualified_stack_result(dict(bounds,constructor=32768),65536,policy)


class OwnedPlatformDependencies(unittest.TestCase):
    def fixture(self,role='advanced_default_fields'):
        from config_regions import Storage,borrow
        import config_owned_access
        frame=Storage('stack:configuration.default','loader',0,456,8,'default_refill',True)
        saved=Storage(frame.space,'loader:saved',352,104,4,frame.phase,False)
        fault=Storage(frame.space,'original fault',8,16,4,frame.phase,False)
        if role=='advanced_default_fields':
            owner=borrow(frame,236,64,frame.phase,writable=True);register='r4';anchor=0x10010000;offset=84
        else:
            slot=Storage('address','private unpublished ConfigSlot',0x2000031c,14604,4,frame.phase,True)
            owner=borrow(slot,0,14576,frame.phase,writable=True);register='r8';anchor=0x10020000;offset=28
        return config_owned_access.OwnedAccess(role,anchor,anchor+offset,'default',owner,frame,register,(saved,fault))

    def test_owned_records_keep_symbolic_stack_and_concrete_private_slot_distinct(self):
        import config_owned_access
        for role in ('advanced_default_fields','configuration_versions'):
            value=self.fixture(role);record=value.to_record()
            self.assertEqual(config_owned_access.read_owned_access(json.loads(json.dumps(record))),value)
            self.assertEqual(record['operation'],'ordinary_single_STR')
            self.assertEqual((record['direction'],record['width']),('write',4))
            self.assertEqual(record['source_phase'],'default_refill')
            self.assertEqual(record['site']-record['anchor'],84 if role=='advanced_default_fields' else 28)
            self.assertEqual(record['owner']['storage']['space'],value.owner.storage.space)
            self.assertNotIn('runtime_sp',record)

    def test_owned_record_rejects_changed_role_operation_receiver_phase_and_guard(self):
        import config_owned_access
        from copy import deepcopy
        for role in ('advanced_default_fields','configuration_versions'):
            record=self.fixture(role).to_record()
            for fault in ('role','operation','width','direction','site','phase','receiver','short','readonly','guard','saves','unknown_owner','extra'):
                changed=deepcopy(record)
                if fault=='role':changed['role']='arbitrary_store'
                elif fault=='operation':changed['operation']='STM'
                elif fault=='width':changed['width']=8
                elif fault=='direction':changed['direction']='read'
                elif fault=='site':changed['site']+=2
                elif fault=='phase':changed['owner']['storage']['phase']='retired_label'
                elif fault=='receiver':changed['receiver_register']='r0'
                elif fault=='short':changed['owner']['extent']-=1
                elif fault=='readonly':changed['owner']['writable']=False
                elif fault=='guard':
                    changed['protected'][1].update(space=changed['owner']['storage']['space'],
                        start=changed['owner']['storage']['start']+changed['owner']['offset'],extent=4)
                elif fault=='saves':changed['protected'].pop(0)
                elif fault=='unknown_owner':changed['owner']['storage']['identity']='arbitrary raw output'
                else:changed['approved']=True
                with self.subTest(role=role,fault=fault),self.assertRaises(AssertionError):
                    config_owned_access.read_owned_access(changed)

    def test_policy_retains_exact_expected_dependency_tuple_and_rejects_loss(self):
        import config_platform
        from dataclasses import replace
        from copy import deepcopy
        objects,view,context=ConditionalStackPolicy.fixture(self)
        config=self.fixture('configuration_versions');advanced=self.fixture()
        from config_owned_access import OwnedAccess
        from config_regions import borrow
        dependencies=(config,advanced)+tuple(OwnedAccess('widget_colors',0x10022000,0x10022000+670,
            'widget'+str(i),borrow(config.owner.storage,444+220*i,220,config.frame.phase,writable=True),
            config.frame,'r4',config.protected,i) for i in range(8))
        policy=config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=(),owned_dependencies=dependencies)
        self.assertEqual(policy.schema,4)
        self.assertEqual(policy.owned_dependencies,dependencies)
        record=policy.to_record()
        self.assertEqual(config_platform.read_policy(json.loads(json.dumps(record)),policy),policy)
        self.assertEqual(config_platform.compose_policy(policy,(policy,policy)),policy)
        for fault in ('missing','empty','duplicate','extra','v1','phase','width','family'):
            changed=deepcopy(record)
            if fault=='missing':changed.pop('owned_dependencies')
            elif fault=='empty':changed['owned_dependencies']=[]
            elif fault=='duplicate':changed['owned_dependencies'].append(deepcopy(changed['owned_dependencies'][0]))
            elif fault=='extra':changed['owned_dependencies'].append(dict(changed['owned_dependencies'][0],activation='unrelated'))
            elif fault=='v1':changed['schema']=1
            elif fault=='phase':changed['owned_dependencies'][0]['owner']['storage']['phase']='retired'
            elif fault=='width':changed['owned_dependencies'][0]['width']=8
            else:changed['owned_family']='readonly_eav'
            with self.subTest(fault=fault),self.assertRaises(AssertionError):config_platform.read_policy(changed,policy)
        with self.assertRaises(AssertionError):config_platform.compose_policy(policy,(replace(policy,owned_dependencies=()),))
        with self.assertRaises(AssertionError):replace(policy,owned_dependencies=dependencies+(dependencies[0],))
        # An unresolved partial helper requirement is not a producer-owned fact.
        for supplied in (dict(operation='single_word_store',site=dependencies[0].site,status='unresolved_object_store_policy'),None):
            with self.subTest(supplied=supplied),self.assertRaises(AssertionError):
                config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=(),owned_dependencies=(supplied,))

    def test_current_owned_sites_bind_typed_functions_bytes_and_relocations(self):
        import config_owned_access,config_default
        from dataclasses import replace
        for code,data in ((0,0),(0x2800,0x19000)):
            objects,records,item=RefillChildInterfaces.fixture(self,'sizez',code,data)
            instance=next(v for v in item['instances'] if v['shape']=='advanced_for_channel_z')
            advanced=replace(self.fixture(),anchor=instance['address']+code,site=instance['address']+code+84)
            self.assertEqual(config_owned_access.current_owned_access(objects,advanced),advanced)
            objects2,records2,root,frame=DefaultPreparationInterfaces.fixture(self,code,data)
            result=config_default.default_preparation(objects2,root,records2,frame,())
            config=config_owned_access.OwnedAccess('configuration_versions',result['proof']['function'],
                result['proof']['function']+28,'default',result['config'],frame,'r8',result['protected'])
            self.assertEqual(config_owned_access.current_owned_access(objects2,config),config)
            from config_regions import borrow
            widgets=tuple(config_owned_access.OwnedAccess('widget_colors',result['continuation'],result['continuation']+670,
                'widget'+str(i),borrow(result['config'].storage,444+220*i,220,frame.phase,writable=True),
                frame,'r4',result['protected'],i) for i in range(8))
            for widget in widgets:self.assertEqual(config_owned_access.current_owned_access(objects2,widget),widget)
            for current,value in ((objects,advanced),(objects2,config),(objects2,widgets[0])):
                section=next(s for s in current.elf.sections if s[2]&2 and s[3]<=value.site<value.site+4<=s[3]+s[5])
                index=section[4]+value.site-section[3];raw=current.elf.data;mutant=bytearray(raw);mutant[index]^=1
                current.elf.data=bytes(mutant)
                with self.subTest(role=value.role,code=code),self.assertRaises(AssertionError):
                    config_owned_access.current_owned_access(current,value)
                current.elf.data=raw

    def test_widget_roles_keep_all_eight_owner_index_and_receiver_pairs(self):
        import config_owned_access
        from config_regions import borrow
        from dataclasses import replace
        config=self.fixture('configuration_versions')
        for i in range(8):
            value=config_owned_access.OwnedAccess('widget_colors',0x10030000,0x10030000+670,'widget'+str(i),
                borrow(config.owner.storage,444+220*i,220,config.frame.phase,writable=True),
                config.frame,'r4',config.protected,i)
            record=value.to_record()
            self.assertEqual(config_owned_access.read_owned_access(record),value)
            self.assertEqual(record['receiver_start']+0x952,value.owner.start+182)
            for fault in ('index','offset','short','phase','receiver','float_receiver'):
                changed=json.loads(json.dumps(record))
                if fault=='index':changed['loop_index']=(i+1)%8
                elif fault=='offset':changed['owner']['offset']+=4
                elif fault=='short':changed['owner']['extent']=219
                elif fault=='phase':changed['owner']['storage']['phase']='published'
                elif fault=='receiver':changed['receiver_start']+=220
                else:changed['receiver_start']=float(changed['receiver_start'])
                with self.subTest(i=i,fault=fault),self.assertRaises(AssertionError):config_owned_access.read_owned_access(changed)
            for invalid in (-1,8,True,None):
                with self.subTest(index=invalid),self.assertRaises(AssertionError):replace(value,loop_index=invalid)

    def test_direct_policy_and_stack_readers_reject_numeric_and_boolean_aliases(self):
        import config_platform
        from test_validation_policy import MemoryQualification
        from copy import deepcopy
        elf,eav,stack=MemoryQualification.owned_fixture(MemoryQualification())
        expected=config_platform.current_image_policy(stack['policies'][0],elf,eav)
        for fault in ('width','owner_mutability','frame_mutability','loop_index'):
            for reader in ('policy','stack'):
                changed=deepcopy(stack);entry=changed['policies'][0]['owned_dependencies'][0]
                if fault=='width':entry['width']=4.0
                elif fault=='owner_mutability':entry['owner']['writable']=1
                elif fault=='frame_mutability':entry['frame']['writable']=1
                else:changed['policies'][0]['owned_dependencies'][2]['loop_index']=0.0
                with self.subTest(fault=fault,reader=reader),self.assertRaises(AssertionError):
                    if reader=='policy':config_platform.read_policy(changed['policies'][0],expected)
                    else:config_platform.read_stack_result(changed,expected,stack['headroom'])

class DefaultWidgetLoop(unittest.TestCase):
    def fixture(self,code_shift=0,data_shift=0):
        objects,records,root,frame=DefaultPreparationInterfaces.fixture(self,code_shift,data_shift)
        donor,item=LoggerObjects.fixture(self,'sizez',code_shift,data_shift,fixture_file='config-default-widget.json')
        data=bytearray(objects.elf.data)
        for index,original,encoded in item['segments']:
            source=donor.elf.sections[index];dest=objects.elf.sections[index]
            shift=0 if dest[2]&1 else code_shift if dest[2]&4 else data_shift
            at=original+shift;n=len(bytes.fromhex(encoded));a=source[4]+at-source[3];b=dest[4]+at-dest[3]
            data[b:b+n]=donor.elf.data[a:a+n]
        for original in item['relocations']:
            section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=original+(code_shift if s[2]&4 else data_shift)<s[3]+s[5])
            at=original+(code_shift if section[2]&4 else data_shift);index=section[4]+at-section[3]
            value=struct.unpack_from('<I',data,index)[0]
            target=next(s for s in objects.elf.sections if s[2]&2 and
                s[3]-(0 if s[2]&1 else code_shift if s[2]&4 else data_shift)<=value&~1<
                s[3]-(0 if s[2]&1 else code_shift if s[2]&4 else data_shift)+s[5])
            struct.pack_into('<I',data,index,value+(0 if target[2]&1 else code_shift if target[2]&4 else data_shift))
        objects.elf.data=bytes(data);objects.elf.functions.update(donor.elf.functions);objects.sizes.update(donor.sizes)
        objects.elf.symbol_entries=list(dict.fromkeys(objects.elf.symbol_entries+donor.elf.symbol_entries))
        for row in item['records']:
            op=row['text'].split(None,1);at=row['address']+code_shift;operand=op[1] if len(op)>1 else ''
            if config_stack.transfer_family(op[0].removesuffix('.w')) in ('b','bl','cbz','cbnz'):
                operand=re.sub(r'0x([0-9a-f]+)',lambda m:hex(int(m[1],16)+code_shift),operand)
            records[at]=Instruction(at,bytes.fromhex(row['bytes']),op[0],operand)
        return objects,records,root,frame

    def test_paired_widget_phase_keeps_eight_indices_epochs_and_failure_edges(self):
        import config_default_widget
        for code,data in ((0,0),(0x2800,0x19000)):
            objects,records,root,frame=self.fixture(code,data)
            result=config_default_widget.widget_loop(objects,root,records,frame,())
            self.assertEqual(tuple((v['index'],v['channel'],v['offset']) for v in result['iterations']),
                ((0,0,444),(1,7,664),(2,6,884),(3,2,1104),(4,1,1324),(5,3,1544),(6,4,1764),(7,5,1984)))
            self.assertEqual(result['normal_exit_count'],8)
            self.assertEqual(set(result['diagnostics']),{'label_capacity','warning_capacity','widget_count'})
            self.assertEqual(len(result['target_access_requirements']),10)
            for iteration in result['iterations']:
                self.assertEqual(iteration['initialized_label'].view.start,iteration['widget'].start+156)
                self.assertEqual(iteration['initialized_warning'].view.start,iteration['widget'].start+36)
                self.assertEqual(iteration['initialized_label'].length.lower,iteration['source_case']['length'])
                self.assertEqual(tuple((v.start-frame.start,v.extent) for v in
                    iteration['opaque_copies'][3]['retained_scratch_epochs']),((84,28),(182,24)))
                self.assertEqual(iteration['opaque_copies'][3]['physical_activation_paths'],((8,),(72,)))
            self.assertEqual(result['diagnostics']['label_capacity']['error_payload_bytes'],0)
            self.assertEqual(result['diagnostics']['warning_capacity']['argument_address'],frame.start+351)
            self.assertTrue(result['requires_actual_incoming_exclusive_slot_fault_SP_and_whole_parent_graph'])

    def test_loop_guard_stride_receiver_copy_and_selector_operands_cannot_drift(self):
        import config_default_widget
        for offset,operand in ((0x2ce,'r0, r11, #0xdb'),(0x28,'r2, r5'),(0x18e,'r2, #0x41'),
            (0x7e,'r8, [sp, #0x3c]'),(0x1b0,'r0, [r4, #0x977]'),(0x2ca,'r0, [r4, #0x975]')):
            objects,records,root,frame=self.fixture();start=(objects.elf.functions[root][0]&~1)+340;at=start+offset
            old=records[at];records[at]=Instruction(at,old.raw,old.mnemonic,operand)
            with self.subTest(offset=offset),self.assertRaises(AssertionError):config_default_widget.widget_loop(objects,root,records,frame,())

    def test_foreign_entry_data_ingress_and_published_phase_are_not_prepared_state(self):
        import config_default_widget
        from dataclasses import replace
        for fault in ('entry','table_data','prior_data','phase'):
            objects,records,root,frame=self.fixture();start=(objects.elf.functions[root][0]&~1)+340
            if fault=='phase':frame=replace(frame,phase='published')
            else:
                target=start+({'entry':0,'table_data':134,'prior_data':256}[fault]);at=0x10060000
                records[at]=Instruction(at,bytes(2),'b',hex(target))
            with self.subTest(fault=fault),self.assertRaises(AssertionError):config_default_widget.widget_loop(objects,root,records,frame,())

    def test_paired_label_and_limit_tables_reject_while_numeric_payload_stays_abstract(self):
        import config_default_widget
        for fault in ('labels','length','limit','numeric'):
            objects,records,root,frame=self.fixture();raw=bytearray(objects.elf.data)
            at={'labels':0x1003c464,'length':0x1002c008,'limit':0x1002bfa0,'numeric':0x1002c38c}[fault]
            section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=at<s[3]+s[5]);index=section[4]+at-section[3]
            if fault=='labels':raw[index:index+4]=raw[index+28:index+32]
            else:raw[index]^=1
            objects.elf.data=bytes(raw)
            if fault=='numeric':
                result=config_default_widget.widget_loop(objects,root,records,frame,())
                self.assertTrue(result['requires_actual_incoming_exclusive_slot_fault_SP_and_whole_parent_graph'])
            else:
                with self.subTest(fault=fault),self.assertRaises(AssertionError):config_default_widget.widget_loop(objects,root,records,frame,())

    def test_shared_copy_preserves_live_output_and_parent_guards_across_restored_tail(self):
        from config_refill_children import opaque_copy_interface
        from config_regions import Storage,borrow
        from dataclasses import replace
        fixture=RefillChildInterfaces()
        objects,records,proof,frame,output=fixture.args('sizez','advanced_for_channel_z')
        source=borrow(frame,0,48,frame.phase);target=objects.elf.functions['__aeabi_memcpy'][0]&~1
        result=opaque_copy_interface(objects,records,target,source,output,48,frame,())
        self.assertEqual(result['physical_activation_paths'],((8,),(72,)))
        self.assertTrue(result['initialized_semantic_bytes_not_inferred'])
        guard=Storage(frame.space,'live ancestor',frame.start-64,4,4,frame.phase,False)
        for fault in ('deep_guard','overlap','readonly','wrong_count','foreign_source','unaligned_SP'):
            src,dest,active,protected=source,output,frame,()
            if fault=='deep_guard':protected=(guard,)
            elif fault=='overlap':dest=borrow(frame,0,48,frame.phase,writable=True)
            elif fault=='readonly':dest=replace(dest,writable=False)
            elif fault=='wrong_count':src=borrow(frame,0,49,frame.phase)
            elif fault=='foreign_source':src=borrow(Storage('address','unbound bytes',0x20000000,48,4,frame.phase,False),0,48,frame.phase)
            else:active=Storage(frame.space,'wrongSP',frame.start+4,frame.extent,4,frame.phase,True)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                opaque_copy_interface(objects,records,target,src,dest,48,active,protected)

class DefaultSourceTerminal(unittest.TestCase):
    def fixture(self,code_shift=0,data_shift=0):
        objects,records,root,frame=DefaultWidgetLoop.fixture(self,code_shift,data_shift)
        donor,child_records,item=RefillChildInterfaces.fixture(self,'sizez',code_shift,data_shift)
        raw=bytearray(objects.elf.data)
        for index,original,encoded in item['segments']:
            section=objects.elf.sections[index];source=donor.elf.sections[index]
            address=original+(code_shift if section[2]&4 else data_shift);n=len(bytes.fromhex(encoded))
            raw[section[4]+address-section[3]:section[4]+address-section[3]+n]=donor.elf.data[source[4]+address-source[3]:source[4]+address-source[3]+n]
        objects.elf.functions.update(donor.elf.functions);objects.sizes.update(donor.sizes);records.update(child_records)
        objects.elf.symbol_entries=list(dict.fromkeys(objects.elf.symbol_entries+donor.elf.symbol_entries))
        extra=json.loads((Path(__file__).parent/'fixtures/config-default-sources.json').read_text())
        for index,original,encoded in extra['segments']:
            section=objects.elf.sections[index];address=original+(code_shift if section[2]&4 else data_shift)
            offset=section[4]+address-section[3];raw[offset:offset+len(bytes.fromhex(encoded))]=bytes.fromhex(encoded)
        objects.elf.functions.update({n:(v[0]+code_shift,v[1],v[2]) for n,v in extra['functions'].items()})
        objects.sizes.update({int(a)+code_shift:v for a,v in extra['sizes'].items()})
        for entry in extra['symbol_entries']:
            entry=list(entry);entry[1]+=code_shift
            if tuple(entry) not in objects.elf.symbol_entries:objects.elf.symbol_entries.append(tuple(entry))
        for original in extra['pointer_cells']:
            section=next(v for v in objects.elf.sections if v[2]&2 and v[3]<=original+code_shift<v[3]+v[5])
            offset=section[4]+original+code_shift-section[3];value=struct.unpack_from('<I',raw,offset)[0]
            target=next(v for v in objects.elf.sections if v[2]&2 and
                v[3]-(0 if v[2]&1 else code_shift if v[2]&4 else data_shift)<=value<
                v[3]-(0 if v[2]&1 else code_shift if v[2]&4 else data_shift)+v[5])
            struct.pack_into('<I',raw,offset,value+(0 if target[2]&1 else code_shift if target[2]&4 else data_shift))
        objects.elf.data=bytes(raw)
        return objects,records,root,frame

    def test_source_array_pairs_reset_and_terminal_validator_handoff(self):
        import config_default_sources
        for code,data in ((0,0),(0x2800,0x19000)):
            objects,records,root,frame=self.fixture(code,data)
            result=config_default_sources.source_terminal(objects,root,records,frame,())
            self.assertEqual(tuple((r['id'],r['channel'],r['offset']) for r in result['iterations']),
                ((5,0,32),(3,1,54),(1,2,76),(14,3,98),(2,4,120),(15,5,142),(4,6,164),(13,7,186)))
            self.assertEqual(result['normal_exit_count'],8)
            self.assertEqual(result['outgoing']['r8'],frame.start+112)
            self.assertEqual(result['outgoing']['result'].start,frame.start+236)
            self.assertNotEqual(result['outgoing']['r1'],result['widget']['prepared']['config'].start)
            self.assertTrue(result['outgoing']['requires_specialized_validator_receiver_Result_preservation_and_graph'])
            self.assertEqual(len(result['target_access_requirements']),10)
            off=result['iterations'][0]
            self.assertEqual(off['poll'],('Off',None))
            self.assertEqual(off['opaque_poll_payload'],(2,4))
            self.assertEqual(off['opaque_route_payload'],(6,10))
            self.assertEqual(off['opaque_copy']['physical_activation_paths'],((8,),(72,)))
            self.assertEqual(result['history_count'],8)
            self.assertEqual(result['led_padding'],(1,5,9,11,15,19,25))

    def test_source_reset_stride_copy_history_and_receiver_operands_are_closed(self):
        import config_default_sources
        for offset,operand in ((2,'r8, sp, #0x74'),(200,'r8, sp, #0x74'),(198,'r5, #0x18'),
            (170,'r2, #0xb'),(286,'r0, #0x9'),(336,'r0, r1')):
            objects,records,root,frame=self.fixture();start=(objects.elf.functions[root][0]&~1)+1068;at=start+offset
            old=records[at];records[at]=Instruction(at,old.raw,old.mnemonic,operand)
            with self.subTest(offset=offset),self.assertRaises(AssertionError):
                config_default_sources.source_terminal(objects,root,records,frame,())

    def test_source_ids_channel_controls_and_led_selectors_stay_typed(self):
        import config_default_sources
        for fault,address in (('ids',0x10020528),('channels',0x1002bf60),('controls',0x10020544),
            ('led_kind',0x10035694),('led_padding',0x10035695)):
            objects,records,root,frame=self.fixture();raw=bytearray(objects.elf.data)
            section=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=address<s[3]+s[5]);index=section[4]+address-section[3]
            raw[index]^=1;objects.elf.data=bytes(raw)
            if fault=='led_padding':
                result=config_default_sources.source_terminal(objects,root,records,frame,())
                self.assertTrue(result['outgoing']['requires_specialized_validator_receiver_Result_preservation_and_graph'])
            else:
                with self.subTest(fault=fault),self.assertRaises(AssertionError):
                    config_default_sources.source_terminal(objects,root,records,frame,())

    def test_foreign_source_entry_or_interior_cannot_borrow_widget_handoff(self):
        import config_default_sources
        for offset in (0,18):
            objects,records,root,frame=self.fixture();start=(objects.elf.functions[root][0]&~1)+1068
            records[0x10060000]=Instruction(0x10060000,bytes(2),'b',hex(start+offset))
            with self.subTest(offset=offset),self.assertRaises(AssertionError):
                config_default_sources.source_terminal(objects,root,records,frame,())

    def test_two_aligned_LED_object_does_not_satisfy_the_word_LDM_consumer(self):
        import config_default_sources
        objects,records,root,frame=self.fixture();raw=bytearray(objects.elf.data)
        old,pool=0x10035694,0x10020eb8
        section=next(v for v in objects.elf.sections if v[1]==1 and v[2]&7==2 and v[5]>512)
        offset=raw[section[4]:section[4]+section[5]].find(bytes(128));self.assertGreaterEqual(offset,0)
        new=section[3]+offset+((2-section[3]-offset)%4)
        def index(address):
            owner=next(v for v in objects.elf.sections if v[2]&2 and v[3]<=address<v[3]+v[5])
            return owner[4]+address-owner[3]
        source=bytes(raw[index(old):index(old)+26]);raw[index(new):index(new)+26]=source
        struct.pack_into('<I',raw,index(pool),new);objects.elf.data=bytes(raw)
        self.assertEqual(new%4,2)
        with self.assertRaisesRegex(AssertionError,'alignment'):
            config_default_sources.source_terminal(objects,root,records,frame,())

class BoundedMixedSourceWindows(unittest.TestCase):
    def fixture(self):
        base=0x10001000;raw=bytes.fromhex('01e0aabbccdd7047');name='source_parent'
        elf=SimpleNamespace(data=raw,sections=[(0,1,6,base,0,8,0,0,2,0)],functions={name:(base|1,8,0)},
            mapping_symbols=[('$t',base,0),('$d',base+2,0),('$t.1',base+6,0)])
        objects=config_stack.DiagnosticObjects(elf,{base:8})
        records={base:Instruction(base,raw[:2],'b',hex(base+6)),base+6:Instruction(base+6,raw[6:],'bx','lr')}
        contract=dict(extent=8,mapping=[[0,2,'t'],[2,6,'d'],[6,8,'t']],
            instructions=[(0,2,'b @self+6'),(6,2,'bx lr')],objects=[],data=[(2,4,'aabbccdd')])
        return objects,name,records,contract,base

    def test_explicit_bounded_partition_covers_exact_current_code_and_data(self):
        from config_finite import _literal_body
        objects,name,records,contract,base=self.fixture()
        proof=_literal_body(objects,name,records,name,'mixed source phase',contract,None,None,window=base)
        self.assertEqual((proof['function'],proof['extent']),(base,8))

    def test_old_terminal_windows_do_not_inherit_mixed_permission_or_uncovered_data(self):
        from config_finite import _literal_body
        from copy import deepcopy
        for fault in ('terminal','partition','missing_data','overlap_data','data_code'):
            objects,name,records,contract,base=self.fixture();contract=deepcopy(contract)
            if fault=='terminal':contract.pop('mapping')
            elif fault=='partition':contract['mapping'][1][1]=4
            elif fault=='missing_data':contract['data']=[]
            elif fault=='overlap_data':contract['data'].append((2,4,'aabbccdd'))
            else:records[base+2]=Instruction(base+2,bytes.fromhex('aabb'),'nop','')
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                _literal_body(objects,name,records,name,'mixed source phase',contract,None,None,window=base)

class OrdinaryContexts(unittest.TestCase):
    def test_one_shot_allocations_keep_overlap_and_duplicate_identity_checks(self):
        from config_regions import Storage,require_disjoint
        saved=Storage('stack:boot','saved_r8',284,4,4,'validation',False)
        overlap=Storage('stack:boot','output',280,12,2,'validation',True)
        duplicate=Storage('stack:boot','saved_r8',288,4,4,'validation',False)
        adjacent=Storage('stack:boot','output',288,12,2,'validation',True)
        for container in (tuple,list,iter,lambda values:(v for v in values)):
            for other in (overlap,duplicate):
                with self.subTest(container=container,other=other),self.assertRaises(AssertionError):
                    require_disjoint(container((saved,other)))
            require_disjoint(container((saved,adjacent)))

    def test_one_shot_protection_remains_live_for_every_declared_write(self):
        from config_regions import Storage,borrow,require_writes
        saved=Storage('stack:boot','saved_r8',284,4,4,'validation',False)
        owner=Storage('stack:boot','output',280,12,2,'validation',True)
        view=borrow(owner,0,12,'validation',writable=True)
        for container in (tuple,list,iter,lambda values:(v for v in values)):
            for writes in (((view,0,12),),((view,0,4),(view,4,4))):
                with self.subTest(container=container,writes=writes),self.assertRaises(AssertionError):
                    require_writes((view,),iter(writes),container((saved,)))

    def test_one_shot_available_views_are_reused_for_all_writes(self):
        from config_regions import Storage,borrow,require_writes
        owner=Storage('stack:boot','output',280,12,2,'validation',True)
        view=borrow(owner,0,12,'validation',writable=True)
        saved=Storage('stack:boot','saved_r8',292,4,4,'validation',False)
        for container in (tuple,list,iter,lambda values:(v for v in values)):
            require_writes(container((view,)),iter(((view,0,4),(view,8,4))),container((saved,)))

    def test_checked_borrows_and_helper_writes_keep_exact_parent_ownership(self):
        from config_regions import Storage,borrow,require_writes
        local=Storage('stack:boot','scratch',0,184,4,'validation',True)
        saved=Storage('stack:boot','saved_r11',264,4,4,'validation',False)
        view=borrow(local,104,22,'validation',writable=True)
        require_writes((view,),((view,0,22),),(saved,))
        for offset,size in ((-1,1),(0,23),(21,2),(0,0x100000000)):
            with self.subTest(offset=offset,size=size),self.assertRaises(AssertionError):
                require_writes((view,),((view,offset,size),),(saved,))
        with self.assertRaises(AssertionError):borrow(local,0,4,'publication')

    def test_helper_output_cannot_alias_protected_carrier_even_with_another_owner_name(self):
        from config_regions import Storage,borrow,require_writes,require_disjoint
        saved=Storage('stack:boot','saved_r8',284,4,4,'validation',False)
        fake=Storage('stack:boot','output',280,12,2,'validation',True)
        with self.assertRaises(AssertionError):require_disjoint((saved,fake))
        view=borrow(fake,0,12,'validation',writable=True)
        with self.assertRaises(AssertionError):require_writes((view,),((view,0,12),),(saved,))
        adjacent=Storage('stack:boot','output',288,12,2,'validation',True)
        require_disjoint((saved,adjacent))

    def test_call_interfaces_preserve_receiver_levels_objects_and_source_domains(self):
        from config_regions import Storage,borrow,Argument,require_arguments
        pool=Storage('address','boot_pool',0x20000000,48,4,'entry',True)
        executor=Storage('stack:main','executor',0,8,4,'entry',True)
        future=Argument('future',borrow(pool,40,8,'entry'),0)
        exe=Argument('executor',borrow(executor,0,8,'entry'),0)
        require_arguments({'r0':future,'r1':exe},{'r0':future,'r1':exe})
        # The shared raw initializer role cannot substitute executor for future.
        with self.assertRaises(AssertionError):require_arguments({'r0':exe},{'r0':future,'r1':exe})
        with self.assertRaises(AssertionError):require_arguments({'r0':Argument('future',future.view,1)},{'r0':future})
        small=Argument('u32',None,0,32,frozenset(range(128)))
        wide=Argument('u32',None,0,32,frozenset((0,128)))
        require_arguments({'r1':Argument('u32',None,0,32,frozenset((0,18)))},{'r1':small})
        with self.assertRaises(AssertionError):require_arguments({'r1':wide},{'r1':small})

    def test_join_requires_each_predecessor_to_supply_the_same_owned_pointer_and_length(self):
        from config_regions import Storage,borrow,Argument,require_join
        label_a=Storage('address','label_a',0x10000000,9,1,'defaults',False)
        label_b=Storage('address','label_b',0x10000020,3,1,'defaults',False)
        a=Argument('label',borrow(label_a,0,9,'defaults'),0)
        b=Argument('label',borrow(label_b,0,3,'defaults'),0)
        n9=Argument('length',None,0,32,frozenset((9,)))
        n3=Argument('length',None,0,32,frozenset((3,)))
        require_join(({'r11':a,'r4':n9},{'r11':a,'r4':n9}),{'r11':a,'r4':n9})
        with self.assertRaises(AssertionError):require_join(({'r11':a,'r4':n3},{'r11':b,'r4':n9}),{'r11':a,'r4':n9})

    def test_readonly_borrow_of_mutable_owner_does_not_grant_a_helper_write(self):
        from config_regions import Storage,borrow,require_writes,Argument,require_arguments
        owner=Storage('stack:caller','Config',0,32,4,'validated',True)
        readonly=borrow(owner,0,32,'validated')
        with self.assertRaises(AssertionError):require_writes((readonly,),((readonly,0,4),),())
        mutable=borrow(owner,0,32,'validated',writable=True)
        with self.assertRaises(AssertionError):require_arguments({'r0':Argument('Config',readonly,1)},
                                                               {'r0':Argument('Config',mutable,1)})

    def test_context_ranges_and_domains_reject_wrap_empty_values_and_foreign_coordinate_spaces(self):
        from config_regions import Storage,borrow,Argument,require_join
        for start,extent in ((0xfffffff0,32),(-1,4),(0,0)):
            with self.subTest(start=start,extent=extent),self.assertRaises(AssertionError):
                Storage('address','input',start,extent,1,'decode',False)
        with self.assertRaises(AssertionError):Storage('arbitrary','input',0,4,1,'decode',False)
        owner=Storage('stack:root','scratch',-32,32,4,'decode',True)
        for offset,extent in ((-1,2),(31,2),(0,0)):
            with self.subTest(offset=offset,extent=extent),self.assertRaises(AssertionError):borrow(owner,offset,extent,'decode')
        for domain in (frozenset(),frozenset((-1,)),frozenset((256,)),frozenset((True,))):
            with self.subTest(domain=domain),self.assertRaises(AssertionError):Argument('u8',None,0,8,domain)
        with self.assertRaises(AssertionError):require_join((),{})


class OrdinaryCoverage(unittest.TestCase):
    def test_unsupported_multiple_load_pc_cannot_have_ordinary_fallthrough(self):
        from dataclasses import replace
        from config_regions import Transfer,audit_coverage
        for operation in ('ldmdb','ldmdb.w','ldmdbne.w'):
            objects,name,records,context,region=self.fixture();base=context[0]
            objects.elf.data=bytes.fromhex('80b53de9108080bd')
            objects.elf.sections=[(0,1,6,base,0,8,0,0,2,0)]
            objects.elf.functions[name]=(base|1,8,0);objects.elf.symbol_entries=[(name,base|1,8,18,0)]
            records={base:records[base],base+2:Instruction(base+2,bytes.fromhex('3de91080'),operation,'sp!, {r4, pc}'),
                     base+6:Instruction(base+6,bytes.fromhex('80bd'),'pop','{r7, pc}')}
            region=replace(region,end=base+8,instructions=tuple(records),
                transfers=(Transfer(base+6,'pop','return',None,'final low-save restore'),))
            # Semantic records exercise the inventory consequence; no decoder
            # execution or raw predication/encoding validity is claimed here.
            with self.subTest(operation=operation),self.assertRaises(AssertionError):
                audit_coverage(objects,name,records,context,(region,))

    def test_pc_relative_reads_and_reviewed_stack_returns_keep_their_classification(self):
        from config_regions import _control
        for operation,operands,expected in (
                ('ldr','r0, [pc, #0x4]',(None,False)),('ldmdb.w','r4!, {r0, r1}',(None,False)),
                ('ldmia.w','sp!, {r4, pc}',('return',False)),('ldmiaeq.w','sp!, {r4, pc}',('return',True))):
            with self.subTest(operation=operation):
                self.assertEqual(_control(Instruction(0,b'\0\0',operation,operands)),expected)

    def fixture(self,data=b''):
        from config_regions import Region,Transfer
        base=0x10000000;name='_RNvCs123_4test5frame';raw=bytes.fromhex('80b56f4680bd')+data
        elf=SimpleNamespace(data=raw,sections=[(0,1,6,base,0,len(raw),0,0,2,0)],
            functions={name:(base|1,len(raw),0)},symbol_entries=[(name,base|1,len(raw),18,0)],
            mapping_symbols=[('$t',base,0)]+([('$d',base+6,0)] if data else []))
        objects=config_stack.DiagnosticObjects(elf,{base:8})
        records={base:Instruction(base,raw[:2],'push','{r7, lr}'),
                 base+2:Instruction(base+2,raw[2:4],'mov','r7, sp'),
                 base+4:Instruction(base+4,raw[4:6],'pop','{r7, pc}')}
        context=(base,'boot','callsite-0')
        region=Region(context,'frame',base,base+6,tuple(records),(),
                      (Transfer(base+4,'pop','return',None,'restored low saves'),))
        return objects,name,records,context,region

    def test_complete_current_code_and_return_coverage_has_no_source_or_effect_exemption(self):
        from config_regions import audit_coverage
        objects,name,records,context,region=self.fixture();proof=audit_coverage(objects,name,records,context,(region,))
        self.assertEqual((proof['code_bytes'],proof['data_bytes'],proof['frame']),(6,0,8))
        self.assertTrue(proof['requires_authored_semantics_source_and_child_effects'])

    def test_uncovered_overlap_wrong_context_and_missing_return_are_rejected(self):
        from dataclasses import replace
        from config_regions import audit_coverage
        objects,name,records,context,region=self.fixture()
        cases=((replace(region,instructions=region.instructions[:-1]),),
               (region,region),(replace(region,context=(context[0],'different phase','callsite-0')),),
               (replace(region,transfers=()),))
        for regions in cases:
            with self.subTest(regions=regions),self.assertRaises(AssertionError):audit_coverage(objects,name,records,context,regions)

    def test_implicit_pc_write_or_unbound_register_transfer_cannot_hide_in_covered_bytes(self):
        from config_regions import audit_coverage
        for raw,mnemonic,operands in ((bytes.fromhex('a746'),'mov','pc, r4'),(bytes.fromhex('2047'),'bx','r4')):
            objects,name,records,context,region=self.fixture();at=context[0]+2;data=bytearray(objects.elf.data);data[2:4]=raw
            objects.elf.data=bytes(data);records[at]=Instruction(at,raw,mnemonic,operands)
            with self.subTest(operation=mnemonic),self.assertRaises(AssertionError):audit_coverage(objects,name,records,context,(region,))

    def test_cross_region_fallthrough_needs_its_exact_continuation(self):
        from dataclasses import replace
        from config_regions import audit_coverage,Region,Transfer
        objects,name,records,context,region=self.fixture();base=context[0]
        first=replace(region,end=base+4,instructions=(base,base+2),transfers=())
        last=replace(region,start=base+4,instructions=(base+4,))
        with self.assertRaises(AssertionError):audit_coverage(objects,name,records,context,(first,last))
        first=replace(first,transfers=(Transfer(base+2,'mov','fallthrough',base+4,'frame continuation'),))
        proof=audit_coverage(objects,name,records,context,(first,last));self.assertEqual(proof['cross_region_fallthrough'],1)
        bad=replace(first,transfers=(Transfer(base+2,'mov','fallthrough',base+2,'wrong continuation'),))
        with self.assertRaises(AssertionError):audit_coverage(objects,name,records,context,(bad,last))

    def test_mapped_data_requires_complete_current_bytes_and_cannot_be_code(self):
        from dataclasses import replace
        from config_regions import audit_coverage,Region
        objects,name,records,context,region=self.fixture(b'\x12\x34');base=context[0]
        with self.assertRaises(AssertionError):audit_coverage(objects,name,records,context,(region,))
        data=Region(context,'data',base+6,base+8,(),((base+6,b'\x12\x34'),),())
        self.assertEqual(audit_coverage(objects,name,records,context,(region,data))['data_bytes'],2)
        for bad in (replace(data,data=((base+6,b'\x12\x35'),)),replace(data,data=((base+4,b'\x80\xbd'),))):
            with self.subTest(data=bad),self.assertRaises(AssertionError):audit_coverage(objects,name,records,context,(region,bad))

    def test_a_matched_issuer_region_does_not_cover_the_rest_of_its_loader(self):
        from config_regions import Region,Transfer,audit_coverage
        import config_upstream
        objects,records,item=IssuerRegions().fixture('sizez');name=item['issuer']['name']
        proof=config_upstream.issuer_region(objects,name,records);start=proof['window'];end=start+proof['extent']
        context=(proof['caller'],'load','callsite-0')
        covered=tuple(a for a in sorted(records) if start<=a<end)
        region=Region(context,'issuer',start,end,covered,(),())
        with self.assertRaises(AssertionError):audit_coverage(objects,name,records,context,(region,))

    def test_conditional_external_tail_must_keep_its_untaken_fallthrough(self):
        from dataclasses import replace
        from config_regions import Region,Transfer,audit_coverage
        objects,name,records,context,region=self.fixture();base=context[0];child=base+0x80
        data=bytearray(0x82);data[:6]=bytes.fromhex('80b53dd180bd');data[0x80:]=bytes.fromhex('7047');objects.elf.data=bytes(data)
        objects.elf.sections=[(0,1,6,base,0,len(data),0,0,2,0)]
        objects.elf.functions['child']=(child|1,2,0);objects.elf.symbol_entries.append(('child',child|1,2,18,0));objects.sizes[child]=0
        last=replace(region,start=base+4,instructions=(base+4,))
        for operation,raw in (('bne','3dd1'),('cbz','e8b3'),('cbnz','e8bb')):
            value=bytes.fromhex(raw);objects.elf.data=objects.elf.data[:2]+value+objects.elf.data[4:]
            records[base+2]=Instruction(base+2,value,operation,('r0, ' if operation!='bne' else '')+hex(child))
            first=replace(region,end=base+4,instructions=(base,base+2),transfers=(Transfer(base+2,operation,'tail',child,'conditional child','child'),))
            with self.subTest(operation=operation),self.assertRaises(AssertionError):
                audit_coverage(objects,name,records,context,(first,last))
            first=replace(first,transfers=first.transfers+(Transfer(base+2,operation,'fallthrough',base+4,'untaken conditional'),))
            with self.subTest(positive=operation):
                self.assertEqual(audit_coverage(objects,name,records,context,(first,last))['cross_region_fallthrough'],1)

    def test_unconditional_bound_register_branch_has_no_fallthrough(self):
        from dataclasses import replace
        from config_regions import Transfer,audit_coverage
        objects,name,records,context,region=self.fixture();base=context[0]
        data=bytearray(objects.elf.data);data[2:4]=bytes.fromhex('2047');objects.elf.data=bytes(data)
        records[base+2]=Instruction(base+2,bytes.fromhex('2047'),'bx','r4')
        first=replace(region,end=base+4,instructions=(base,base+2),transfers=(Transfer(base+2,'bx','branch',base+4,'bound local register target'),))
        last=replace(region,start=base+4,instructions=(base+4,))
        self.assertEqual(audit_coverage(objects,name,records,context,(first,last))['cross_region_fallthrough'],0)

    def test_all_data_mapping_cannot_serve_as_a_thumb_function_entry(self):
        from config_regions import Region,audit_coverage
        objects,name,records,context,region=self.fixture();base=context[0]
        objects.elf.mapping_symbols=[('$d',base,0)]
        data=Region(context,'data',base,base+6,(),((base,objects.elf.data),),())
        with self.assertRaises(AssertionError):audit_coverage(objects,name,{},context,(data,))

    def test_foreign_direct_transfer_cannot_enter_a_function_region_interior(self):
        from config_regions import audit_coverage
        objects,name,records,context,region=self.fixture();base=context[0]
        records[base-4]=Instruction(base-4,bytes.fromhex('01e0'),'b',hex(base+2))
        with self.assertRaises(AssertionError):audit_coverage(objects,name,records,context,(region,))

    def test_exact_post_return_nop_padding_requires_no_incoming_or_fallthrough_edge(self):
        from dataclasses import replace
        from config_regions import audit_coverage,Region,Transfer
        objects,name,records,context,region=self.fixture();base=context[0]
        objects.elf.data+=bytes.fromhex('00bf');objects.elf.sections=[(0,1,6,base,0,8,0,0,2,0)]
        pointer,_,index=objects.elf.functions[name];objects.elf.functions[name]=(pointer,8,index)
        records[base+6]=Instruction(base+6,bytes.fromhex('00bf'),'nop','')
        padding=Region(context,'padding',base+6,base+8,(base+6,),(),())
        self.assertEqual(audit_coverage(objects,name,records,context,(region,padding))['code_bytes'],8)
        for kind in ('direct','table'):
            changed=dict(records);changed[base+4]=Instruction(base+4,bytes.fromhex('00e0'),'b',hex(base+6))
            if kind=='table':changed[base+4]=Instruction(base+4,bytes.fromhex('00e0'),'tbb','[pc, r0]')
            objects.elf.data=objects.elf.data[:4]+bytes.fromhex('00e0')+objects.elf.data[6:]
            edge=Transfer(base+4,changed[base+4].mnemonic,'table' if kind=='table' else 'branch',
                          (base+6,) if kind=='table' else base+6,'forbidden padding ingress')
            with self.subTest(kind=kind),self.assertRaises(AssertionError):
                audit_coverage(objects,name,changed,context,(replace(region,transfers=(edge,)),padding))
        objects.elf.data=objects.elf.data[:4]+bytes.fromhex('80bd')+objects.elf.data[6:]
        changed=dict(records);changed[base+4]=Instruction(base+4,bytes.fromhex('3f46'),'mov','r7, r7')
        objects.elf.data=objects.elf.data[:4]+bytes.fromhex('3f46')+objects.elf.data[6:]
        fall=Transfer(base+4,'mov','fallthrough',base+6,'forbidden padding fallthrough')
        with self.assertRaises(AssertionError):
            audit_coverage(objects,name,changed,context,(replace(region,transfers=(fall,)),padding))
        objects.elf.data=objects.elf.data[:4]+bytes.fromhex('80bd')+objects.elf.data[6:]
        records[base-4]=Instruction(base-4,bytes.fromhex('03e0'),'b',hex(base+6))
        with self.assertRaises(AssertionError):audit_coverage(objects,name,records,context,(region,padding))


    def test_post_return_padding_can_precede_completely_owned_literal_data(self):
        from config_regions import Region,audit_coverage
        objects,name,records,context,region=self.fixture();base=context[0]
        objects.elf.data+=bytes.fromhex('00bf1234');objects.elf.sections=[(0,1,6,base,0,10,0,0,2,0)]
        pointer,_,index=objects.elf.functions[name];objects.elf.functions[name]=(pointer,10,index)
        objects.elf.mapping_symbols=[('$t',base,0),('$d',base+8,0)]
        records[base+6]=Instruction(base+6,bytes.fromhex('00bf'),'nop','')
        padding=Region(context,'padding',base+6,base+8,(base+6,),(),())
        data=Region(context,'data',base+8,base+10,(),((base+8,bytes.fromhex('1234')),),())
        proof=audit_coverage(objects,name,records,context,(region,padding,data))
        self.assertEqual((proof['code_bytes'],proof['data_bytes']),(8,2))
        with self.assertRaises(AssertionError):audit_coverage(objects,name,records,context,(region,padding))


class RootLocations(unittest.TestCase):
    def fixture(self,path,line=61,column=63):
        value=path.encode('utf-8');base=0x10000000
        blob=struct.pack('<4I',base+16,len(value),line,column)+value
        elf=SimpleNamespace(data=blob,sections=[(0,1,2,base,0,len(blob),0,0,4,0)])
        return config_stack.DiagnosticObjects(elf,{}),base

    def test_location_source_role_line_column_accept_actual_host_prefixes_and_separator_forms(self):
        import config_roots
        for path in ('D:/GitHub/project/crates/dashboard-config/src/slot.rs',
                     r'D:\GitHub\project\crates\dashboard-config\src\slot.rs',
                     '/home/ci/work/project/crates/dashboard-config/src/slot.rs',
                     '/home/żółw/project/crates/dashboard-config/src/slot.rs',
                     'crates/dashboard-config/src/slot.rs'):
            objects,address=self.fixture(path)
            proof=config_roots.root_location(objects,address,'reviewed_defaults')
            self.assertEqual(proof['filename_bytes'],path.encode('utf-8'))
            self.assertEqual((proof['line'],proof['column']),(61,63))

    def test_location_wrong_role_line_column_suffix_storage_or_nonpath_bytes_reject(self):
        import config_roots
        for fault in ('role','line','column','filename','prefix_only','nul','traversal','unbacked','writable','unaligned'):
            path='D:/repo/crates/dashboard-config/src/slot.rs'
            if fault=='filename':path=path.replace('slot.rs','slots.rs')
            if fault=='prefix_only':path+='-other'
            if fault=='nul':path='D:/bad\0name/crates/dashboard-config/src/slot.rs'
            if fault=='traversal':path='D:/repo/../crates/dashboard-config/src/slot.rs'
            objects,address=self.fixture(path,62 if fault=='line' else 61,64 if fault=='column' else 63)
            if fault=='unbacked':objects.elf.data=objects.elf.data[:-1]
            if fault=='writable':
                section=list(objects.elf.sections[0]);section[2]|=1;objects.elf.sections[0]=tuple(section)
            if fault=='unaligned':address+=1
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_roots.root_location(objects,address,'not_a_role' if fault=='role' else 'reviewed_defaults')


class PanicBodies(unittest.TestCase):
    def fixture(self,mode,code_shift=0,data_shift=0,guard_shift=0):
        spec=next(r for r in json.loads((Path(__file__).parent/'fixtures/config-panic.json').read_text()) if r['mode']==mode)
        if code_shift or data_shift or guard_shift:
            relocation={}
            for obj in spec['objects'].values():
                old=int(obj['value'],16)
                delta=code_shift if obj['kind']=='thumb_callback' else guard_shift if obj['kind']=='writable_guard_byte' else data_shift if ('bytes_hex' in obj or 'layout_words' in obj) else 0
                relocation[old]=old+delta;obj['value']=hex(old+delta)
                if 'address' in obj:obj['address']=hex(int(obj['address'],16)+code_shift)
                for target in obj.get('slots',{}).values():target['address']=hex(int(target['address'],16)+code_shift)
            for f in spec['functions']:
                body=bytearray.fromhex(f['body_hex']);lows={}
                for row in f['instructions']:
                    parts=row['operation'].split(None,1);mnemonic=parts[0];operands=parts[1] if len(parts)>1 else ''
                    pair=re.fullmatch(r'(r[0-9]+), #(0x[0-9a-f]+|[0-9]+)',operands)
                    if mnemonic=='movw' and pair:lows[pair[1]]=(row,int(pair[2],0))
                    elif mnemonic=='movt' and pair and pair[1] in lows:
                        lower,low=lows.pop(pair[1]);old=low|(int(pair[2],0)<<16)
                        if old in relocation:
                            for current,value in ((lower,relocation[old]&65535),(row,relocation[old]>>16)):
                                raw=bytes.fromhex(current['bytes_hex']);first,second=struct.unpack('<HH',raw)
                                raw=struct.pack('<HH',(first&~0x040f)|((value>>12)&15)|((value>>1)&0x400),
                                                (second&~0x70ff)|((value&0x700)<<4)|(value&255))
                                current['bytes_hex']=raw.hex();current['operation']=current['operation'].split(None,1)[0]+' '+pair[1]+', #'+hex(value)
                                body[current['offset']:current['offset']+len(raw)]=raw
                    if re.fullmatch(r'(?:b(?:l|eq|ne|cs|cc|mi|pl|vs|vc|hi|ls|ge|lt|gt|le)?|cbz|cbnz)(?:\.w)?',mnemonic) and '0x' in operands:
                        row['operation']=mnemonic+' '+re.sub(r'0x([0-9a-f]+)',lambda m:hex(int(m[1],16)+code_shift),operands,count=1)
                for lo,hi,kind in f['mapping']:
                    if kind!='$d':continue
                    for offset in range(lo,hi,4):
                        old=struct.unpack_from('<I',body,offset)[0]
                        struct.pack_into('<I',body,offset,relocation.get(old,old))
                f['body_hex']=body.hex();f['address']+=code_shift
            for f in spec['extra_functions']:f['address']=hex(int(f['address'],16)+code_shift)
        segments=[];functions={};sizes={};mappings=[];records={}
        for f in spec['functions']:
            start=f['address'];segments.append((start,bytes.fromhex(f['body_hex'])))
            for name in f['aliases']:functions[name]=(start|1,f['extent'],0)
            sizes[start]=f['frame']
            mappings.extend((kind,start+lo,0) for lo,hi,kind in f['mapping'])
            for row in f['instructions']:
                parts=row['operation'].split(None,1)
                at=start+row['offset'];records[at]=Instruction(at,bytes.fromhex(row['bytes_hex']),parts[0],parts[1] if len(parts)>1 else '')
        for f in spec['extra_functions']:
            start=int(f['address'],16)
            for name in f['raw_roles']:functions[name]=(start|1,f['extent'],0)
            sizes[start]=f['frame']
            if not any(a<=start<a+len(b) for a,b in segments):segments.append((start,bytes(f['extent'])))
        symbols=[];guard=None
        for key,obj in spec['objects'].items():
            value=int(obj['value'],16)
            if obj['kind']=='writable_guard_byte':guard=value;symbols.append((obj['raw_role'],value,1,1,2))
            elif obj['kind']=='defmt_interned_symbol_value':symbols.append((obj['raw_symbol'],value,1,17,3))
            elif 'bytes_hex' in obj:segments.append((value,bytes.fromhex(obj['bytes_hex'])))
            elif 'layout_words' in obj:
                pointers=[int(v['address'],16)|1 for slot,v in sorted(obj['slots'].items(),key=lambda p:int(p[0]))]
                segments.append((value,struct.pack('<'+'I'*(3+len(pointers)),*obj['layout_words'],*pointers)))
        code_lo=min(a for a,b in segments)
        ro=min(int(o['value'],16) for o in spec['objects'].values() if 'bytes_hex' in o or 'layout_words' in o)
        hi=max(a+len(b) for a,b in segments);blob=bytearray(hi-code_lo)
        for a,b in segments:blob[a-code_lo:a-code_lo+len(b)]=b
        meta=(0,1,0x200000,0,len(blob),64,0,0,1,0);blob+=bytes(64)
        elf=SimpleNamespace(data=bytes(blob),functions=functions,mapping_symbols=mappings,symbol_entries=symbols,
            sections=[(0,1,6,code_lo,0,ro-code_lo,0,0,2,0),(0,1,2,ro,ro-code_lo,hi-ro,0,0,4,0),
                      (0,8,3,guard,0,1,0,0,1,0),meta],named={'.defmt':meta})
        return config_stack.DiagnosticObjects(elf,sizes),records,spec

    def test_complete_panic_shapes_objects_and_guarded_reentry_all_modes(self):
        import config_panic
        for mode in ('speed2','sizes','sizez'):
            objects,records,spec=self.fixture(mode)
            for f in spec['functions']:
                proof=config_panic.panic_body(objects,f['aliases'][0],records)
                self.assertEqual(proof['shape'],f['shape'])
                if f['shape'].startswith('handler'):
                    self.assertEqual(proof['guarded_reentry_frames'],(32,32))

    def test_independent_panic_code_readonly_guard_relocation_preserves_ids(self):
        import config_panic
        for mode in ('speed2','sizes','sizez'):
            objects,records,spec=self.fixture(mode,0x1000,0x21000,0x100)
            for f in spec['functions']:
                proof=config_panic.panic_body(objects,f['aliases'][0],records)
                self.assertEqual(proof['function'],f['address'])
                if f['shape'].startswith('handler'):
                    self.assertEqual(proof['objects']['header_id'],14 if mode!='sizes' else 15)

    def test_guard_order_callback_cells_and_restoring_epilogue_mutations_reject(self):
        import config_panic
        for shape,offset,mnemonic,operands in [('handler_movwide',0x16,'strb','r2, [r0]'),
                ('handler_movwide',0x12,'cbnz','r2, 0x10048d0e'),
                ('handler_literal',0x2c,'str','r0, [sp, #0xc]'),
                ('panicinfo_literal',0x5c,'add','sp, #0x24'),
                ('panicinfo_epilogue',0x4,'bx','lr')]:
            mode='speed2' if shape=='handler_movwide' else 'sizez';objects,records,spec=self.fixture(mode)
            f=next(f for f in spec['functions'] if f['shape']==shape);at=f['address']+offset
            old=records[at];records[at]=Instruction(at,old.raw,mnemonic,operands)
            with self.subTest(shape=shape,offset=offset),self.assertRaises(AssertionError):
                config_panic.panic_body(objects,f['aliases'][0],records)

    def test_panic_tables_literals_terminal_address_and_current_callback_metadata_reject(self):
        import config_panic
        for mode in ('speed2','sizes','sizez'):
            for fault in ('table_header','table_slot','callback_frame','template','prefix','shcsr','guard_alias'):
                objects,records,spec=self.fixture(mode)
                shape='handler_literal' if mode=='sizez' else 'handler_movwide'
                f=next(f for f in spec['functions'] if f['shape']==shape)
                data=bytearray(objects.elf.data)
                key='sink_vtable' if fault.startswith('table_') or fault=='callback_frame' else ('outer_template' if fault=='template' else 'prefix')
                obj=spec['objects'].get(key)
                if fault=='guard_alias':objects.elf.symbol_entries.append(objects.elf.symbol_entries[0])
                elif fault=='shcsr':
                    with self.assertRaises(AssertionError):config_panic.panic_object(objects,0xe000ed28,'shcsr')
                    continue
                elif fault=='callback_frame':objects.sizes[int(obj['slots']['12']['address'],16)]=0
                else:
                    address=int(obj['value'],16)
                    section=next(s for s in objects.elf.sections if s[1]==1 and s[3]<=address<s[3]+s[5])
                    at=section[4]+address-section[3]
                    if fault=='table_header':struct.pack_into('<I',data,at+8,4)
                    elif fault=='table_slot':data[at+12:at+16]=data[at+16:at+20]
                    else:data[at]^=1
                    objects.elf.data=bytes(data)
                # Prefix belongs to PanicInfo rather than the outer handler.
                if fault=='prefix':f=next(f for f in spec['functions'] if f['shape'].startswith('panicinfo_') and f['shape']!='panicinfo_epilogue')
                # SizeZ's sink table is produced by the child write_fmt, not
                # this handler body; assert that independent object obligation.
                if mode=='sizez' and fault in ('table_header','table_slot','callback_frame'):
                    with self.subTest(mode=mode,fault=fault),self.assertRaises(AssertionError):
                        config_panic.panic_object(objects,int(obj['value'],16),'sink_vtable')
                else:
                    with self.subTest(mode=mode,fault=fault),self.assertRaises(AssertionError):
                        config_panic.panic_body(objects,f['aliases'][0],records)

    def test_sizez_literal_pool_binding_and_full_body_coverage_reject(self):
        import config_panic
        for fault in ('pool','mapping','partial','raw','epilogue_frame'):
            objects,records,spec=self.fixture('sizez')
            f=next(f for f in spec['functions'] if f['shape']=='handler_literal')
            if fault=='pool':
                data=bytearray(objects.elf.data);start=f['address']
                struct.pack_into('<I',data,start+0x54-objects.elf.sections[0][3],0xe000ed28)
                objects.elf.data=bytes(data)
            elif fault=='mapping':objects.elf.mapping_symbols=[m for m in objects.elf.mapping_symbols if m[1]!=f['address']+0x54]
            elif fault=='partial':del records[f['address']]
            elif fault=='raw':
                row=records[f['address']];records[row.address]=Instruction(row.address,bytes(len(row.raw)),row.mnemonic,row.operands)
            else:
                f=next(f for f in spec['functions'] if f['shape']=='panicinfo_literal')
                outline=next(f for f in spec['functions'] if f['shape']=='panicinfo_epilogue')
                objects.sizes[outline['address']]=8
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_panic.panic_body(objects,f['aliases'][0],records)


class DiagnosticRoots(unittest.TestCase):
    def test_returned_widget_view_and_unknown_debug_roots_reject(self):
        self.assertEqual(error_root_role(VALIDATION),'validation_error')
        for symbol in [WIDGET,WIDGET.replace('6Widget','4View'),
                       VALIDATION.replace('15ValidationError','6Widget'),
                       VALIDATION.replace('5Debug3fmt','5Debug7new_fmt')]:
            with self.subTest(symbol=symbol),self.assertRaises(AssertionError):error_root_role(symbol)

    def test_role_does_not_depend_on_crate_disambiguator_or_clone_address(self):
        moved=VALIDATION.replace('CshxbT6mSCLYQ_','Cs12345_').replace('Cs6m2Zc6X1WBq_','Cs98765_')+'.169'
        self.assertEqual(error_root_role(moved),'validation_error')


class TypedDiagnosticObjects(unittest.TestCase):
    def fixture(self, shift=0):
        data=bytearray(128)
        base=0x10000000+shift
        struct.pack_into('<4I',data,0,0,12,2,base+97)
        data[32:38]=b'\xc0\x02: \xc0\x00'
        data[48:69]=b'built-in widget count'
        elf=SimpleNamespace(data=bytes(data),sections=[(0,1,2,base,0,96,0,0,4,0),
                                                     (0,1,6,base+96,96,32,0,0,2,0)],
                            functions={VALIDATION:(base+97,8,1)})
        return elf,base,{base+96:72}

    def test_typed_debug_vtable_and_default_arguments_relocate_by_role(self):
        for shift in (0,0x1200):
            elf,base,sizes=self.fixture(shift)
            objects=config_stack.DiagnosticObjects(elf,sizes)
            self.assertEqual(objects.debug_vtable(base,'validation_error'),base+96)
            self.assertEqual(objects.default_template(base+32,'unwrap_failed'),(0,1))
            self.assertEqual(objects.tagged_literal(base+48,43,'widget_count'),b'built-in widget count')

    def test_typed_object_wrong_role_layout_pointer_and_metadata_reject(self):
        for mutation in ('role','wrong_approved_role','size','align','drop','interior','arm','writable',
                         'code_writable','code_extent','alias','section','truncated','metadata'):
            elf,base,sizes=self.fixture();data=bytearray(elf.data)
            if mutation=='role':elf.functions={WIDGET:(base+97,8,1)}
            if mutation=='wrong_approved_role':elf.functions={next(s for s,r in config_stack.ERROR_ROOTS.items() if r=='capacity_error'):(base+97,8,1)}
            if mutation=='size':struct.pack_into('<I',data,4,16)
            if mutation=='align':struct.pack_into('<I',data,8,4)
            if mutation=='drop':struct.pack_into('<I',data,0,base+97)
            if mutation=='interior':struct.pack_into('<I',data,12,base+99)
            if mutation=='arm':struct.pack_into('<I',data,12,base+96)
            if mutation=='writable':elf.sections[0]=(0,1,3,base,0,96,0,0,4,0)
            if mutation=='code_writable':elf.sections[1]=(0,1,7,base+96,96,32,0,0,2,0)
            if mutation=='code_extent':elf.functions[VALIDATION]=(base+97,64,1)
            if mutation=='alias':elf.functions[VALIDATION+'.12']=(base+97,10,1)
            if mutation=='section':elf.functions[VALIDATION]=(base+97,8,2)
            if mutation=='truncated':elf.sections[0]=(0,1,2,base,0,12,0,0,4,0)
            if mutation=='metadata':sizes={}
            elf.data=bytes(data)
            with self.subTest(mutation=mutation),self.assertRaises(AssertionError):
                config_stack.DiagnosticObjects(elf,sizes).debug_vtable(base,'validation_error')

    def test_default_template_pretty_wrong_arity_and_tagged_length_reject(self):
        for mutation in ('pretty','arity','literal_length','not_tagged','wrong_literal'):
            elf,base,sizes=self.fixture();data=bytearray(elf.data)
            if mutation=='pretty':data[32]=0xc1
            if mutation=='arity':data[37]=1
            if mutation=='wrong_literal':data[48]=ord('x')
            elf.data=bytes(data);objects=config_stack.DiagnosticObjects(elf,sizes)
            with self.subTest(mutation=mutation),self.assertRaises(AssertionError):
                if mutation in ('pretty','arity'):objects.default_template(base+32,'unwrap_failed')
                else:objects.tagged_literal(base+48,41 if mutation=='literal_length' else 42 if mutation=='not_tagged' else 43,'widget_count')

    def test_callback_extent_must_belong_to_its_declared_existing_code_section(self):
        elf,base,sizes=self.fixture()
        # A different valid RO/EXEC section has the same flags, but does not own
        # this callback. The original section still backs the address and bytes.
        elf.data += bytes(32)
        elf.sections.append((0,1,6,base+0x100,128,32,0,0,2,0))
        elf.functions[VALIDATION]=(base+97,8,2)
        with self.assertRaises(AssertionError):
            config_stack.DiagnosticObjects(elf,sizes).debug_vtable(base,'validation_error')


    def test_payload_vtable_keeps_reference_type_origin_after_relocation(self):
        for shift in (0,0x1400):
            elf,base,sizes=self.fixture(shift);data=bytearray(elf.data)
            struct.pack_into('<4I',data,0,0,4,4,base+97);elf.data=bytes(data)
            elf.functions={CONTROL:(base+97,8,1)}
            objects=config_stack.DiagnosticObjects(elf,sizes)
            self.assertEqual(objects.debug_vtable(base,'control_error_ref'),base+96)
            elf.functions={U8:(base+97,8,1)}
            with self.assertRaises(AssertionError):objects.debug_vtable(base,'control_error_ref')
            self.assertEqual(objects.debug_vtable(base,'u8_ref'),base+96)

    def test_reference_and_value_payload_layouts_cannot_substitute(self):
        elf,base,sizes=self.fixture();data=bytearray(elf.data)
        field='_RNvXsd_NtCs123_16dashboard_config8validateNtB5_5FieldNtNtCs456_4core3fmt5Debug3fmt'
        elf.functions={field:(base+97,8,1)}
        struct.pack_into('<4I',data,0,0,3,1,base+97);elf.data=bytes(data)
        objects=config_stack.DiagnosticObjects(elf,sizes)
        self.assertEqual(objects.debug_vtable(base,'field'),base+96)
        struct.pack_into('<I',data,4,4);elf.data=bytes(data)
        with self.assertRaises(AssertionError):objects.debug_vtable(base,'field')


class PanicSymbolObjects(unittest.TestCase):
    def fixture(self):
        guard='_RNvNvNtCs123_11panic_probe3imp5panic8PANICKED.0'
        identifier='{"package":"panic-probe","tag":"defmt_error","data":"{}","disambiguator":"3281125884690968605","crate_name":"panic_probe"}'
        bss=(0,8,3,0x20000000,0,32,0,0,1,0)
        metadata=(0,1,0x200000,0,0,32,0,0,1,0)
        elf=SimpleNamespace(data=bytes(32),sections=[bss,metadata],named={'.defmt':metadata},
            symbol_entries=[(guard,0x20000007,1,1,0),(identifier,14,1,17,1)])
        return config_stack.DiagnosticObjects(elf,{}),guard,identifier

    def test_guard_and_interned_symbol_ids_have_distinct_typed_storage(self):
        objects,_,_=self.fixture()
        self.assertEqual(objects.panic_guard(0x20000007),0x20000007)
        self.assertEqual(objects.defmt_id(14,'header_id'),14)

    def test_defmt_pre_end_object_ids_reject_reserved_domain_with_coherent_metadata(self):
        for role in ('header_id','display_encoding_id','usize_encoding_id'):
            for value in (0,65533,65534,65535):
                objects,_,_=self.fixture()
                name=json.dumps(config_stack.DEFMT_IDENTITIES[role])
                metadata=(0,1,0x200000,0,0,65536,0,0,1,0)
                objects.elf.sections[1]=metadata;objects.elf.named['.defmt']=metadata
                objects.elf.data=bytes(65536)
                objects.elf.symbol_entries[1]=(name,value,1,17,1)
                with self.subTest(role=role,value=value),self.assertRaises(AssertionError):
                    objects.defmt_id(value,role)

    def test_defmt_pre_end_object_id_boundaries_accept(self):
        for role in ('header_id','display_encoding_id','usize_encoding_id'):
            for value in (1,65532):
                objects,_,_=self.fixture()
                name=json.dumps(config_stack.DEFMT_IDENTITIES[role])
                metadata=(0,1,0x200000,0,0,65533,0,0,1,0)
                objects.elf.sections[1]=metadata;objects.elf.named['.defmt']=metadata
                objects.elf.data=bytes(65533)
                objects.elf.symbol_entries[1]=(name,value,1,17,1)
                with self.subTest(role=role,value=value):self.assertEqual(objects.defmt_id(value,role),value)

    def test_guard_size_type_section_identity_and_duplicate_reject(self):
        for mutation in ('size','type','binding','section','address','outside_section','readonly','name','duplicate','identical_duplicate'):
            objects,guard,_=self.fixture();entry=list(objects.elf.symbol_entries[0])
            if mutation=='size':entry[2]=2
            if mutation=='type':entry[3]=0
            if mutation=='binding':entry[3]=17
            if mutation=='section':entry[4]=1
            if mutation=='address':entry[1]=0x20000020
            if mutation=='name':entry[0]=guard.replace('PANICKED','OTHER')
            if mutation=='outside_section':
                section=list(objects.elf.sections[0]);section[5]=7;objects.elf.sections[0]=tuple(section)
            if mutation=='readonly':
                section=list(objects.elf.sections[0]);section[2]=2;objects.elf.sections[0]=tuple(section)
            objects.elf.symbol_entries[0]=tuple(entry)
            if mutation=='duplicate':objects.elf.symbol_entries.append((guard,0x20000008,1,1,0))
            if mutation=='identical_duplicate':objects.elf.symbol_entries.append(tuple(entry))
            with self.subTest(mutation=mutation),self.assertRaises(AssertionError):objects.panic_guard(0x20000007)

    def test_interned_id_size_type_section_value_json_and_duplicate_reject(self):
        for mutation in ('size','type','binding','section','allocated','truncated','value','json','disambiguator','duplicate','identical_duplicate'):
            objects,_,identifier=self.fixture();entry=list(objects.elf.symbol_entries[1])
            if mutation=='size':entry[2]=2
            if mutation=='type':entry[3]=0
            if mutation=='binding':entry[3]=1
            if mutation=='section':entry[4]=0
            if mutation=='value':entry[1]=15
            if mutation=='json':entry[0]=identifier.replace('defmt_error','defmt_info')
            if mutation=='disambiguator':entry[0]=identifier.replace('3281125884690968605','3281125884690968606')
            if mutation=='allocated':
                section=list(objects.elf.sections[1]);section[2]|=2
                objects.elf.sections[1]=tuple(section);objects.elf.named['.defmt']=tuple(section)
            if mutation=='truncated':objects.elf.data=b''
            objects.elf.symbol_entries[1]=tuple(entry)
            if mutation=='duplicate':objects.elf.symbol_entries.append((identifier,15,1,17,1))
            if mutation=='identical_duplicate':objects.elf.symbol_entries.append(tuple(entry))
            with self.subTest(mutation=mutation),self.assertRaises(AssertionError):objects.defmt_id(14,'header_id')



class BoundaryProducers(unittest.TestCase):
    fixture = TypedDiagnosticObjects.fixture

    def block(self):
        return [Instruction(0x10000060,b'\x00\x23','movs','r3, #0x0'),
                Instruction(0x10000062,b'\x04\x4a','ldr','r2, [pc, #0x10]'),
                Instruction(0x10000064,b'\x13\x46','mov','r3, r2'),
                Instruction(0x10000066,b'\x00\xbf','nop',''),
                Instruction(0x10000068,b'\x00\xf0\x0a\xf8','bl','0x10000080 <advisory>')]

    def test_current_pc_literal_or_movw_movt_identity_is_bound_to_consumer(self):
        elf,base,sizes=self.fixture();data=bytearray(elf.data)
        struct.pack_into('<I',data,0x74,base);elf.data=bytes(data)
        value,sites=config_stack.constant_before(self.block(),0x10000068,'r3',config_stack.DiagnosticObjects(elf,sizes))
        self.assertEqual(value,base);self.assertEqual(sites,(0x10000064,0x10000062))
        block=[Instruction(0x10000060,bytes(4),'movw','r3, #0x1234'),
               Instruction(0x10000064,bytes(4),'movt','r3, #0x1003'),
               Instruction(0x10000068,b'\x00\xf0\x0a\xf8','bl','0x10000080')]
        value,sites=config_stack.constant_before(block,0x10000068,'r3',config_stack.DiagnosticObjects(elf,sizes))
        self.assertEqual((value,sites),(0x10031234,(0x10000064,0x10000060)))

    def test_intervening_overwrite_branch_unknown_definition_and_unused_literal_fail_binding(self):
        for mnemonic,operands in [('movs','r3, #0x0'),('eors','r3, r0'),('b','0x10000068'),
                                  ('ldr','r3, [r1, #0xc]'),('mov','r3, r1'),('svc','#0x0'),('trap','')]:
            elf,base,sizes=self.fixture();data=bytearray(elf.data)
            struct.pack_into('<I',data,0x74,base);elf.data=bytes(data)
            block=self.block();block[3]=Instruction(0x10000066,b'\x00\x00',mnemonic,operands)
            with self.subTest(mnemonic=mnemonic,operands=operands),self.assertRaises(AssertionError):
                value,_=config_stack.constant_before(block,0x10000068,'r3',config_stack.DiagnosticObjects(elf,sizes))
                config_stack.DiagnosticObjects(elf,sizes).debug_vtable(value,'validation_error')

    def test_branch_ingress_cannot_bypass_the_selected_producer(self):
        elf,base,sizes=self.fixture();data=bytearray(elf.data)
        struct.pack_into('<I',data,0x74,base);elf.data=bytes(data)
        block=[Instruction(0x10000050,b'\x0a\xe0','b','0x10000068'),*self.block()]
        with self.assertRaises(AssertionError):
            value,_=config_stack.constant_before(block,0x10000068,'r3',config_stack.DiagnosticObjects(elf,sizes))
            config_stack.DiagnosticObjects(elf,sizes).debug_vtable(value,'validation_error')

    def test_postindexed_store_base_writeback_clobbers_current_argument(self):
        for mnemonic in ('str','strd'):
            elf,base,sizes=self.fixture();data=bytearray(elf.data)
            struct.pack_into('<I',data,0x74,base);elf.data=bytes(data)
            block=self.block()[:-2]
            operand='r0, [r3], #4' if mnemonic=='str' else 'r0, r1, [r3], #8'
            block += [Instruction(0x10000066,bytes(4),mnemonic,operand),
                      Instruction(0x1000006a,b'\x00\xf0\x00\xf8','bl','0x10000080')]
            with self.subTest(mnemonic=mnemonic),self.assertRaises(AssertionError):
                value,_=config_stack.constant_before(block,0x1000006a,'r3',config_stack.DiagnosticObjects(elf,sizes))
                config_stack.DiagnosticObjects(elf,sizes).debug_vtable(value,'validation_error')

    def test_implicit_pc_ingress_outside_producer_slice_rejects(self):
        for mnemonic,operands in [('mov','pc, r4'),('ldr','pc, [r4]'),
                                  ('ldm','r4!, {r0, pc}')]:
            elf,base,sizes=self.fixture();data=bytearray(elf.data)
            struct.pack_into('<I',data,0x74,base);elf.data=bytes(data)
            block=[Instruction(0x10000050,bytes(4),mnemonic,operands),*self.block()]
            with self.subTest(mnemonic=mnemonic),self.assertRaises(AssertionError):
                value,_=config_stack.constant_before(block,0x10000068,'r3',config_stack.DiagnosticObjects(elf,sizes))
                config_stack.DiagnosticObjects(elf,sizes).debug_vtable(value,'validation_error')

    def test_caller_abi_return_forms_do_not_create_ingress(self):
        for mnemonic,operands in [('bx','lr'),('pop','{r4, pc}'),('ldmia','sp!, {r4, pc}')]:
            elf,base,sizes=self.fixture();data=bytearray(elf.data)
            struct.pack_into('<I',data,0x74,base);elf.data=bytes(data)
            block=[*self.block(),Instruction(0x1000006c,bytes(4),mnemonic,operands)]
            value,_=config_stack.constant_before(block,0x10000068,'r3',config_stack.DiagnosticObjects(elf,sizes))
            self.assertEqual(value,base)

    def test_pc_relative_tbb_interval_disjoint_from_producer_is_only_ingress_exclusion(self):
        elf,base,sizes=self.fixture();data=bytearray(elf.data)
        struct.pack_into('<I',data,0x74,base);elf.data=bytes(data)
        for address in (0x0ffffe00,0x10000200):
            block=[Instruction(address,bytes.fromhex('dfe800f0'),'tbb','[pc, r0]'),*self.block()]
            value,_=config_stack.constant_before(block,0x10000068,'r3',config_stack.DiagnosticObjects(elf,sizes))
            self.assertEqual(value,base)

    def test_overlapping_unbounded_or_wrapping_table_ingress_rejects(self):
        for address,mnemonic,operands,raw in [
            (0x10000000,'tbb','[pc, r0]',bytes.fromhex('dfe800f0')),
            (0xfffffe00,'tbb','[pc, r0]',bytes.fromhex('dfe800f0')),
            (0x0ffffe00,'tbh','[pc, r0, lsl #1]',bytes.fromhex('dfe810f0')),
            (0x0ffffe00,'tbb','[r4, r0]',bytes.fromhex('d4e800f0')),
            (0x0ffffe00,'tbb','[pc, r0]',bytes(2))]:
            elf,base,sizes=self.fixture();data=bytearray(elf.data)
            struct.pack_into('<I',data,0x74,base);elf.data=bytes(data)
            block=[Instruction(address,raw,mnemonic,operands),*self.block()]
            with self.subTest(address=address,mnemonic=mnemonic,operands=operands),self.assertRaises(AssertionError):
                config_stack.constant_before(block,0x10000068,'r3',config_stack.DiagnosticObjects(elf,sizes))




    def test_conditional_and_security_transfers_outside_slice_reject(self):
        # Spellings independently confirmed with selected LLVM on a decoded-only
        # IT/BX/BLX/BXNS/BLXNS/SVC fixture; these remain semantic-record tests.
        for mnemonic,operands,raw in [('bxeq','r4','2047'),('blxeq','r4','a047'),
                                      ('bxns','r4','2447'),('blxns','r4','a447'),
                                      ('svcne','#0x0','00df')]:
            elf,base,sizes=self.fixture();data=bytearray(elf.data)
            struct.pack_into('<I',data,0x74,base);elf.data=bytes(data)
            block=[Instruction(0x10000050,bytes.fromhex(raw),mnemonic,operands),*self.block()]
            with self.subTest(mnemonic=mnemonic),self.assertRaises(AssertionError):
                config_stack.constant_before(block,0x10000068,'r3',config_stack.DiagnosticObjects(elf,sizes))



class EnumDispatch(unittest.TestCase):
    def fixture(self, shift=0, control=False):
        base=0x10000000+shift
        name='_RNvXsd_NtCs123_16dashboard_config8validateNtB5_5FieldNtNtCs456_4core3fmt5Debug3fmt'
        prefix=[('80b5','push','{r7, lr}'),('6f46','mov','r7, sp'),('82b0','sub','sp, #0x8'),
                ('0278','ldrb','r2, [r0]'),('083a','subs','r2, #0x8'),('98bf','it','ls'),
                ('0022','movls','r2, #0x0'),('dfe812f0','tbh','[pc, r2, lsl #1]')]
        if control:
            name=CONTROL
            prefix=[('f0b5','push','{r4, r5, r6, r7, lr}'),('03af','add','r7, sp, #0xc'),
                    ('4df804bd','str','r11, [sp, #-4]!'),('8ab0','sub','sp, #0x28'),
                    ('0268','ldr','r2, [r0]'),('1078','ldrb','r0, [r2]'),('0238','subs','r0, #0x2'),
                    ('38bf','it','lo'),('0220','movlo','r0, #0x2'),('c0b2','uxtb','r0, r0'),
                    ('dfe800f0','tbb','[pc, r0]')]
        code=b''.join(bytes.fromhex(r) for r,_,_ in prefix)
        count=7 if control else 58
        table=bytes(4+i for i in range(7))+b'\0' if control else b''.join(struct.pack('<H',58+i) for i in range(58))
        tail=len(code)+len(table)
        data=code+table+bytes.fromhex('00bf')*count
        records=[];address=base
        for raw,mnemonic,operands in prefix:
            raw=bytes.fromhex(raw);records.append(Instruction(address,raw,mnemonic,operands));address+=len(raw)
        records += [Instruction(base+tail+2*i,bytes.fromhex('00bf'),'nop','') for i in range(count)]
        elf=SimpleNamespace(data=data,sections=[(0,1,6,base,0,len(data),0,0,2,0)],
                            functions={name:(base|1,len(data),0)},
                            mapping_symbols=[('$t',base,0),('$d',base+len(code),0),('$t',base+tail,0)])
        # The complete byte-checked producer body, not an arbitrary table blob.
        elf.function_bytes=lambda selected: elf.data
        return elf,base,name,records

    def test_typed_field_dispatch_relocates_all_58_current_entries(self):
        for shift in (0,0x2000):
            elf,base,name,records=self.fixture(shift)
            dispatch=config_stack.enum_dispatch(config_stack.DiagnosticObjects(elf,{base:16}),name,records)
            self.assertEqual(dispatch['role'],'field')
            self.assertEqual(dispatch['entries'],tuple(base+134+2*i for i in range(58)))

    def test_control_dispatch_has_seven_cases_and_no_eighth_padding_case(self):
        elf,base,name,records=self.fixture(control=True)
        dispatch=config_stack.enum_dispatch(config_stack.DiagnosticObjects(elf,{base:64}),name,records)
        self.assertEqual(dispatch['entries'],tuple(base+34+2*i for i in range(7)))
        data=bytearray(elf.data);data[33]=1;elf.data=bytes(data)
        with self.assertRaises(AssertionError):
            config_stack.enum_dispatch(config_stack.DiagnosticObjects(elf,{base:64}),name,records)

    def test_valid_dispatch_alone_cannot_admit_unproved_case_bodies(self):
        elf,base,name,records=self.fixture()
        with self.assertRaises(AssertionError):
            config_stack.field_case_bindings(config_stack.DiagnosticObjects(elf,{base:16}),name,records)

    def test_wrong_tag_transform_unbacked_data_and_nonentry_destinations_reject(self):
        for fault in ('transform','data_target','outside','missing_record','wrong_role','writable'):
            elf,base,name,records=self.fixture();data=bytearray(elf.data)
            if fault=='transform':data[8]^=1
            elif fault=='data_target':struct.pack_into('<H',data,18,1)
            elif fault=='outside':struct.pack_into('<H',data,18,0xffff)
            elif fault=='missing_record':records.pop()
            elif fault=='wrong_role':
                other=name.replace('5Field','9ErrorCode');elf.functions={other:elf.functions[name]};name=other
            elif fault=='writable':elf.sections[0]=(0,1,7,base,0,len(data),0,0,2,0)
            elf.data=bytes(data)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_stack.enum_dispatch(config_stack.DiagnosticObjects(elf,{base:16}),name,records)

class FieldSuffix(unittest.TestCase):
    def fixture(self):
        elf,base,sizes=TypedDiagnosticObjects.fixture(self)
        data=bytearray(elf.data);data[32:40]=b'Controls'
        struct.pack_into('<4I',data,0,0,4,4,base+97)
        struct.pack_into('<4I',data,16,0,4,4,base+105)
        elf.data=bytes(data)
        tuple_role='_RNvMsa_NtCs123_4core3fmtNtB5_9Formatter25debug_tuple_field1_finish'
        elf.functions={CONTROL:(base+97,8,1),U8:(base+105,8,1),tuple_role:(base+113,8,1)}
        sizes={base+96:64,base+104:24,base+112:64}
        specs=[('movw','r12, #0x0'),('movt','r12, #0x1000'),('str','r0, [sp, #0x4]'),
               ('add','r3, sp, #0x4'),('mov','r0, r1'),('movw','r1, #0x20'),
               ('movt','r1, #0x1000'),('movs','r2, #0x8'),('str','r12, [sp]'),
               ('bl','0x10000070'),('add','sp, #0x8'),('pop','{r7, pc}')]
        records={base+0x200+4*i:Instruction(base+0x200+4*i,bytes(4),m,o) for i,(m,o) in enumerate(specs)}
        return config_stack.DiagnosticObjects(elf,sizes),base,records

    def test_current_typed_controls_predecessor_binds_its_own_reference_and_vtable(self):
        objects,base,records=self.fixture()
        binding,visited=config_stack._field_case(objects,records,base+0x200,'Controls')
        self.assertEqual((binding['role'],binding['callback'],binding['bridge']),
                         ('control_error_ref',base+96,base+112))
        self.assertEqual(visited,set(records))

    def test_shared_suffix_cannot_borrow_valid_u8_object_or_skip_its_reference_producers(self):
        for fault in ('wrong_type','ingress','writeback','wrong_field_offset','overwrite'):
            objects,base,records=self.fixture();entry=base+0x200
            if fault=='wrong_type':records[entry]=Instruction(entry,bytes(4),'movw','r12, #0x10')
            elif fault=='ingress':entry+=8*4
            elif fault=='writeback':records[entry+8]=Instruction(entry+8,bytes(4),'str','r0, [sp], #4')
            elif fault=='wrong_field_offset':
                specs=[(i.mnemonic,i.operands) for i in records.values()]
                specs.insert(2,('adds','r0, #0x1'))
                records={entry+4*i:Instruction(entry+4*i,bytes(4),m,o) for i,(m,o) in enumerate(specs)}
            else:records[entry+7*4]=Instruction(entry+7*4,bytes(4),'movs','r12, #0x0')
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_stack._field_case(objects,records,entry,'Controls')

    def test_unit_suffix_retains_live_sink_origin_and_exact_name(self):
        objects,base,_=self.fixture();data=bytearray(objects.elf.data);data[32:39]=b'Version';objects.elf.data=bytes(data)
        specs=[('ldrd','r0, r1, [r1]'),('ldr','r3, [r1, #0xc]'),('movw','r1, #0x20'),
               ('movt','r1, #0x1000'),('movs','r2, #0x7'),('add','sp, #0x8'),
               ('pop','{r7, lr}'),('bx','r3')]
        records={base+0x200+4*i:Instruction(base+0x200+4*i,bytes(4),m,o) for i,(m,o) in enumerate(specs)}
        binding,_=config_stack._field_case(objects,records,base+0x200,'Version')
        self.assertEqual(binding['role'],'live_write_str')
        for fault in ('slot','origin','name','unbalanced','unknown'):
            changed=dict(records)
            if fault=='slot':changed[base+0x204]=Instruction(base+0x204,bytes(4),'ldr','r3, [r1, #0x10]')
            elif fault=='origin':changed[base+0x200]=Instruction(base+0x200,bytes(4),'ldrd','r0, r1, [r0]')
            elif fault=='name':changed[base+0x210]=Instruction(base+0x210,bytes(4),'movs','r2, #0x8')
            elif fault=='unbalanced':changed[base+0x214]=Instruction(base+0x214,bytes(4),'add','sp, #0x4')
            else:changed[base+0x210]=Instruction(base+0x210,bytes(4),'eors','r3, r0')
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_stack._field_case(objects,changed,base+0x200,'Version')

    def test_materialization_cannot_ignore_implicit_pc_or_stack_pointer_writes(self):
        for mnemonic,operands in [('mov','pc, r1'),('mov','sp, r1'),('add','pc, r3, #0x0')]:
            objects,base,records=self.fixture();entry=base+0x200
            specs=[(i.mnemonic,i.operands) for i in records.values()]
            specs.insert(8,(mnemonic,operands))
            records={entry+4*i:Instruction(entry+4*i,bytes(4),m,o) for i,(m,o) in enumerate(specs)}
            with self.subTest(mnemonic=mnemonic,operands=operands),self.assertRaises(AssertionError):
                config_stack._field_case(objects,records,entry,'Controls')

    def test_tuple_bridge_identity_includes_code_ownership_and_consistent_thumb_aliases(self):
        for fault in ('data_section','arm','alias'):
            objects,base,records=self.fixture()
            name=next(n for n in objects.elf.functions if 'debug_tuple_field1_finish' in n)
            value,extent,index=objects.elf.functions[name]
            if fault=='data_section':objects.elf.functions[name]=(value,extent,0)
            elif fault=='arm':objects.elf.functions[name]=(value&~1,extent,index)
            else:objects.elf.functions[name+'.1']=(value,extent+2,index)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_stack._field_case(objects,records,base+0x200,'Controls')

    def outline_fixture(self):
        objects,base,_=self.fixture();elf=objects.elf
        elf.functions['OUTLINED_FUNCTION_123']=(base+121,8,1);objects.sizes[base+120]=0
        data=bytearray(elf.data);data[120:128]=bytes.fromhex('0090 0846 1146 7047');elf.data=bytes(data)
        elf.mapping_symbols=[('$t',base+96,1)]
        specs=[('str','r0, [sp, #0x4]'),('movw','r0, #0x0'),('movt','r0, #0x1000'),
               ('movw','r2, #0x20'),('movt','r2, #0x1000'),('add','r3, sp, #0x4'),
               ('bl','0x10000078'),('movs','r2, #0x8'),('bl','0x10000070'),
               ('add','sp, #0x8'),('pop','{r7, pc}')]
        records={base+0x200+4*i:Instruction(base+0x200+4*i,bytes(4),m,o) for i,(m,o) in enumerate(specs)}
        outline=[Instruction(base+120,bytes.fromhex('0090'),'str','r0, [sp]'),
                 Instruction(base+122,bytes.fromhex('0846'),'mov','r0, r1'),
                 Instruction(base+124,bytes.fromhex('1146'),'mov','r1, r2'),
                 Instruction(base+126,bytes.fromhex('7047'),'bx','lr')]
        return objects,base,records,{base+120:outline}

    def test_changed_incoming_sp_cannot_reuse_old_tuple_argument_cells(self):
        objects,base,records=self.fixture();entry=base+0x200
        specs=[(i.mnemonic,i.operands) for i in records.values()]
        specs[9],specs[10]=specs[10],specs[9]
        records={entry+4*i:Instruction(entry+4*i,bytes(4),m,o) for i,(m,o) in enumerate(specs)}
        with self.assertRaises(AssertionError):config_stack._field_case(objects,records,entry,'Controls')

    def test_popped_register_cannot_retain_live_sink_callback_identity(self):
        objects,base,_=self.fixture();data=bytearray(objects.elf.data);data[32:39]=b'Version';objects.elf.data=bytes(data)
        specs=[('ldrd','r0, r1, [r1]'),('ldr','r3, [r1, #0xc]'),('movw','r1, #0x20'),
               ('movt','r1, #0x1000'),('movs','r2, #0x7'),('mov','r7, r3'),
               ('add','sp, #0x8'),('pop','{r7, lr}'),('bx','r7')]
        records={base+0x200+4*i:Instruction(base+0x200+4*i,bytes(4),m,o) for i,(m,o) in enumerate(specs)}
        with self.assertRaises(AssertionError):config_stack._field_case(objects,records,base+0x200,'Version')

    def test_zero_frame_outline_transports_current_vtable_without_ordinal_authority(self):
        objects,base,records,outlines=self.outline_fixture()
        binding,_=config_stack._field_case(objects,records,base+0x200,'Controls',outlines)
        self.assertEqual(binding['callback'],base+96)
        objects.elf.functions['OUTLINED_FUNCTION_987']=objects.elf.functions.pop('OUTLINED_FUNCTION_123')
        binding,_=config_stack._field_case(objects,records,base+0x200,'Controls',outlines)
        self.assertEqual(binding['callback'],base+96)

    def test_sizez_scratch_pop_returns_without_restoring_stale_argument_registers(self):
        objects,base,records,outlines=self.outline_fixture();entry=base+0x200
        specs=[(i.mnemonic,i.operands) for i in records.values()]
        specs[-2:]=[('pop','{r2, r3, r7, pc}')]
        records={entry+4*i:Instruction(entry+4*i,bytes(4),m,o) for i,(m,o) in enumerate(specs)}
        binding,_=config_stack._field_case(objects,records,entry,'Controls',outlines,epilogue='scratch')
        self.assertEqual(binding['callback'],base+96)

    def test_outline_cannot_use_argument_cells_after_call_phase_sp_moves(self):
        objects,base,records,outlines=self.outline_fixture();entry=base+0x200
        specs=[(i.mnemonic,i.operands) for i in records.values()]
        specs.insert(6,('add','sp, #0x8'))
        specs.pop(-2)
        records={entry+4*i:Instruction(entry+4*i,bytes(4),m,o) for i,(m,o) in enumerate(specs)}
        with self.assertRaises(AssertionError):
            config_stack._field_case(objects,records,entry,'Controls',outlines)

    def sink_outline_fixture(self):
        objects,base,_,outlines=self.outline_fixture();data=bytearray(objects.elf.data)
        data[32:39]=b'Version';data[120:128]=bytes.fromhex('d1e90001 cb68 7047');objects.elf.data=bytes(data)
        outlines[base+120]=[Instruction(base+120,bytes.fromhex('d1e90001'),'ldrd','r0, r1, [r1]'),
                            Instruction(base+124,bytes.fromhex('cb68'),'ldr','r3, [r1, #0xc]'),
                            Instruction(base+126,bytes.fromhex('7047'),'bx','lr')]
        specs=[('mov','r2, r1'),('ldrd','r0, r1, [r1]'),('ldr','lr, [r1, #0xc]'),
               ('mov','r1, r2'),('bl','0x10000078'),('movw','r1, #0x20'),
               ('movt','r1, #0x1000'),('movs','r2, #0x7')]
        return objects,base,specs,outlines

    def test_sink_tail_requires_observed_restore_instead_of_only_balanced_sp(self):
        for tail in ('r3','lr'):
            objects,base,specs,outlines=self.sink_outline_fixture();specs += [('add','sp, #0x10'),('bx',tail)]
            records={base+0x200+4*i:Instruction(base+0x200+4*i,bytes(4),m,o) for i,(m,o) in enumerate(specs)}
            with self.subTest(tail=tail),self.assertRaises(AssertionError):
                config_stack._field_case(objects,records,base+0x200,'Version',outlines)

    def test_real_outline_bl_overwrites_lr_even_when_later_epilogue_restores_it(self):
        objects,base,specs,outlines=self.sink_outline_fixture()
        specs.insert(5,('mov','r3, lr'))
        specs += [('add','sp, #0x8'),('pop','{r7, lr}'),('bx','r3')]
        records={base+0x200+4*i:Instruction(base+0x200+4*i,bytes(4),m,o) for i,(m,o) in enumerate(specs)}
        with self.assertRaises(AssertionError):
            config_stack._field_case(objects,records,base+0x200,'Version',outlines)

    def test_outline_nonzero_frame_wrong_slot_missing_bytes_and_unknown_body_reject(self):
        for fault in ('frame','slot','missing','body'):
            objects,base,records,outlines=self.outline_fixture()
            if fault=='frame':objects.sizes[base+120]=8
            elif fault=='missing':outlines[base+120].pop()
            else:
                data=bytearray(objects.elf.data)
                if fault=='slot':data[120:122]=bytes.fromhex('0190');outlines[base+120][0]=Instruction(base+120,bytes.fromhex('0190'),'str','r0, [sp, #4]')
                else:data[120:122]=bytes.fromhex('10b4');outlines[base+120][0]=Instruction(base+120,bytes.fromhex('10b4'),'push','{r4}')
                objects.elf.data=bytes(data)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                config_stack._field_case(objects,records,base+0x200,'Controls',outlines)




class LoaderContinuation(unittest.TestCase):
    def fixture(self,code_shift=0,data_shift=0):
        objects,records,root,frame=DefaultSourceTerminal.fixture(self,code_shift,data_shift)
        item=json.loads((Path(__file__).parent/'fixtures/config-loader-continuation.json').read_text())
        data=bytearray(objects.elf.data)
        def index(address):
            s=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=address<s[3]+s[5])
            return s[4]+address-s[3]
        for ix,address,encoded in item['segments']:
            s=objects.elf.sections[ix];shift=0 if s[2]&1 else code_shift if s[2]&4 else data_shift
            raw=bytes.fromhex(encoded);at=index(address+shift);data[at:at+len(raw)]=raw
        for address,offset,kind in item['pointers']:
            ix=next(i for i,s in enumerate(objects.elf.sections) if s[2]&2 and
                s[3]-(0 if s[2]&1 else code_shift if s[2]&4 else data_shift)<=address<
                s[3]-(0 if s[2]&1 else code_shift if s[2]&4 else data_shift)+s[5])
            s=objects.elf.sections[ix];at=index(address+(0 if s[2]&1 else code_shift if s[2]&4 else data_shift))+offset
            value=struct.unpack_from('<I',data,at)[0]
            struct.pack_into('<I',data,at,value+(code_shift if kind=='code' else data_shift if kind=='data' else 0))
        objects.elf.data=bytes(data)
        objects.elf.functions.update({n:(v[0]+code_shift,v[1],v[2]) for n,v in item['functions'].items()})
        objects.sizes.update({int(a)+code_shift:v for a,v in item['sizes'].items()})
        for entry in item['symbol_entries']:
            n,a,z,info,ix=entry;s=objects.elf.sections[ix]
            row=(n,a+(0 if s[2]&1 else code_shift if s[2]&4 else data_shift),z,info,ix)
            if row not in objects.elf.symbol_entries:objects.elf.symbol_entries.append(row)
        for r in item['records']:
            at=r['address']+code_shift;op=r['text'].split(None,1)
            records[at]=Instruction(at,bytes.fromhex(r['bytes']),op[0],op[1] if len(op)>1 else '')
        return objects,records,root,frame

    def test_continuation_binds_distinct_result_epochs_and_paired_publication(self):
        import importlib.util
        self.assertIsNotNone(importlib.util.find_spec('config_loader_continuation'),'missing finite continuation consumer')
        import config_loader_continuation
        for code,data in ((0,0),(0x2800,0x19000)):
            objects,records,root,frame=self.fixture(code,data)
            result=config_loader_continuation.loader_continuation(objects,root,records,frame,())
            self.assertEqual(tuple((c['phase'],c['result'].offset,c['r8']) for c in result['validator_contexts']),
                (('fallback',236,frame.start+112),('decoded',112,result['config'].start)))
            self.assertEqual(result['valid_result_tags'],(0,1,2))
            self.assertEqual(result['excluded_non2_sentinel'],65535)
            self.assertEqual(tuple(p['fault'] for p in result['publication_predecessors']),('Some(original fault)','None'))
            self.assertEqual(result['return_interface']['restored_bytes'],456)
            self.assertEqual(result['return_interface']['r0'],'issued previous identity or zero; not Config pointer')
            self.assertTrue(result['requires_complete_validator_typed_fault_and_boot_graph'])
            self.assertEqual(len(result['target_access_requirements']),10)

    def test_continuation_operand_epochs_fault_reconstruction_and_return_are_closed(self):
        import config_loader_continuation
        for address,operand in ((0x100203e6,'r0, [sp, #0x70]'),(0x10020414,'r0, r1, #0x0'),
            (0x1002046c,'sp, #0x164'),(0x10020e34,'r0, sp, #0xec'),
            (0x10020e5c,'r0, [r10, r1]'),(0x10020e70,'r5, r1'),(0x1002846c,'{r8, r9, r10}')):
            objects,records,root,frame=self.fixture();old=records[address]
            records[address]=Instruction(address,old.raw,old.mnemonic,operand)
            with self.subTest(address=hex(address)),self.assertRaises(AssertionError):
                config_loader_continuation.loader_continuation(objects,root,records,frame,())

    def test_foreign_continuation_ingress_does_not_borrow_another_phase(self):
        import config_loader_continuation
        for address in (0x100203e6,0x100203f0,0x10020424,0x1002045c,0x10020e58):
            objects,records,root,frame=self.fixture()
            records[0x10060000]=Instruction(0x10060000,bytes(2),'b',hex(address))
            with self.subTest(address=hex(address)),self.assertRaises(AssertionError):
                config_loader_continuation.loader_continuation(objects,root,records,frame,())


    def test_issuer_expect_start_and_interior_require_actual_zero_predecessor(self):
        import config_loader_continuation
        for site in (0x10060000,0x1001feee):
            for destination in (0x100209c8,0x100209cc,0x100209d0,0x100209d2):
                objects,records,root,frame=self.fixture()
                if site in records:old=records[site];raw=old.raw
                else:raw=bytes(2)
                records[site]=Instruction(site,raw,'b',hex(destination))
                with self.subTest(source=hex(site),target=hex(destination)),self.assertRaises(AssertionError):
                    config_loader_continuation.loader_continuation(objects,root,records,frame,())



class ValidatorPlatformFamilies(unittest.TestCase):
    def fixture(self,role,mode):
        from config_validator_access import ValidatorContext,ValidatorFamily
        from config_regions import Storage,borrow
        item=next(v for v in json.loads((Path(__file__).parent/'fixtures/config-validator-accesses.json').read_text())['instances']
            if (v['role'],v['mode'])==(role,mode))
        child,caller,outputs={'speed2':(296,1288,(156,996)),'sizes':(288,696,(100,408)),'sizez':(320,456,(236,112))}[mode]
        contexts=[]
        for phase,offset in zip(('default_validation','decoded_validation'),outputs):
            parent=Storage('stack:configuration.default','loader',0,caller,8,phase,True)
            frame=Storage(parent.space,'validator',-child,child,8,phase,True)
            slot=Storage('address','private unpublished ConfigSlot',{'speed2':0x2000064c,'sizes':0x20000388,'sizez':0x2000031c}[mode],14604,4,phase,True)
            saves=Storage(frame.space,'validator:saved',frame.start+(200 if mode=='speed2' else 184 if mode=='sizes' else 216),
                child-(200 if mode=='speed2' else 184 if mode=='sizes' else 216),4,phase,False)
            contexts.append(ValidatorContext(mode,phase,parent,frame,borrow(slot,0,14576,phase),
                borrow(parent,offset,12,phase,writable=True),(saves,)))
        return ValidatorFamily(role,mode,{'speed2':0x10036830,'sizes':0x100277dc,'sizez':0x1001ea20}[mode],tuple((v['site'],v['encoding'],v['offset'],v['outlined']) for v in item['sites']),tuple(contexts))

    def test_v4_all25_roles_keep_both_epochs_and_strict_roundtrip(self):
        import importlib.util
        self.assertIsNotNone(importlib.util.find_spec('config_validator_access'),'missing closed validator dependency families')
        from config_validator_access import read_validator_family
        items=json.loads((Path(__file__).parent/'fixtures/config-validator-accesses.json').read_text())['instances']
        self.assertEqual(sum(len(i['sites']) for i in items),25)
        for item in items:
            value=self.fixture(item['role'],item['mode']);record=value.to_record()
            self.assertEqual(read_validator_family(json.loads(json.dumps(record))),value)
            self.assertEqual(tuple(c.phase for c in value.contexts),('default_validation','decoded_validation'))
            self.assertEqual(len(record['contexts']),2)
            self.assertNotIn('runtime_sp',record)

    def test_page_and_widget_footprints_stay_paired_without_instance_expansion(self):
        for mode in ('speed2','sizes','sizez'):
            pages=self.fixture('control_buttons',mode)
            base={'speed2':0x2000064c,'sizes':0x20000388,'sizez':0x2000031c}[mode]
            for i in range(3):
                expected=((base+213+10*i,base+217+10*i),(base+217+10*i,base+221+10*i))
                self.assertEqual(tuple(sorted(pages.footprints('default_validation',i,3))),expected)
            widgets=self.fixture('widget_placement',mode)
            self.assertEqual(widgets.footprints('decoded_validation',63,64),((base+14519,base+14521),))
            self.assertLess(len(json.dumps(widgets.to_record())),10000)
            for index,count in ((0,0),(64,64),(0,65),(-1,64),(True,64)):
                with self.subTest(mode=mode,index=index,count=count),self.assertRaises(AssertionError):
                    widgets.footprints('decoded_validation',index,count)

    def test_validator_reader_rejects_role_width_numeric_alias_epoch_and_owner_substitution(self):
        from config_validator_access import read_validator_family
        from copy import deepcopy
        record=self.fixture('y_action_pairs','speed2').to_record()
        for fault in ('role','mode','width_float','direction','epoch','first_for_result','frame','protected','cross_config','cross_caller','extra'):
            bad=deepcopy(record)
            if fault=='role':bad['role']='arbitrary_halfword'
            elif fault=='mode':bad['mode']='sizez'
            elif fault=='width_float':bad['width']=2.0
            elif fault=='direction':bad['direction']='read'
            elif fault=='epoch':bad['contexts'][1]=deepcopy(bad['contexts'][0])
            elif fault=='first_for_result':bad['contexts'][0]['result']['storage']=deepcopy(bad['contexts'][0]['frame'])
            elif fault=='frame':bad['contexts'][0]['frame']['extent']=288
            elif fault=='protected':bad['contexts'][0]['protected']=[]
            elif fault=='cross_config':bad['contexts'][1]['config']['storage']['start']+=4
            elif fault=='cross_caller':
                c=bad['contexts'][1]
                for v in (c['caller'],c['frame'],c['result']['storage'],*c['protected']):v['start']+=8
            else:bad['approved']=True
            with self.subTest(fault=fault),self.assertRaises(AssertionError):read_validator_family(bad)


    def dependencies(self,mode,include_led=True):
        roles={'speed2':('first_reads','result_writes','control_buttons','first_halfwords','y_action_pairs','widget_placement'),
            'sizes':('first_word','control_buttons','widget_placement'),'sizez':('first_word','control_buttons','widget_placement')}
        values=tuple(self.fixture(role,mode) for role in roles[mode])
        led=LedPlatformFamilies.dependencies(LedPlatformFamilies(),mode) if include_led else ()
        if mode!='sizez':return values+led
        from config_owned_access import OwnedAccess
        from config_regions import borrow
        config=OwnedPlatformDependencies.fixture(self,'configuration_versions')
        advanced=OwnedPlatformDependencies.fixture(self,'advanced_default_fields')
        return (config,advanced)+tuple(OwnedAccess('widget_colors',0x10022000,0x10022000+670,
            'widget'+str(i),borrow(config.owner.storage,444+220*i,220,config.frame.phase,writable=True),
            config.frame,'r4',config.protected,i) for i in range(8))+values+led

    def artifact(self,mode):
        # Deliberately metadata/site-only synthetic ELF; unfilled body bytes
        # grant no validator grammar, source, ownership or graph acceptance.
        from config_entry import CONFIG
        from config_validator_access import LAYOUTS
        parent={'speed2':0x10036830,'sizes':0x100277dc,'sizez':0x1001ea20}[mode]
        base={'speed2':0x2000064c,'sizes':0x20000388,'sizez':0x2000031c}[mode]
        items=[v for v in json.loads((Path(__file__).parent/'fixtures/config-validator-accesses.json').read_text())['instances'] if v['mode']==mode]
        end=max([parent+LAYOUTS[mode][6]]+[v['site']+8 for r in items for v in r['sites']]);raw=bytearray(end-parent)
        for r in items:
            for v in r['sites']:raw[v['site']-parent:v['site']-parent+4]=bytes.fromhex(v['encoding'])
        functions={'_RNvNtCs_16dashboard_config8validate12validate_ref':(parent|1,LAYOUTS[mode][6],0)}
        sizes={parent:LAYOUTS[mode][0]}
        if mode=='sizez':
            outline=0x10029084;raw[outline-parent:outline-parent+10]=bytes.fromhex('8df85600cdf852107047')
            functions['OUTLINED_FUNCTION_310']=(outline|1,10,0);sizes[outline]=0
        text=(0,1,6,parent,0,len(raw),0,0,2,0);data=(0,1,7,base,len(raw),14604,0,0,4,0)
        elf=SimpleNamespace(data=bytes(raw)+bytes(14604),sections=[text,data],functions=functions,
            symbol_entries=[(CONFIG,base,14604,1,1)],named={'.data':data},mapping_symbols=[('$t',parent,0)])
        # Merge the new LED metadata/site fixture into this same synthetic
        # text allocation; no complete source/LED/validator body is implied.
        led=LedPlatformFamilies.artifact(LedPlatformFamilies(),mode)
        led_text=led.elf.sections[0];start=min(parent,led_text[3]);finish=max(parent+len(raw),led_text[3]+led_text[5])
        merged=bytearray(finish-start);merged[parent-start:parent-start+len(raw)]=raw
        for family in LedPlatformFamilies.dependencies(LedPlatformFamilies(),mode):
            for at,encoded,offset,epoch in family.sites:merged[at-start:at-start+4]=bytes.fromhex(encoded)
        text=(0,1,6,start,0,len(merged),0,0,2,0);data=(0,1,7,base,len(merged),14604,0,0,4,0)
        elf.data=bytes(merged)+bytes(14604);elf.sections=[text,data];elf.named={'.data':data}
        elf.functions.update(led.elf.functions);elf.mapping_symbols=[('$t',start,0)];sizes.update(led.sizes)
        return config_stack.DiagnosticObjects(elf,sizes)

    def test_v5_policy_retains_exact_closed_family_sets_and_rejects_stale_or_lost_qualifiers(self):
        import config_platform
        self.assertEqual((config_platform.POLICY_SCHEMA,config_platform.SCOPE,config_platform.SCOPE_SHA256),
            (4,'h2-rp2350-secure-owned-access-v5','06a1ff14d9a17278c60e9ba5fd594c5e3195b904a3a0fdbce5d88f4d99e4871f'))
        from copy import deepcopy
        objects,view,context=ConditionalStackPolicy.fixture(self)
        for mode,count in (('speed2',7),('sizes',4),('sizez',16)):
            dependencies=self.dependencies(mode)
            policy=config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=(),owned_dependencies=dependencies)
            self.assertEqual(policy.owned_family,mode+'_configuration')
            self.assertEqual(len(policy.owned_dependencies),count)
            self.assertEqual(config_platform.read_policy(json.loads(json.dumps(policy.to_record())),policy),policy)
            for fault in ('old','lost','duplicate','injected','hardware','width_float'):
                bad=deepcopy(policy.to_record())
                if fault=='old':bad.update(schema=3,scope='h2-rp2350-secure-owned-access-v4')
                elif fault=='lost':bad['owned_dependencies'].pop()
                elif fault=='duplicate':bad['owned_dependencies'][-1]=deepcopy(bad['owned_dependencies'][-2])
                elif fault=='injected':bad['owned_dependencies'].append(deepcopy(bad['owned_dependencies'][-1]))
                elif fault=='hardware':bad['hardware_qualified']=True
                else:bad['owned_dependencies'][-1]['width']=2.0
                with self.subTest(mode=mode,fault=fault),self.assertRaises(AssertionError):config_platform.read_policy(bad,policy)

    def test_current_artifact_rebinds_all25_sites_and_exact_zero_growth_outline(self):
        import config_validator_access
        self.assertTrue(callable(getattr(config_validator_access,'current_validator_family',None)),
            'missing current-artifact validator family consumer')
        for mode in ('speed2','sizes','sizez'):
            objects=self.artifact(mode)
            for value in self.dependencies(mode):
                if not isinstance(value,config_validator_access.ValidatorFamily):continue
                self.assertEqual(config_validator_access.current_validator_family(objects,value),value)
                for site,raw,offset,outline in value.sites:
                    old=objects.elf.data;bad=bytearray(old);bad[site-objects.elf.sections[0][3]]^=1;objects.elf.data=bytes(bad)
                    with self.subTest(mode=mode,role=value.role,site=hex(site)),self.assertRaises(AssertionError):
                        config_validator_access.current_validator_family(objects,value)
                    objects.elf.data=old
            if mode=='sizez':
                value=self.fixture('first_word',mode);old=objects.elf.data;bad=bytearray(old);bad[0x10029084-objects.elf.sections[0][3]]^=1;objects.elf.data=bytes(bad)
                with self.assertRaises(AssertionError):config_validator_access.current_validator_family(objects,value)

    def test_validator_sites_relocate_with_parent_metadata_and_mapping(self):
        from config_validator_access import current_validator_family,ValidatorFamily
        from dataclasses import replace
        for mode in ('speed2','sizes','sizez'):
            objects=self.artifact(mode);delta=0x1000
            text=list(objects.elf.sections[0]);text[3]+=delta;objects.elf.sections[0]=tuple(text)
            objects.elf.functions={n:(a+delta,size,section) for n,(a,size,section) in objects.elf.functions.items()}
            objects.elf.mapping_symbols=[(n,a+delta,section) for n,a,section in objects.elf.mapping_symbols]
            objects.sizes={a+delta:size for a,size in objects.sizes.items()}
            for old in self.dependencies(mode):
                if not isinstance(old,ValidatorFamily):continue
                value=replace(old,parent=old.parent+delta,sites=tuple((a+delta,raw,o,x) for a,raw,o,x in old.sites))
                self.assertEqual(current_validator_family(objects,value),value)
                with self.assertRaises(AssertionError):current_validator_family(objects,old)

    def test_validator_current_metadata_and_typed_record_cannot_be_substituted(self):
        from config_validator_access import current_validator_family,read_validator_family
        from copy import deepcopy
        for fault in ('frame','extent','data_mapping','config_extent','outline_frame','outline_extent'):
            objects=self.artifact('sizez');value=self.fixture('first_word','sizez')
            if fault=='frame':objects.sizes[value.parent]=288
            elif fault=='extent':
                name=next(n for n in objects.elf.functions if 'validate_ref' in n)
                a,size,section=objects.elf.functions[name];objects.elf.functions[name]=(a,size-2,section)
            elif fault=='data_mapping':objects.elf.mapping_symbols=[('$d',value.parent,0)]
            elif fault=='config_extent':
                name,a,size,kind,section=objects.elf.symbol_entries[0];objects.elf.symbol_entries=[(name,a,size-4,kind,section)]
            elif fault=='outline_frame':objects.sizes[0x10029084]=8
            else:
                a,size,section=objects.elf.functions['OUTLINED_FUNCTION_310']
                objects.elf.functions['OUTLINED_FUNCTION_310']=(a,size-2,section)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):current_validator_family(objects,value)
        record=self.fixture('y_action_pairs','speed2').to_record()
        for fault in ('bool_site','float_offset','float_capture','bool_writable','unknown_operation','missing_transport','protected_overlap'):
            bad=deepcopy(record)
            if fault=='bool_site':bad['sites'][0]['site']=True
            elif fault=='float_offset':bad['sites'][0]['offset']=1.0
            elif fault=='float_capture':bad['transport']['capture_offset']=96.0
            elif fault=='bool_writable':bad['contexts'][0]['config']['storage']['writable']=1
            elif fault=='unknown_operation':bad['operation']='ordinary_multiple_STRD'
            elif fault=='missing_transport':bad['transport'].pop('output')
            else:
                c=bad['contexts'][0];guard=deepcopy(c['protected'][0])
                guard.update(start=c['frame']['start']+169,extent=2,alignment=1);c['protected'].append(guard)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):read_validator_family(bad)


    def test_exact_wire_bound_is_preserved_at_actual_stack_result_boundary(self):
        import config_platform
        from dataclasses import replace
        from config_regions import Storage
        objects,view,context=ConditionalStackPolicy.fixture(self)
        def policy(padding):
            deps=self.dependencies('sizes');first=deps[0];c=first.contexts[0]
            extra=Storage(c.frame.space,'bound-'+('x'*padding),c.frame.start+184,4,4,c.phase,False)
            first=replace(first,contexts=(replace(c,protected=c.protected+(extra,)),first.contexts[1]))
            return config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=(),owned_dependencies=(first,)+deps[1:])
        small=policy(1);size=lambda value:len((json.dumps(value,sort_keys=True)+'\n').encode())
        padding=65500-size(small.to_record())+1
        try:near=policy(padding)
        except AssertionError as error:self.fail('wire-compatible policy rejected before enclosing record: '+str(error))
        self.assertEqual(size(near.to_record()),65500)
        with self.assertRaises(AssertionError):
            config_platform.qualified_stack_result(dict(boot=4096,constructor=4096,runtime=8192,dashboard=16384),65536,near)


    def image_fixture(self,mode,padding=0):
        # Supplied stage facts only. The image/capture consumers are real; the
        # missing complete source/validator/graph producer is never inferred.
        from test_validation_policy import MemoryQualification
        from config_platform import PlatformPolicy,qualified_stack_result
        from config_owned_access import read_owned_access
        from config_validator_access import ValidatorFamily
        from config_led_access import LedFamily
        from dataclasses import replace
        from config_regions import Storage
        import check_memory_layout
        import hashlib
        elf,eav,old=MemoryQualification.owned_fixture(MemoryQualification())
        prototype=self.artifact(mode);code=prototype.elf.sections[0];index=len(elf.sections);offset=len(elf.data)
        elf.data+=prototype.elf.data[:code[5]];section=list(code);section[4]=offset;elf.sections.append(tuple(section))
        elf.functions.update({n:(v[0],v[1],index) for n,v in prototype.elf.functions.items()})
        elf.mapping_symbols=[('$t',elf.sections[1][3],1),('$t',code[3],index)]
        metadata=bytearray()
        for address,size in prototype.sizes.items():
            metadata+=struct.pack('<I',address)
            while True:
                byte=size&127;size>>=7;metadata.append(byte|(128 if size else 0))
                if not size:break
        elf.section_bytes=lambda name:bytes(metadata) if name=='.stack_sizes' else b''
        validators=[]
        for value in self.dependencies(mode):
            if not isinstance(value,ValidatorFamily):continue
            contexts=[]
            for c in value.contexts:
                contexts.append(replace(c,config=replace(c.config,storage=replace(c.config.storage,start=0x20000000))))
            validators.append(replace(value,contexts=tuple(contexts)))
        if padding:
            first=validators[0];c=first.contexts[0];local={'speed2':200,'sizes':184,'sizez':216}[mode]
            extra=Storage(c.frame.space,'bound-'+('x'*padding),c.frame.start+local,4,4,c.phase,False)
            validators[0]=replace(first,contexts=(replace(c,protected=c.protected+(extra,)),first.contexts[1]))
        leds=[]
        for value in self.dependencies(mode):
            if not isinstance(value,LedFamily):continue
            contexts=[]
            for c in value.contexts:
                config=replace(c.config,storage=replace(c.config.storage,start=0x20000000))
                owner=replace(c.owner,storage=config.storage) if c.owner.storage.space=='address' else c.owner
                contexts.append(replace(c,config=config,owner=owner))
            leds.append(replace(value,contexts=tuple(contexts)))
        legacy=tuple(read_owned_access(v) for v in old['policies'][0]['owned_dependencies']) if mode=='sizez' else ()
        policy=PlatformPolicy(0x1000000c,0x10000000+len(eav),hashlib.sha256(eav[12:]).hexdigest(),
            hashlib.sha256(elf.data).hexdigest(),owned_dependencies=legacy+tuple(validators)+tuple(leds),owned_family=mode+'_configuration')
        _,headroom=check_memory_layout.geometry(elf)
        stack=qualified_stack_result(dict(boot=4096,constructor=4096,runtime=8192,dashboard=16384),headroom,policy)
        return elf,eav,stack

    def test_actual_image_and_capture_readers_preserve_v5_with_supplied_stage_facts(self):
        import check_memory_layout,capture_target_bench as capture
        from unittest.mock import patch
        from contextlib import redirect_stdout
        from copy import deepcopy
        import io,tempfile,subprocess,sys
        real_run=subprocess.run
        for mode in ('speed2','sizes','sizez'):
            elf,eav,stack=self.image_fixture(mode);output=io.StringIO()
            with patch.object(check_memory_layout,'ElfImage',return_value=elf),\
                 patch.object(check_memory_layout,'check_stack_budget',return_value=stack),\
                 patch.object(check_memory_layout,'check_flash_lockout'),redirect_stdout(output):
                record=check_memory_layout.check('synthetic-validator.elf')
            self.assertIn('CONDITIONAL_IMAGE_PASS, assumed_external, hardware_qualified=false',output.getvalue())
            self.assertEqual(check_memory_layout.read_image_result(json.loads(json.dumps(record)),elf,eav),record)
            with tempfile.TemporaryDirectory() as directory,patch.object(capture,'ElfImage',return_value=elf):
                path=Path(directory)/'memory-layout.json';path.write_text(json.dumps(record,sort_keys=True)+'\n')
                self.assertEqual(capture.read_memory_gate(path,'synthetic-validator.elf',eav)['record'],record)
                path.unlink()
                def writer(argv,**kwargs):
                    return real_run([sys.executable,'-c','import pathlib,sys;pathlib.Path(sys.argv[1]).write_text(sys.argv[2],encoding="utf-8")',
                        argv[-1],json.dumps(record,sort_keys=True)+'\n'],check=kwargs['check'],timeout=10)
                with patch.object(capture.subprocess,'run',side_effect=writer):
                    self.assertEqual(capture.run_memory_gate(Path('synthetic-validator.elf'),Path(directory),eav)['record'],record)
                for fault in ('lost','duplicate','context','old','hardware','width_float','nonreturn_intermediate'):
                    bad=deepcopy(record);policy=bad['stack']['policies'][0]
                    if fault=='lost':policy['owned_dependencies'].pop()
                    elif fault=='duplicate':policy['owned_dependencies'].append(deepcopy(policy['owned_dependencies'][-1]))
                    elif fault=='context':policy['owned_dependencies'][-1]['contexts'][0]['epoch']='retired'
                    elif fault=='old':policy.update(schema=3,scope='h2-rp2350-secure-owned-access-v4')
                    elif fault=='hardware':bad['hardware_qualified']=True
                    elif fault=='width_float':policy['owned_dependencies'][-1]['width']=2.0
                    else:bad=bad['stack']
                    path.write_text(json.dumps(bad))
                    with self.subTest(mode=mode,fault=fault),self.assertRaises((AssertionError,ValueError)):
                        capture.read_memory_gate(path,'synthetic-validator.elf',eav)

    def test_enclosing_image_wire_bound_is_checked_after_fitting_stack_record(self):
        import check_memory_layout
        import capture_target_bench as capture
        from unittest.mock import patch
        import tempfile
        size=lambda value:len((json.dumps(value,sort_keys=True)+'\n').encode())
        _,_,small=self.image_fixture('sizes',1)
        elf,eav,stack=self.image_fixture('sizes',65500-size(small)+1)
        self.assertEqual(size(stack),65500)
        static,headroom=check_memory_layout.geometry(elf)
        with self.assertRaises(AssertionError):check_memory_layout._qualified_image(stack,elf,static,headroom,eav)
        initial,eav,small=self.image_fixture('sizes',1)
        static,headroom=check_memory_layout.geometry(initial)
        small_image=check_memory_layout._qualified_image(small,initial,static,headroom,eav)
        elf,eav,stack=self.image_fixture('sizes',65536-size(small_image)+1)
        static,headroom=check_memory_layout.geometry(elf)
        image=check_memory_layout._qualified_image(stack,elf,static,headroom,eav)
        self.assertEqual(size(image),65536)
        with tempfile.TemporaryDirectory() as directory,patch.object(capture,'ElfImage',return_value=elf):
            path=Path(directory)/'boundary.json';wire=json.dumps(image,sort_keys=True)+'\n'
            path.write_bytes(wire.encode())
            self.assertEqual(capture.read_memory_gate(path,'synthetic-validator.elf',eav)['record'],image)
            path.write_bytes((wire+' ').encode())
            with self.assertRaises(ValueError):capture.read_memory_gate(path,'synthetic-validator.elf',eav)

class ValidatorResultBoundary(unittest.TestCase):
    def fixture(self,phase='default_validation',shift=0):
        from dataclasses import replace
        import re
        item=json.loads((Path(__file__).parent/'fixtures/config-validator-result.json').read_text())
        def relocated(a):return a+shift if 0x10000000<=a<0x10200000 else a
        def section(s):s=list(s);s[3]=relocated(s[3]);return tuple(s)
        sections=list(map(section,item['sections']))
        raw=bytearray(max(s[4]+s[5] for s in sections if s[2]&2 and s[1]!=8))
        for i,address,encoded in item['segments']:
            s=sections[i];at=s[4]+relocated(address)-s[3];raw[at:at+len(bytes.fromhex(encoded))]=bytes.fromhex(encoded)
        for address in item['pointers']:
            s=next(s for s in sections if s[2]&2 and s[3]<=relocated(address)<s[3]+s[5])
            at=s[4]+relocated(address)-s[3];value=struct.unpack_from('<I',raw,at)[0]
            struct.pack_into('<I',raw,at,relocated(value))
        elf=SimpleNamespace(data=bytes(raw),sections=sections,named={n:section(s) for n,s in item['named'].items()},
            functions={n:(relocated(a),z,ix) for n,(a,z,ix) in item['functions'].items()},
            symbol_entries=[(n,relocated(a),z,info,ix) for n,a,z,info,ix in item['symbol_entries']],
            mapping_symbols=[(n,relocated(a),ix) for n,a,ix in item['mapping_symbols']])
        objects=config_stack.DiagnosticObjects(elf,{relocated(int(a)):z for a,z in item['sizes'].items()})
        rows={}
        for row in item['records']:
            a=relocated(row['address']);operand=re.sub(r'0x[0-9a-f]+',lambda m:hex(relocated(int(m[0],16))),row['operands'])
            rows[a]=Instruction(a,bytes.fromhex(row['bytes']),row['mnemonic'],operand)
        name=next(n for n,e in elf.functions.items() if e[0]&~1==0x1001ea20+shift)
        contexts=ValidatorPlatformFamilies().fixture('first_word','sizez').contexts
        context=contexts[0 if phase=='default_validation' else 1]
        return objects,name,rows,context,(0x100203e2 if phase=='default_validation' else 0x10020e36)+shift

    def test_result_boundary_has_no_untyped_output_or_implicit_return_pointer(self):
        import importlib.util
        self.assertIsNotNone(importlib.util.find_spec('config_validator_result'),
            'missing finite validator Result/carrier boundary')
        from config_validator_result import validator_result_boundary
        for phase in ('default_validation','decoded_validation'):
            for shift in (0,0x2800):
                objects,name,rows,context,call=self.fixture(phase,shift)
                result=validator_result_boundary(objects,name,rows,context,call)
                self.assertFalse(result['early_carrier']['spill_initialized'])
                self.assertEqual(result['normal_return']['restored_bytes'],320)
                self.assertEqual(result['normal_return']['return_to'],call+4)
                self.assertEqual(result['normal_return']['r0'],'not a Result pointer ABI')
                self.assertEqual(result['save_layout']['registers']['r8'],284)
                self.assertEqual(result['save_layout']['registers']['return'],316)
                self.assertEqual(result['first'].start,context.frame.start+76)
                self.assertEqual(result['spill'].start,context.frame.start+60)
                error,success=result['alternatives']
                self.assertEqual(error['requires_completed_first_Some_tags'],(0,1))
                self.assertEqual(success['inactive_or_padding'],((2,4),(8,12)))
                self.assertEqual(result['error_copy']['physical_activation_paths'],((8,),(72,)))
                self.assertTrue(result['requires_all_visitor_first_capture_spill_valid_save_and_ancestor_preservation'])
                self.assertTrue(result['platform_requirements_unresolved'])

    def test_wrong_call_epoch_carrier_spill_success_store_and_restore_reject(self):
        from config_validator_result import validator_result_boundary
        from dataclasses import replace
        for fault in ('early_r5','spill','late_reload','success_output','restore','epoch','config','ancestor_prologue'):
            objects,name,rows,context,call=self.fixture();parent=objects.elf.functions[name][0]&~1
            if fault=='epoch':call=0x10020e36
            elif fault=='config':context=replace(context,config=replace(context.config,storage=replace(context.config.storage,start=context.config.start+4)))
            elif fault=='ancestor_prologue':
                from config_regions import Storage
                guard=Storage(context.frame.space,'unexpected live ancestor',context.frame.start+220,4,4,context.phase,False)
                context=replace(context,protected=context.protected+(guard,))
            else:
                at={'early_r5':parent+14,'spill':parent+86,'late_reload':parent+396,
                    'success_output':parent+4494,'restore':0x1002846c}[fault]
                row=rows[at]
                operation,operand={'early_r5':('mov','r5, r1'),'spill':('str','r5, [sp, #0x40]'),
                    'late_reload':('ldr','r5, [sp, #0x40]'),'success_output':('vstr','s0, [r5, #8]'),
                    'restore':('pop.w','{r8, r9, r10}') }[fault]
                rows[at]=Instruction(at,row.raw,operation,operand)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                validator_result_boundary(objects,name,rows,context,call)

    def test_coherent_register_slot_store_and_restore_mutants_do_not_use_stale_byte_rejection(self):
        from config_validator_result import validator_result_boundary
        cases=((0x1001ea2e,'0d46','mov','r5, r1'),
            (0x1001ea76,'1095','str','r5, [sp, #0x40]'),
            (0x1001ebac,'109d','ldr','r5, [sp, #0x40]'),
            (0x1001fbae,'85ed020a','vstr','s0, [r5, #8]'),
            (0x1002846c,'bde80007','pop.w','{r8, r9, r10}'),
            (0x100203e0,'2846','mov','r0, r5'))
        for at,encoded,operation,operand in cases:
            objects,name,rows,context,call=self.fixture();raw=bytes.fromhex(encoded)
            s=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=at<at+len(raw)<=s[3]+s[5])
            offset=s[4]+at-s[3];data=bytearray(objects.elf.data);data[offset:offset+len(raw)]=raw;objects.elf.data=bytes(data)
            rows[at]=Instruction(at,raw,operation,operand)
            self.assertEqual(objects.read_only(at,len(raw)),rows[at].raw)
            with self.subTest(site=hex(at)),self.assertRaisesRegex(AssertionError,'operands differ'):
                validator_result_boundary(objects,name,rows,context,call)

    def test_foreign_start_and_interior_ingress_cannot_borrow_output_or_nonreturn_suffix(self):
        from config_validator_result import validator_result_boundary
        for source in (0x1001ea84,0x10060000):
            for target in (0x1001ea2e,0x1001eb66,0x1001eb70,0x1001eb7a,0x1001ebac,0x1001edc0,0x1001edc8,0x1001fbae):
                objects,name,rows,context,call=self.fixture()
                rows[source]=Instruction(source,rows[source].raw if source in rows else bytes(2),'b',hex(target))
                with self.subTest(source=hex(source),target=hex(target)),self.assertRaises(AssertionError):
                    validator_result_boundary(objects,name,rows,context,call)

    def test_hold_range_keeps_actual_u16_reference_alignment(self):
        from config_validator_result import validator_result_boundary
        objects,name,rows,context,call=self.fixture();data=bytearray(objects.elf.data)
        def offset(address):
            s=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=address<s[3]+s[5])
            return s[4]+address-s[3]
        pool=offset(0x1001f1e0);old=struct.unpack_from('<I',data,pool)[0];source=objects.read_only(old,6)
        new=old+1;at=offset(new);data[at:at+6]=source;struct.pack_into('<I',data,pool,new);objects.elf.data=bytes(data)
        with self.assertRaises(AssertionError):validator_result_boundary(objects,name,rows,context,call)

    def test_independent_even_range_and_consumed_PC_pool_relocation_accept(self):
        from config_validator_result import validator_result_boundary
        from llvm_stack import mapping_ranges
        for phase in ('default_validation','decoded_validation'):
            for shift in (0,0x2800):
                for move_pool in (False,True):
                    objects,name,rows,context,call=self.fixture(phase,shift);elf=objects.elf
                    def offset(address):
                        s=next(s for s in elf.sections if s[2]&2 and s[3]<=address<s[3]+s[5])
                        return s[4]+address-s[3]
                    original_pool=0x1001f1e0+shift;original=struct.unpack_from('<I',elf.data,offset(original_pool))[0]
                    literal=objects.read_only(original,6);new_range=0x10070002
                    self.assertEqual(new_range%2,0);self.assertNotEqual(original,new_range)
                    # Fresh uniquely backed readonly data; its position does
                    # not track code, the original pool or the common shift.
                    data=bytearray(elf.data);index=len(elf.sections);file_offset=len(data)
                    data.extend(literal);elf.sections.append((0,1,2,new_range,file_offset,6,0,0,2,0))
                    elf.mapping_symbols.append(('$d',new_range,index))
                    pool=(0x1001f1c0+shift) if move_pool else original_pool
                    if move_pool:
                        self.assertNotEqual(pool,original_pool)
                        self.assertTrue(any(k=='d' and lo<=pool<pool+4<=hi for lo,hi,k in mapping_ranges(elf,name)))
                        at=0x1001ea60+shift;pc=(at+4)&~3;immediate=pool-pc
                        self.assertTrue(0<=immediate<=0xfff)
                        encoded=struct.pack('<HH',0xf8df,immediate)  # Thumb LDR.W R0,[PC,#imm12].
                        before=rows[at];self.assertNotEqual(encoded,before.raw)
                        data[offset(at):offset(at)+4]=encoded
                        rows[at]=Instruction(at,encoded,'ldr.w','r0, [pc, #'+hex(immediate)+']')
                        self.assertEqual(pc+(struct.unpack('<HH',encoded)[1]&0xfff),pool)
                    struct.pack_into('<I',data,offset(pool),new_range);elf.data=bytes(data)
                    self.assertEqual(objects.read_only(new_range,6),literal)
                    self.assertEqual(struct.unpack('<I',objects.read_only(pool,4))[0],new_range)
                    result=validator_result_boundary(objects,name,rows,context,call)
                    with self.subTest(phase=phase,code_shift=shift,moved_pool=move_pool):
                        self.assertEqual(result['proofs']['entry']['function'],0x1001ea20+shift)
                        self.assertEqual(result['proofs']['entry']['objects'][1],new_range)
                        self.assertEqual(result['normal_return']['return_to'],call+4)
                        self.assertTrue(result['requires_complete_typed_Config_and_exclusive_Result_owner'])


class ValidatorLocalWrites(unittest.TestCase):
    def test_fixed_local_envelope_preserves_unique_capture_and_result_pointer_spill(self):
        import importlib.util
        self.assertIsNotNone(importlib.util.find_spec('config_validator_writes'),
            'missing grouped current validator local-write proof')
        from config_validator_writes import validator_local_writes
        for phase in ('default_validation','decoded_validation'):
            for shift in (0,0x2800):
                objects,name,rows,context,call=ValidatorResultBoundary().fixture(phase,shift)
                result=validator_local_writes(objects,name,rows,context,call)
                self.assertEqual(len(result['local_writes']),128)
                self.assertEqual(len(result['unresolved_pointer_writes']),56)
                self.assertEqual(result['spill_writers'],((0x1001ea76+shift,60,64),))
                self.assertEqual(result['capture_writers'],((0x1001ea3e+shift,88,92),))
                self.assertTrue(all(0<=start<end<=216 for site,start,end in result['local_ranges']))
                self.assertTrue(result['requires_all_child_SP_R7_and_pointer_effects'])

    def test_local_base_map_rejects_load_outputs_aliasing_SP_or_second_R7(self):
        from config_validator_writes import validator_local_writes
        for operation,operand in (('ldr.w','sp, [sp, #0x4]'),('ldrd','r0, r7, [r7, #4]')):
            objects,name,rows,context,call=ValidatorResultBoundary().fixture();parent=objects.elf.functions[name][0]&~1
            at=next(a for a,row in sorted(rows.items()) if parent+450<=a<parent+4478 and not parent+928<=a<parent+942 and len(row.raw)==4 and row.mnemonic==operation)
            row=rows[at];rows[at]=Instruction(at,row.raw,operation,operand)
            with self.subTest(operation=operation),self.assertRaises(AssertionError):
                validator_local_writes(objects,name,rows,context,call)

    def test_coherent_stack_stores_cannot_overwrite_saves_capture_or_spill(self):
        from config_validator_writes import validator_local_writes
        for destination in (216,60,88):
            objects,name,rows,context,call=ValidatorResultBoundary().fixture();parent=objects.elf.functions[name][0]&~1
            at=next(a for a,row in sorted(rows.items()) if parent+88<=a<parent+304 and row.mnemonic=='str' and
                len(row.raw)==2 and '[sp' in row.operands)
            encoded=struct.pack('<H',0x9000|(destination//4));row=Instruction(at,encoded,'str','r0, [sp, #'+hex(destination)+']')
            s=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=at<s[3]+s[5]);offset=s[4]+at-s[3]
            raw=bytearray(objects.elf.data);raw[offset:offset+2]=encoded;objects.elf.data=bytes(raw);rows[at]=row
            self.assertEqual(objects.read_only(at,2),row.raw)
            with self.subTest(destination=destination),self.assertRaisesRegex(AssertionError,'escapes locals|overwrites its exclusive'):
                validator_local_writes(objects,name,rows,context,call)

    def test_unknown_effect_alignment_and_base_writeback_are_not_opaque_arithmetic(self):
        from config_validator_writes import validator_local_writes
        cases=(('str','r0, [sp, #0x1]'),('str','r0, [sp], #0x4'),('strt','r0, [sp, #0x4]'),
            ('mov','r13, r0'),('push','{r0}'),('str','r0, [r7, #-216]!'))
        for operation,operand in cases:
            objects,name,rows,context,call=ValidatorResultBoundary().fixture();parent=objects.elf.functions[name][0]&~1
            at=next(a for a,row in sorted(rows.items()) if parent+88<=a<parent+304 and row.mnemonic=='str')
            rows[at]=Instruction(at,rows[at].raw,operation,operand)
            with self.subTest(operation=operation,operand=operand),self.assertRaises(AssertionError):
                validator_local_writes(objects,name,rows,context,call)


class ValidatorPointerRegions(unittest.TestCase):
    def fixture(self,phase='default_validation',shift=0):
        objects,name,rows,context,call=ValidatorResultBoundary().fixture(phase,shift)
        item=json.loads((Path(__file__).parent/'fixtures/config-validator-pointers.json').read_text())
        def relocated(a):return a+shift if 0x10000000<=a<0x10200000 else a
        elf=objects.elf;raw=bytearray(elf.data)
        for index,address,encoded in item['segments']:
            section=elf.sections[index];at=section[4]+relocated(address)-section[3]
            raw[at:at+len(bytes.fromhex(encoded))]=bytes.fromhex(encoded)
        elf.data=bytes(raw)
        for n,(a,z,index) in item['functions'].items():elf.functions[n]=(relocated(a),z,index)
        objects.sizes.update({relocated(int(a)):z for a,z in item['sizes'].items()})
        elf.symbol_entries.extend((n,relocated(a),z,info,index) for n,a,z,info,index in item['symbol_entries'])
        for row in item['records']:
            a=relocated(row['address']);operand=re.sub(r'0x[0-9a-f]+',lambda m:hex(relocated(int(m[0],16))),row['operands'])
            rows[a]=Instruction(a,bytes.fromhex(row['bytes']),row['mnemonic'],operand)
        return objects,name,rows,context,call

    def test_current_source_regions_bind_distinct_pointer_owners_and_child_effects(self):
        import importlib.util
        self.assertIsNotNone(importlib.util.find_spec('config_validator_pointers'),
            'missing grouped validator pointer source regions')
        from config_validator_pointers import validator_pointer_regions
        for phase in ('default_validation','decoded_validation'):
            for shift in (0,0x2800):
                objects,name,rows,context,call=self.fixture(phase,shift)
                proof=validator_pointer_regions(objects,name,rows,context,call)
                self.assertEqual(len(proof['pointer_writes']),55)
                self.assertEqual(proof['families']['edges']['owner_offsets'],(104,107))
                self.assertEqual(proof['families']['occupied']['owner_offsets'],(104,112))
                self.assertEqual(proof['families']['page_array']['owner_offsets'],(120,123))
                self.assertEqual(proof['families']['result']['view'],context.result)
                self.assertIn('r4',proof['children']['band_capture']['clobbers'])
                self.assertTrue(proof['requires_actual_source_typed_parent_entry_and_full_graph'])
                self.assertTrue(proof['platform_requirements_unresolved'])

    def test_semantic_foreign_CBZ_ingress_cannot_bypass_a_capture_outline(self):
        from config_validator_pointers import validator_pointer_regions
        objects,name,rows,context,call=self.fixture()
        # Semantic-record ingress fixture, not a coherent decoded opcode.
        rows[0x10000050]=Instruction(0x10000050,b'\x00\xb1','cbz','r0, 0x1001f784')
        with self.assertRaisesRegex(AssertionError,'foreign direct ingress'):
            validator_pointer_regions(objects,name,rows,context,call)

    def test_capture_leaf_effects_invalidate_condition_predicates(self):
        from config_validator_pointers import validator_pointer_regions
        for phase in ('default_validation','decoded_validation'):
            for shift in (0,0x2800):
                objects,name,rows,context,call=self.fixture(phase,shift)
                proof=validator_pointer_regions(objects,name,rows,context,call)
                for role,site,register in (('first_capture',0x10028930,'r1'),('band_capture',0x1002823c,'r4')):
                    self.assertEqual((rows[site+shift].mnemonic,rows[site+shift].operands),('movs',register+', #0x0'))
                    with self.subTest(phase=phase,shift=shift,role=role):
                        self.assertIn('flags',proof['children'][role]['clobbers'])

    def test_coherent_capture_load_cannot_borrow_the_Result_spill(self):
        from config_validator_pointers import validator_pointer_regions
        for address,operation,operand,encoded in (
            (0x1001fb34,'ldr','r5, [sp, #0x3c]',bytes.fromhex('0f9d')),
            (0x1002823c,'movs','r7, #0x0',bytes.fromhex('0027'))):
            objects,name,rows,context,call=self.fixture();s=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=address<s[3]+s[5])
            raw=bytearray(objects.elf.data);at=s[4]+address-s[3];raw[at:at+2]=encoded
            objects.elf.data=bytes(raw);rows[address]=Instruction(address,encoded,operation,operand)
            self.assertEqual(objects.read_only(address,2),encoded)
            with self.subTest(address=hex(address)),self.assertRaisesRegex(AssertionError,'operands differ'):
                validator_pointer_regions(objects,name,rows,context,call)

    def test_warning_literal_source_role_cannot_change_with_coherent_data(self):
        from config_validator_pointers import validator_pointer_regions
        objects,name,rows,context,call=self.fixture();slot=0x1001fd54
        s=next(s for s in objects.elf.sections if s[2]&2 and s[3]<=slot<s[3]+s[5]);at=s[4]+slot-s[3]
        raw=bytearray(objects.elf.data);raw[at:at+4]=bytes.fromhex('ff053400');objects.elf.data=bytes(raw)
        with self.assertRaisesRegex(AssertionError,'literal source role'):
            validator_pointer_regions(objects,name,rows,context,call)

    def test_independent_warning_pool_placement_preserves_the_paired_scalar_role(self):
        from config_validator_pointers import validator_pointer_regions
        for phase in ('default_validation','decoded_validation'):
            for shift in (0,0x2800):
                objects,name,rows,context,call=self.fixture(phase,shift)
                elf=objects.elf;new=0x1001fdb4+shift;load=0x1001f9fe+shift;old=0x1001fd54+shift
                def offset(a):
                    s=next(s for s in elf.sections if s[2]&2 and s[3]<=a<s[3]+s[5]);return s[4]+a-s[3]
                raw=bytearray(elf.data);literal=objects.read_only(old,4);raw[offset(new):offset(new)+4]=literal
                immediate=new-((load+4)&~3);self.assertTrue(0<=immediate<=1020 and immediate%4==0)
                encoded=struct.pack('<H',0x4a00|(immediate//4));raw[offset(load):offset(load)+2]=encoded
                elf.data=bytes(raw);rows[load]=Instruction(load,encoded,'ldr','r2, [pc, #'+hex(immediate)+']')
                proof=validator_pointer_regions(objects,name,rows,context,call)
                self.assertIn((load,new,0x3305ff,'scalar'),proof['regions']['widget_variants_and_root_warning']['pools'])

    def test_leaf_frame_and_outline_identity_changes_are_not_unknown_child_allowances(self):
        from config_validator_pointers import validator_pointer_regions
        for at,frame in ((0x1001bdec,16),(0x1001c1f8,8),(0x1002892e,8),(0x10028b16,8)):
            objects,name,rows,context,call=self.fixture();objects.sizes[at]=frame
            with self.subTest(child=hex(at)),self.assertRaises(AssertionError):
                validator_pointer_regions(objects,name,rows,context,call)

    def test_other_semantic_foreign_branch_entries_and_shared_interiors_reject(self):
        from config_validator_pointers import validator_pointer_regions
        for operation in ('b','bl','cbnz'):
            for target in (0x1001f780,0x1001f784,0x1001fa52,0x1001fb34):
                objects,name,rows,context,call=self.fixture()
                operand=('r0, ' if operation=='cbnz' else '')+hex(target)
                rows[0x10000050]=Instruction(0x10000050,b'\x00\xbf',operation,operand)
                with self.subTest(operation=operation,target=hex(target)),self.assertRaisesRegex(AssertionError,'foreign direct ingress'):
                    validator_pointer_regions(objects,name,rows,context,call)

    def test_child_pushes_do_not_overwrite_another_supplied_live_allocation(self):
        from dataclasses import replace
        from config_regions import Storage
        from config_validator_pointers import validator_pointer_regions
        objects,name,rows,context,call=self.fixture();q=context.frame
        below=Storage(q.space,'separate protected allocation below this frame',q.start-8,8,8,q.phase,False)
        context=replace(context,protected=context.protected+(below,))
        with self.assertRaisesRegex(AssertionError,'overlap'):
            validator_pointer_regions(objects,name,rows,context,call)


class ValidatorControls(unittest.TestCase):
    @staticmethod
    def patch(objects,rows,address,encoded,operation=None,operand=None):
        s=next(s for s in objects.elf.sections if s[1]==1 and s[2]&2 and s[3]<=address<address+len(encoded)<=s[3]+s[5])
        raw=bytearray(objects.elf.data);at=s[4]+address-s[3];raw[at:at+len(encoded)]=encoded;objects.elf.data=bytes(raw)
        if operation is not None:rows[address]=Instruction(address,encoded,operation,operand)
        assert objects.read_only(address,len(encoded))==encoded

    def fixture(self,phase='default_validation',shift=0):
        objects,name,rows,context,call=ValidatorPointerRegions().fixture(phase,shift)
        item=json.loads((Path(__file__).parent/'fixtures/config-validator-controls.json').read_text())
        def relocated(a):return a+shift if 0x10000000<=a<0x10200000 else a
        elf=objects.elf;raw=bytearray(elf.data)
        for index,address,encoded in item['segments']:
            s=elf.sections[index];at=s[4]+relocated(address)-s[3]
            raw[at:at+len(bytes.fromhex(encoded))]=bytes.fromhex(encoded)
        # This previously unconsumed FIRST data-pool pointer is now typed.
        s=next(s for s in elf.sections if s[2]&2 and s[3]<=relocated(0x1001bde8)<s[3]+s[5])
        struct.pack_into('<I',raw,s[4]+relocated(0x1001bde8)-s[3],relocated(0x1003c3a0))
        elf.data=bytes(raw)
        for n,(a,z,index) in item['functions'].items():elf.functions[n]=(relocated(a),z,index)
        objects.sizes.update({relocated(int(a)):z for a,z in item['sizes'].items()})
        elf.symbol_entries.extend((n,relocated(a),z,info,index) for n,a,z,info,index in item['symbol_entries'])
        for row in item['records']:
            a=relocated(row['address']);operand=re.sub(r'0x[0-9a-f]+',lambda m:hex(relocated(int(m[0],16))),row['operands'])
            rows[a]=Instruction(a,bytes.fromhex(row['bytes']),row['mnemonic'],operand)
        return objects,name,rows,context,call

    def test_current_control_source_arguments_and_capture_effects_remain_paired(self):
        import importlib.util
        self.assertIsNotNone(importlib.util.find_spec('config_validator_controls'),'missing finite controls argument/effect consumer')
        from config_validator_controls import validator_controls
        for phase in ('default_validation','decoded_validation'):
            for shift in (0,0x2800):
                objects,name,rows,context,call=self.fixture(phase,shift)
                proof=validator_controls(objects,name,rows,context,call)
                self.assertEqual(len(proof['regions']),3)
                self.assertEqual(proof['loops']['pages'],(0,1,2))
                self.assertEqual(proof['loops']['buttons'],(0,1,2,3))
                self.assertEqual(proof['loops']['tap_hold'],((0,False),(2,True)))
                self.assertEqual(proof['children']['action_allowed']['source_action_indices'],tuple(range(19)))
                self.assertEqual(proof['children']['control_error_capture']['source_error_bytes'],(0,1,2,3,5,6,7,8))
                self.assertEqual(proof['children']['control_error_capture']['code_map'],(5,7,7,7,7,3,3))
                self.assertIn('flags',proof['children']['control_error_capture']['clobbers'])
                self.assertEqual(proof['capture_cells'],dict(valid=95,first=76,capture=88))
                self.assertEqual(proof['child_stack_alternatives'],((8,0),(16,)))
                self.assertEqual(tuple(len(t['targets']) for t in proof['typed_tables']),(19,4))
                self.assertEqual(proof['typed_tables'][1]['targets'],tuple(a+shift for a in (0x1001eb38,0x1001eb3a,0x1001eb46,0x1001eb40)))
                self.assertTrue(proof['requires_actual_private_Config_source_and_loop_entry'])
                self.assertTrue(proof['requires_whole_parent_control_coverage_and_platform_policy'])

    def test_coherent_capture_arguments_and_first_stores_cannot_borrow_other_cells(self):
        from config_validator_controls import validator_controls
        for address,encoded,operation,operand in (
            (0x1001eba6,bytes.fromhex('0fa9'),'add','r1, sp, #0x3c'),
            (0x1001bdda,bytes.fromhex('4a81'),'strh','r2, [r1, #0xa]')):
            objects,name,rows,context,call=self.fixture()
            self.patch(objects,rows,address,encoded,operation,operand)
            with self.subTest(address=hex(address)),self.assertRaisesRegex(AssertionError,'operands differ'):
                validator_controls(objects,name,rows,context,call)

    def test_action_rotation_padding_and_ErrorCode_tables_remain_exact(self):
        from config_validator_controls import validator_controls
        for address,encoded in ((0x1000ff1b,b'\x15'),(0x1001eb34,b'\x03'),(0x1003c3a0,b'\x07')):
            objects,name,rows,context,call=self.fixture();self.patch(objects,rows,address,encoded)
            with self.subTest(address=hex(address)),self.assertRaisesRegex(AssertionError,'mapped bytes|ErrorCode map'):
                validator_controls(objects,name,rows,context,call)

    def test_error_code_object_relocates_independently_of_the_caller_and_child(self):
        from config_validator_controls import validator_controls
        for phase in ('default_validation','decoded_validation'):
            for shift in (0,0x2800):
                objects,name,rows,context,call=self.fixture(phase,shift);elf=objects.elf
                raw=bytearray(elf.data);new=0x10070001;at=len(raw);raw.extend(bytes.fromhex('05070707070303'))
                elf.sections.append((0,1,2,new,at,7,0,0,1,0));elf.data=bytes(raw)
                self.patch(objects,rows,0x1001bde8+shift,struct.pack('<I',new))
                proof=validator_controls(objects,name,rows,context,call)
                self.assertEqual(proof['children']['control_error_capture']['objects'][0],new)

    def test_semantic_foreign_entries_cannot_bypass_controls_loop_or_capture_arguments(self):
        from config_validator_controls import validator_controls
        for operation in ('b','bl','cbz','cbnz'):
            for source in (0x10000050,0x1001ed02):
                objects,name,rows,context,call=self.fixture()
                # Semantic records only; no claim of a newly decoded branch.
                operand=('r0, ' if operation.startswith('cb') else '')+'0x1001eba6'
                raw=rows[source].raw if source in rows else b'\x00\xbf'
                rows[source]=Instruction(source,raw,operation,operand)
                with self.subTest(operation=operation,source=hex(source)),self.assertRaisesRegex(AssertionError,'foreign direct ingress'):
                    validator_controls(objects,name,rows,context,call)

    def test_child_frame_and_zero_outline_identity_are_closed(self):
        from config_validator_controls import validator_controls,_child
        for address,frame in ((0x1000fefc,16),(0x1001bdb4,8),(0x10028926,8)):
            objects,name,rows,context,call=self.fixture();objects.sizes[address]=frame
            with self.subTest(address=hex(address)),self.assertRaises(AssertionError):
                validator_controls(objects,name,rows,context,call)
        objects,name,rows,context,call=self.fixture()
        ordinary=next(n for n,e in objects.elf.functions.items() if e[0]&~1==0x1000fefc)
        with self.assertRaisesRegex(AssertionError,'outline borrows ordinary'):
            _child(objects,ordinary,rows,'zero_to_bool',{})

    def test_capture_effects_preserve_return_state_and_invalidate_predicates(self):
        from config_validator_controls import validator_controls
        objects,name,rows,context,call=self.fixture();proof=validator_controls(objects,name,rows,context,call)
        capture=proof['children']['control_error_capture']
        self.assertEqual(tuple((view.offset,offset,size) for view,offset,size in capture['writes']),
            ((76,0,2),(76,4,1),(76,6,1),(76,7,1),(76,8,2),(76,10,1),(95,0,1)))
        self.assertIn('sp',capture['preserves']);self.assertEqual(capture['returns'],255)
        for child in proof['children'].values():self.assertIn('flags',child['clobbers'])
        self.assertEqual(proof['children']['action_allowed']['padding_index_excluded'],19)
        self.assertNotIn(4,capture['source_error_bytes'])


class InitialConfiguration(unittest.TestCase):
    @staticmethod
    def change(objects,address,offset,encoded):
        elf=objects.elf;s=elf.named['.data'];raw=bytearray(elf.data);at=s[4]+address-s[3]+offset
        raw[at:at+len(encoded)]=encoded;elf.data=bytes(raw)

    def fixture(self,mode='sizez',ram_shift=0):
        objects,records,item=EntryBodies().fixture(mode)
        # Supplied section/segment fixture. The actual ELF headers are checked
        # separately; these records do not establish live-RAM initialization.
        from config_entry import CONFIG,static_role
        elf=objects.elf
        if ram_shift:
            elf.sections=[tuple(v+ram_shift if i==3 and s[2]&1 else v for i,v in enumerate(s)) for s in elf.sections]
            elf.named={name:elf.sections[item['named'][name]] for name in elf.named}
            elf.symbol_entries=[(n,a+(ram_shift if elf.sections[ix][2]&1 else 0),z,t,ix) for n,a,z,t,ix in elf.symbol_entries]
        s=elf.named['.data'];elf.segments=[(1,s[4],s[3],0x10080000,s[5],s[5],7,8)]
        address=next(a for n,a,z,t,ix in elf.symbol_entries if static_role(n)==CONFIG)
        return objects,address

    def test_exact_masked_load_image_is_not_an_acquired_live_Config(self):
        import importlib.util
        self.assertIsNotNone(importlib.util.find_spec('config_initial'),'missing admitted masked initial-value contract')
        from config_initial import initial_configuration
        for mode in ('speed2','sizes','sizez'):
            for shift in (0,0x4000):
                objects,address=self.fixture(mode,shift);proof=initial_configuration(objects)
                self.assertEqual(proof['address'],address)
                self.assertEqual((proof['extent'],proof['active_bytes'],proof['opaque_bytes']),(14604,185,14419))
                self.assertEqual(proof['phase'],'initial_load_image_only')
                self.assertEqual(len(proof['groups']),15)
                self.assertEqual(proof['initial_collection_lengths'],(0,0))
                self.assertTrue(proof['requires_actual_startup_copy_and_live_preservation'])
                self.assertTrue(proof['requires_actual_successful_once_and_exclusive_source_owner'])
                self.assertTrue(proof['must_not_reuse_after_any_decode_or_refill_write'])
                self.assertNotIn('acquired_owner',proof)

    def test_each_active_byte_rejects_a_different_even_if_type_valid_initial_value(self):
        from config_initial import initial_configuration,_partition
        objects,address=self.fixture();original=objects.elf.data
        active,names=_partition();self.assertEqual(len(active),185)
        for offset,value in active.items():
            objects.elf.data=original;self.change(objects,address,offset,bytes([value^1]))
            with self.subTest(offset=offset),self.assertRaisesRegex(AssertionError,'active constructor value'):
                initial_configuration(objects)
        # Explicit valid-but-stale examples: length1, negative zero, cacheSome,
        # history121 and the alternate initial-status values still reject.
        for offset,encoded in ((244,b'\x01\0\0\0'),(440,b'\x01\0\0\0'),(14576,b'\0\0\0\x80'),
                (14580,b'\x01\0\0\0'),(14528,b'y\0'),(14584,b'\0'),(14584,b'\x02')):
            objects.elf.data=original;self.change(objects,address,offset,encoded)
            with self.subTest(offset=offset,encoded=encoded),self.assertRaisesRegex(AssertionError,'active constructor value'):
                initial_configuration(objects)

    def test_all_named_opaque_bytes_can_change_without_creating_live_elements_or_ownership(self):
        from config_initial import initial_configuration
        from config_initial_contract import OPAQUE
        for mode in ('speed2','sizes','sizez'):
            objects,address=self.fixture(mode);before=initial_configuration(objects)
            for lo,hi in OPAQUE:self.change(objects,address,lo,bytes([0xa5])*(hi-lo))
            after=initial_configuration(objects)
            self.assertEqual(after['initial_collection_lengths'],(0,0));self.assertEqual(after['active_bytes'],185)
            self.assertNotEqual(before['elf_sha256'],after['elf_sha256'])
            self.assertTrue(after['must_not_reuse_after_any_decode_or_refill_write'])
            self.assertEqual(after['phase'],'initial_load_image_only')

    def test_typed_owner_metadata_and_declared_backing_cannot_be_aliased_or_truncated(self):
        from config_initial import initial_configuration
        from config_entry import CONFIG,static_role
        for fault in ('duplicate','alias','size','type','section','alignment','overlap','wrap'):
            objects,address=self.fixture();elf=objects.elf;ix=next(i for i,v in enumerate(elf.symbol_entries) if static_role(v[0])==CONFIG)
            entry=list(elf.symbol_entries[ix])
            if fault=='duplicate':elf.symbol_entries.append(tuple(entry))
            elif fault=='alias':elf.symbol_entries.append(('other_object',address+10,4,1,entry[4]))
            elif fault=='size':entry[2]-=4
            elif fault=='type':entry[3]=2
            elif fault=='section':entry[4]=0
            elif fault=='alignment':entry[1]+=2
            elif fault=='overlap':elf.sections.append((0,8,3,address+7,0,8,0,0,1,0))
            else:
                s=list(elf.sections[entry[4]]);s[5]=0xffffffff;elf.sections[entry[4]]=tuple(s);elf.named['.data']=tuple(s)
            elf.symbol_entries[ix]=tuple(entry)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):initial_configuration(objects)

    def test_load_backing_is_unique_complete_current_and_strictly_numeric(self):
        from config_initial import initial_configuration
        for fault in ('absent','duplicate','partial','offset','file_short','virtual_wrap','physical_wrap','flags','bool'):
            objects,address=self.fixture();elf=objects.elf;segment=list(elf.segments[0])
            if fault=='absent':elf.segments=[]
            elif fault=='duplicate':elf.segments.append(tuple(segment))
            elif fault=='partial':elf.segments.append((1,0,address+5,0x10000000,1,1,7,1))
            elif fault=='offset':segment[1]+=4
            elif fault=='file_short':segment[4]=address-segment[2]+14603
            elif fault=='virtual_wrap':segment[5]=0xffffffff
            elif fault=='physical_wrap':segment[3]=0xfffffff0
            elif fault=='flags':segment[6]=3
            else:segment[7]=True
            if fault not in ('absent','duplicate','partial'):elf.segments=[tuple(segment)]
            with self.subTest(fault=fault),self.assertRaises(AssertionError):initial_configuration(objects)

    def test_prewrite_image_record_cannot_make_a_later_active_mutation_match(self):
        from config_initial import initial_configuration
        objects,address=self.fixture();proof=initial_configuration(objects)
        self.change(objects,address,14570,b'\x04\0')
        with self.assertRaisesRegex(AssertionError,'active constructor value'):initial_configuration(objects)
        self.assertTrue(proof['must_not_reuse_after_any_decode_or_refill_write'])
        self.assertTrue(proof['requires_first_batch_caller_specialized_loader_and_alias_closure'])
        self.assertTrue(proof['requires_complete_validated_outcome_before_publication'])

    def test_fixed_partition_cannot_omit_overlap_or_inject_an_active_group(self):
        import config_initial
        from unittest.mock import patch
        groups=config_initial.GROUPS;opaque=config_initial.OPAQUE
        for changed in (groups[:-1],groups[:-1]+(groups[0],),groups+(("injected",1,1,0,((0,'00'),)),)):
            with patch.object(config_initial,'GROUPS',changed),self.assertRaises(AssertionError):config_initial._partition()
        with patch.object(config_initial,'OPAQUE',((0,1),)+opaque),self.assertRaises(AssertionError):config_initial._partition()
        bad=list(groups);name,base,count,stride,fields=bad[0];bad[0]=(name,base,True,stride,fields)
        with patch.object(config_initial,'GROUPS',tuple(bad)),self.assertRaises(AssertionError):config_initial._partition()


class ConfigurationEntryBoundary(unittest.TestCase):
    def fixture(self,mode='sizez'):
        objects,rows,item=EntryBodies().fixture(mode)
        s=objects.elf.named['.data'];objects.elf.segments=[(1,s[4],s[3],0x10080000,s[5],s[5],7,8)]
        return objects,rows,item

    def relocated(self,mode,delta):
        from config_entry import boot_entry
        from config_entry_contracts import ENTRY_GRAMMARS
        objects,rows,item=self.fixture(mode);elf=objects.elf;proof=boot_entry(objects,rows);raw=bytearray(elf.data)
        def put(address,encoded):
            s=next(s for s in elf.sections if s[1]==1 and s[3]<=address<address+len(encoded)<=s[3]+s[5])
            offset=s[4]+address-s[3];raw[offset:offset+len(encoded)]=encoded
        def mov_imm(operation,register,value):
            first=(0xf240 if operation=='movw' else 0xf2c0)|((value>>12)&15)|(((value>>11)&1)<<10)
            second=(((value>>8)&7)<<12)|(int(register[1:])<<8)|(value&255)
            return struct.pack('<HH',first,second)
        for phase,body in proof['phases'].items():
            contract=ENTRY_GRAMMARS[body['shape']];static={i for i,v in enumerate(contract['objects']) if v['kind']=='entry_static'}
            for offset,width,text in contract['instructions']:
                p=re.search(r'@object([0-9]+)\.(low|high)',text)
                if p is None or int(p[1]) not in static:continue
                address=body['function']+offset;row=rows[address];register=row.operands.partition(',')[0]
                original=body['objects'][int(p[1])];shift=0 if p[2]=='low' else 16
                self.assertEqual(row.raw,mov_imm(row.mnemonic,register,(original>>shift)&0xffff))
                value=((original+delta)>>shift)&0xffff;encoded=mov_imm(row.mnemonic,register,value);put(address,encoded)
                rows[address]=Instruction(address,encoded,row.mnemonic,register+', #'+hex(value))
            for offset,width,value in contract['data']:
                if value.startswith('@object') and int(value[7:]) in static:
                    put(body['function']+offset,struct.pack('<I',body['objects'][int(value[7:])]+delta))
        elf.data=bytes(raw)
        writable={i for i,s in enumerate(elf.sections) if s[2]&1}
        elf.sections=[tuple(v+delta if i==3 and ix in writable else v for i,v in enumerate(s)) for ix,s in enumerate(elf.sections)]
        elf.named={name:elf.sections[item['named'][name]] for name in elf.named}
        elf.symbol_entries=[(n,a+(delta if ix in writable else 0),z,t,ix) for n,a,z,t,ix in elf.symbol_entries]
        s=elf.named['.data'];elf.segments=[(1,s[4],s[3],0x10080000,s[5],s[5],7,8)]
        return objects,rows,item

    def test_actual_once_first_batch_and_loader_tail_keep_ownership_unresolved(self):
        import importlib.util
        self.assertIsNotNone(importlib.util.find_spec('config_owner_entry'),'missing finite actual entry/once/loader boundary')
        from config_owner_entry import configuration_entry_boundary
        for mode,frame in (('speed2',1288),('sizes',696),('sizez',456)):
            objects,rows,item=self.fixture(mode);proof=configuration_entry_boundary(objects,rows)
            self.assertEqual(proof['mode'],mode)
            self.assertEqual(proof['phase'],'once_loader_boundary_pending_startup')
            self.assertEqual(proof['loader']['frame'],frame)
            self.assertEqual(proof['once']['taken_address'],proof['initial']['address']+14600)
            self.assertEqual(proof['once']['success']['predicate'],('loaded_taken_zero','exclusive_store_succeeded'))
            self.assertEqual(proof['once']['retry']['predicate'],'exclusive_store_failed')
            self.assertTrue(proof['once']['already_taken']['requires_nonreturn_diagnostic_closure'])
            self.assertEqual(proof['loader']['incoming_r0'],proof['once']['taken_address'])
            self.assertNotEqual(proof['loader']['incoming_r0'],proof['initial']['address'])
            self.assertEqual(proof['loader']['initialize_frame_live_on_tail'],0)
            self.assertTrue(proof['requires_actual_startup_preservation_and_no_prior_Config_alias'])
            self.assertTrue(proof['requires_specialized_loader_reconstruction_and_all_internal_effects'])
            self.assertNotIn('acquired_owner',proof)

    def test_independent_Config_and_task_storage_relocation_updates_every_selected_reference(self):
        from config_owner_entry import configuration_entry_boundary
        for mode in ('speed2','sizes','sizez'):
            objects,rows,item=self.relocated(mode,0x4000);proof=configuration_entry_boundary(objects,rows)
            self.assertTrue(all(v[3]==proof['initial']['address'] for v in proof['actual_Config_references']))
            self.assertEqual(proof['loader']['incoming_r0'],proof['initial']['address']+14600)
            self.assertEqual(proof['phase'],'once_loader_boundary_pending_startup')

    def test_semantic_foreign_direct_entry_cannot_bypass_once_load_retry_or_restore(self):
        from config_owner_entry import configuration_entry_boundary
        from config_entry import boot_entry
        for operation in ('b','bl','cbz','cbnz'):
            for where in ('entry','store','restore','data'):
                objects,rows,item=self.fixture();entry=boot_entry(objects,rows);start=entry['initialize']
                target=start+dict(entry=0,store=20,restore=28,data=48)[where]
                # Semantic-record target proof, not a newly decoded opcode.
                source=entry['phases']['configuration_initialize']['ordinary_edges'][0][2]+200
                operand=('r0, ' if operation.startswith('cb') else '')+hex(target)
                rows[source]=Instruction(source,b'\x00\xbf',operation,operand)
                with self.subTest(operation=operation,where=where),self.assertRaisesRegex(AssertionError,'foreign direct ingress'):
                    configuration_entry_boundary(objects,rows)

    def test_direct_ingress_rejects_malformed_target_positions_and_suffixes(self):
        from config_owner_entry import configuration_entry_boundary
        from config_entry import boot_entry
        operations=('b','bl','bne.w','bleq.w','cbz','cbnz','cbz.w','cbnz.w')
        for operation in operations:
            for fault in ('unresolved_annotation','extra_target','trailing_comma','trailing_text',
                          'unclosed_annotation','multiple_annotations','overflow','newline'):
                objects,rows,item=self.fixture();entry=boot_entry(objects,rows)
                inside=entry['initialize']+20
                source=entry['phases']['configuration_initialize']['ordinary_edges'][0][2]+200
                outside=source+8
                target=dict(unresolved_annotation=f'missing_target <{outside:#x} >',
                    extra_target=f'{inside:#x}, {outside:#x}',trailing_comma=f'{outside:#x},',
                    trailing_text=f'{outside:#x} garbage',unclosed_annotation=f'{outside:#x} <symbol',
                    multiple_annotations=f'{outside:#x} <symbol> <other>',overflow='0x100000000',
                    newline=f'{outside:#x} <symbol\nother>')[fault]
                operand=('r0, ' if operation.startswith('cb') else '')+target
                rows[source]=Instruction(source,b'\x00\xbf',operation,operand)
                with self.subTest(operation=operation,fault=fault),self.assertRaisesRegex(AssertionError,
                        'unresolved direct configuration claim ingress'):
                    configuration_entry_boundary(objects,rows)
        for register in ('r8','r15','pc','sp','R0','missing','r0, r1'):
            objects,rows,item=self.fixture();entry=boot_entry(objects,rows)
            source=entry['phases']['configuration_initialize']['ordinary_edges'][0][2]+200
            rows[source]=Instruction(source,b'\x00\xbf','cbnz',register+', '+hex(source+8))
            with self.subTest(register=register),self.assertRaisesRegex(AssertionError,
                    'unresolved direct configuration claim ingress'):
                configuration_entry_boundary(objects,rows)

    def test_complete_direct_operands_keep_optional_annotations_and_conditional_forms(self):
        from config_owner_entry import configuration_entry_boundary
        from config_entry import boot_entry
        for operation in ('b','bl','bne.w','bleq.w','cbz','cbnz','cbz.w','cbnz.w'):
            for annotation in ('',' <symbol>',' <generic<type>::method+0x4>'):
                objects,rows,item=self.fixture();entry=boot_entry(objects,rows)
                source=entry['phases']['configuration_initialize']['ordinary_edges'][0][2]+200
                prefix='r7, ' if operation.startswith('cb') else ''
                rows[source]=Instruction(source,b'\x00\xbf',operation,prefix+hex(source+8)+annotation)
                proof=configuration_entry_boundary(objects,rows)
                self.assertEqual(proof['phase'],'once_loader_boundary_pending_startup')
                # The same supported form must still check its actual target.
                rows[source]=Instruction(source,b'\x00\xbf',operation,prefix+hex(entry['initialize']+20)+annotation)
                with self.subTest(operation=operation,annotation=annotation),self.assertRaisesRegex(
                        AssertionError,'foreign direct ingress'):
                    configuration_entry_boundary(objects,rows)

    def test_coherent_once_store_retry_and_normal_return_mutations_fail_complete_source_shape(self):
        from config_owner_entry import configuration_entry_boundary
        from config_entry import boot_entry
        for offset,encoded,operation,operand in ((12,bytes.fromhex('0021'),'movs','r1, #0x0'),
                (26,bytes.fromhex('f8d0'),'beq',None),
                (28,bytes.fromhex('bde88080'),'pop.w','{r7, pc}')):
            objects,rows,item=self.fixture();start=boot_entry(objects,rows)['initialize'];address=start+offset
            s=next(s for s in objects.elf.sections if s[1]==1 and s[3]<=address<address+len(encoded)<=s[3]+s[5])
            raw=bytearray(objects.elf.data);at=s[4]+address-s[3];raw[at:at+len(encoded)]=encoded;objects.elf.data=bytes(raw)
            old=rows[address];rows[address]=Instruction(address,encoded,operation,old.operands if operand is None else operand)
            with self.subTest(offset=offset),self.assertRaises(AssertionError):configuration_entry_boundary(objects,rows)

    def test_wrong_loader_frame_and_stale_initial_taken_value_cannot_issue_a_boundary(self):
        from config_owner_entry import configuration_entry_boundary
        from config_entry import boot_entry
        objects,rows,item=self.fixture();entry=boot_entry(objects,rows)
        loader=next(e[2] for e in entry['phases']['configuration_initialize']['ordinary_edges'] if 'load_at_boot' in e[3])
        objects.sizes[loader]+=8
        with self.assertRaisesRegex(AssertionError,'loader tail/frame'):configuration_entry_boundary(objects,rows)
        objects,rows,item=self.fixture();from config_initial import initial_configuration
        initial=initial_configuration(objects);InitialConfiguration.change(objects,initial['address'],14600,b'\x01')
        with self.assertRaisesRegex(AssertionError,'active constructor value'):configuration_entry_boundary(objects,rows)


class StartupConfigurationCopy(unittest.TestCase):
    def fixture(self,mode='sizez',delta=0):
        base=ConfigurationEntryBoundary()
        objects,rows,item=base.relocated(mode,delta) if delta else base.fixture(mode);elf=objects.elf
        part=next(v for v in json.loads((Path(__file__).parent/'fixtures/config-startup.json').read_text()) if v['mode']==mode)
        # Synthetic metadata namespace, not a reparsed linked ELF. Reserve a
        # real 52-byte ELF header prefix so e_entry and section bytes coexist.
        prefix=bytes.fromhex(part['header']);elf.data=prefix+elf.data
        elf.sections=[tuple(v+52 if i==4 else v for i,v in enumerate(s)) for s in elf.sections]
        elf.named={name:elf.sections[item['named'][name]] for name in elf.named}
        elf.named.update({name:elf.sections[index] for name,index in part['named'].items()})
        raw=bytearray(elf.data)
        for ix,address,encoded in part['segments']:
            s=elf.sections[ix];at=s[4]+address-s[3];raw[at:at+len(bytes.fromhex(encoded))]=bytes.fromhex(encoded)
        elf.data=bytes(raw)
        elf.functions.update({n:tuple(v) for n,v in part['functions'].items()})
        writable={i for i,s in enumerate(elf.sections) if s[2]&1}
        symbols=[(n,a+(delta if ix in writable else 0),z,t,ix) for n,a,z,t,ix in part['symbol_entries']]
        elf.symbol_entries.extend(v for v in symbols if v not in elf.symbol_entries)
        elf.mapping_symbols.extend(tuple(v) for v in part['mapping_symbols'] if tuple(v) not in elf.mapping_symbols)
        objects.sizes.update({int(a):v for a,v in part['sizes'].items()})
        for v in part['records']:rows[v['address']]=Instruction(v['address'],bytes.fromhex(v['bytes']),v['mnemonic'],v['operands'])
        if delta:
            reset=elf.functions['Reset'][0]&~1
            for offset,name in zip((4,6,18,20),('__sbss','__ebss','__sdata','__edata')):
                row=rows[reset+offset];imm=int(row.operands.split('#',1)[1].split(']',1)[0],0)
                slot=((row.address+4)&~3)+imm
                self.put(objects,slot,struct.pack('<I',next(a for n,a,z,t,ix in elf.symbol_entries if n==name)))
        sidata=next(a for n,a,z,t,ix in elf.symbol_entries if n=='__sidata')
        elf.segments=[]
        for s in elf.sections:
            if not s[2]&2:continue
            physical=sidata if s==elf.named['.data'] else s[3]
            flags=4|(2 if s[2]&1 else 0)|(1 if s[2]&4 else 0)
            elf.segments.append((1,s[4],s[3],physical,s[5] if s[1]==1 else 0,s[5],flags,s[8]))
        return objects,rows,item

    @staticmethod
    def put(objects,address,encoded):
        elf=objects.elf
        sections=[s for s in elf.sections if s[1]==1 and s[3]<=address<address+len(encoded)<=s[3]+s[5]]
        if len(sections)!=1:raise AssertionError('test write lacks unique section')
        s=sections[0];at=s[4]+address-s[3];data=bytearray(elf.data);data[at:at+len(encoded)]=encoded;elf.data=bytes(data)

    def move_source(self,objects,rows,delta):
        elf=objects.elf;old=next(a for n,a,z,t,ix in elf.symbol_entries if n=='__sidata');new=old+delta
        elf.symbol_entries=[(n,new if n=='__sidata' else a,z,t,ix) for n,a,z,t,ix in elf.symbol_entries]
        elf.segments=[tuple(v+delta if i==3 and s[2]==elf.named['.data'][3] else v for i,v in enumerate(s)) for s in elf.segments]
        reset=elf.functions['Reset'][0]&~1;row=rows[reset+22]
        immediate=int(row.operands.split('#',1)[1].split(']',1)[0],0);slot=((row.address+4)&~3)+immediate
        self.put(objects,slot,struct.pack('<I',new))

    @staticmethod
    def branch_link(at,target):
        offset=target-at-4
        if offset%2 or not -(1<<24)<=offset<(1<<24):raise AssertionError('test BL range')
        value=offset&0x1ffffff;s=(value>>24)&1;i1=(value>>23)&1;i2=(value>>22)&1
        return struct.pack('<HH',0xf000|(s<<10)|((value>>12)&1023),
            0xd000|((1^i1^s)<<13)|((1^i2^s)<<11)|((value>>1)&2047))

    def move_hook(self,objects,rows,address=0x10071000):
        elf=objects.elf;old,extent,index=elf.functions['__pre_init'];old&=~1;s=elf.sections[index]
        at=s[4]+old-s[3];code=elf.data[at:at+extent];newrows={}
        for at,row in list(rows.items()):
            if not old<=at<old+extent:continue
            operand=row.operands.split('@',1)[0].split('<',1)[0].strip()
            if row.mnemonic.startswith(('b','cb')) and row.mnemonic!='bx':
                prefix,comma,target=operand.rpartition(', ')
                target=int(target if comma else operand,16);self.assertTrue(old<=target<old+extent)
                operand=(prefix+', ' if comma else '')+hex(target+address-old)
            newrows[address+at-old]=Instruction(address+at-old,row.raw,row.mnemonic,operand);del rows[at]
        file_at=(len(elf.data)+3)&~3;elf.data+=bytes(file_at-len(elf.data))+code;index=len(elf.sections)
        elf.sections.append((0,1,6,address,file_at,extent,0,0,4,0))
        elf.functions['__pre_init']=(address|1,extent,index)
        elf.symbol_entries=[(n,address|1,z,t,index) if n=='__pre_init' else (n,a,z,t,ix) for n,a,z,t,ix in elf.symbol_entries]
        elf.mapping_symbols.append(('$t',address,index))
        if extent==60:elf.mapping_symbols.append(('$d',address+48,index))
        elf.segments.append((1,file_at,address,address,extent,extent,5,4));rows.update(newrows)
        objects.sizes[address]=objects.sizes.pop(old)
        reset=elf.functions['Reset'][0]&~1;row=rows[reset]
        original=int(row.operands.split('@',1)[0].split('<',1)[0].strip(),16)
        self.assertEqual(row.raw,self.branch_link(reset,original))
        encoded=self.branch_link(reset,address);self.put(objects,reset,encoded)
        rows[reset]=Instruction(reset,encoded,'bl',hex(address))

    def move_Reset(self,objects,rows,address=0x10070000):
        # Independent complete code + external-pool placement. The new section
        # is a synthetic coherent namespace; no new linked/decoded ELF claim.
        elf=objects.elf;old=elf.functions['Reset'][0]&~1
        start,extent,index=elf.functions['Reset'];s=elf.sections[index];file_start=s[4]+old-s[3]
        code=bytearray(elf.data[file_start:file_start+62]);pools=[];newrows={}
        for at,row in list(rows.items()):
            if not old<=at<old+62:continue
            offset=at-old;operand=row.operands.split('@',1)[0].split('<',1)[0].strip();encoded=row.raw
            if row.mnemonic=='ldr' and '[pc,' in operand:
                register=operand.partition(',')[0];immediate=int(operand.split('#',1)[1].split(']',1)[0],0)
                slot=((at+4)&~3)+immediate
                s=next(s for s in elf.sections if s[1]==1 and s[3]<=slot<slot+4<=s[3]+s[5])
                value=elf.data[s[4]+slot-s[3]:s[4]+slot-s[3]+4];pools.append(value)
                newslot=address+80+4*(len(pools)-1);imm=newslot-((address+offset+4)&~3)
                encoded=struct.pack('<H',0x4800|(int(register[1:])<<8)|(imm//4))
                operand=register+', [pc, #'+hex(imm)+']'
            elif row.mnemonic=='bl':
                target=int(operand,16);self.assertEqual(encoded,self.branch_link(at,target));encoded=self.branch_link(address+offset,target)
            elif row.mnemonic.startswith('b'):
                target=int(operand,16);self.assertTrue(old<=target<old+62);operand=hex(target+address-old)
            code[offset:offset+len(encoded)]=encoded
            newrows[address+offset]=Instruction(address+offset,encoded,row.mnemonic,operand)
            del rows[at]
        self.assertEqual(len(pools),6)
        body=bytes(code)+bytes(18)+b''.join(pools);file_at=(len(elf.data)+3)&~3
        elf.data+=bytes(file_at-len(elf.data))+body;index=len(elf.sections)
        elf.sections.append((0,1,6,address,file_at,len(body),0,0,4,0))
        elf.functions['Reset']=(address|1,62,index)
        elf.symbol_entries=[(n,address|1,z,t,index) if n=='Reset' else (n,a,z,t,ix) for n,a,z,t,ix in elf.symbol_entries]
        elf.mapping_symbols.extend((('$t',address,index),('$d',address+80,index)))
        elf.segments.append((1,file_at,address,address,len(body),len(body),5,4));rows.update(newrows)
        vector=elf.named['.vector_table'][3];self.put(objects,vector+4,struct.pack('<I',address|1))
        data=bytearray(elf.data);struct.pack_into('<I',data,24,address|1);elf.data=bytes(data)

    def test_independent_Reset_external_pool_and_data_LMA_relocations_preserve_the_bridge(self):
        from config_startup import startup_configuration_copy
        for mode in ('speed2','sizes','sizez'):
            for move in ('Reset','source','RAM','hook','both','all'):
                objects,rows,item=self.fixture(mode,0x4000 if move in ('RAM','all') else 0)
                if move in ('hook','all'):self.move_hook(objects,rows)
                if move in ('source','both','all'):self.move_source(objects,rows,0x100000)
                if move in ('Reset','both','all'):self.move_Reset(objects,rows)
                proof=startup_configuration_copy(objects,rows)
                self.assertEqual(proof['phase'],'machine_copy_at_main_pending_application_owner')
                self.assertTrue(proof['requires_M2_pre_init_source_reconciliation'])
                if move in ('Reset','both','all'):
                    self.assertEqual(proof['reset']['function'],0x10070000)
                    self.assertEqual(proof['reset']['external_literals'][0][1],0x10070050)
                if move in ('source','both','all'):
                    self.assertGreaterEqual(proof['copy']['source'][0],0x10100000)

    def test_copy_literals_and_complete_Reset_instructions_cannot_be_skipped_or_retargeted(self):
        from config_startup import startup_configuration_copy
        for fault in ('source_pool','destination_pool','changed_zero_store','changed_copy_load','skip_hook','extra_frame'):
            objects,rows,item=self.fixture();reset=objects.elf.functions['Reset'][0]&~1
            if fault.endswith('pool'):
                row=rows[reset+(22 if fault=='source_pool' else 18)]
                imm=int(row.operands.split('#',1)[1].split(']',1)[0],0);slot=((row.address+4)&~3)+imm
                self.put(objects,slot,struct.pack('<I',0x10000000 if fault=='source_pool' else 0x20000004))
            elif fault=='extra_frame':objects.sizes[reset]=8
            else:
                offset,encoded,op,args=dict(changed_zero_store=(14,'01c0','stm','r0!, {r0}'),
                    changed_copy_load=(28,'04c9','ldm','r1!, {r2}'),skip_hook=(0,'00f000b8','b.w',hex(reset+4)))[fault]
                at=reset+offset;self.put(objects,at,bytes.fromhex(encoded));rows[at]=Instruction(at,bytes.fromhex(encoded),op,args)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):startup_configuration_copy(objects,rows)

    def test_mapping_load_vector_and_saved_stack_boundaries_are_not_caller_assertions(self):
        from config_startup import startup_configuration_copy
        for fault in ('pool_code','physical_alias','load_short','source_offset','source_wrap','vector','header',
                      'pre_init_stack','data_alias','bss_alignment'):
            objects,rows,item=self.fixture();elf=objects.elf;data=elf.named['.data']
            if fault=='pool_code':
                reset=elf.functions['Reset'][0]&~1;index=elf.functions['Reset'][2]
                elf.mapping_symbols=[(n,a,ix) for n,a,ix in elf.mapping_symbols if not (ix==index and reset+62<=a<=reset+88)]
            elif fault in ('physical_alias','load_short','source_offset','source_wrap'):
                ix=next(i for i,s in enumerate(elf.segments) if s[2]==data[3]);segment=list(elf.segments[ix])
                if fault=='physical_alias':elf.segments.append((1,segment[1],0x10090000,segment[3]+4,8,8,4,4))
                elif fault=='load_short':segment[4]-=4
                elif fault=='source_offset':segment[1]+=4
                else:segment[3]=0xfffffff0
                elf.segments[ix]=tuple(segment)
            elif fault=='vector':self.put(objects,elf.named['.vector_table'][3]+4,struct.pack('<I',0x10000143))
            elif fault=='header':
                raw=bytearray(elf.data);struct.pack_into('<I',raw,24,0x10000143);elf.data=bytes(raw)
            elif fault=='pre_init_stack':elf.sections.append((0,8,3,0x2007fffc,0,4,0,0,4,0))
            elif fault=='data_alias':elf.sections.append((0,8,3,data[3]+4,0,4,0,0,4,0))
            else:
                elf.symbol_entries=[(n,a+2 if n=='__sbss' else a,z,t,ix) for n,a,z,t,ix in elf.symbol_entries]
            with self.subTest(fault=fault),self.assertRaises(AssertionError):startup_configuration_copy(objects,rows)

    def test_complete_hook_frame_restore_and_fixed_MMIO_literals_remain_exact(self):
        from config_startup import startup_configuration_copy
        for fault in ('hook_frame','hook_restore','hook_pool','Reset_bool_frame'):
            objects,rows,item=self.fixture();elf=objects.elf;hook=elf.functions['__pre_init'][0]&~1
            if fault=='hook_frame':objects.sizes[hook]=16
            elif fault=='Reset_bool_frame':objects.sizes[elf.functions['Reset'][0]&~1]=False
            elif fault=='hook_pool':self.put(objects,hook+48,struct.pack('<I',0xd0000178))
            else:
                # POP {r7,pc} -> BX LR: a real two-byte instruction which does
                # not restore the eight-byte hook activation.
                self.put(objects,hook+46,bytes.fromhex('7047'))
                rows[hook+46]=Instruction(hook+46,bytes.fromhex('7047'),'bx','lr')
            with self.subTest(fault=fault),self.assertRaises(AssertionError):startup_configuration_copy(objects,rows)

    def test_whole_data_destination_cannot_gain_a_second_virtual_load_outside_Config(self):
        from config_startup import startup_configuration_copy
        objects,rows,item=self.fixture();elf=objects.elf;data=elf.named['.data']
        # This four-byte alias is outside Config, so the initial-value object's
        # narrower backing check deliberately does not discharge the copy range.
        segment=next(s for s in elf.segments if s[2]==data[3])
        elf.segments.append((1,segment[1],data[3]+data[5]-4,0x10090000,4,4,6,4))
        with self.assertRaisesRegex(AssertionError,'competing virtual load overlaps initialized data'):
            startup_configuration_copy(objects,rows)

    def test_main_target_and_live_initial_active_bytes_are_rechecked_by_composition(self):
        from config_startup import startup_configuration_copy
        from config_initial import initial_configuration
        for fault in ('main_target','initial_cache'):
            objects,rows,item=self.fixture();elf=objects.elf
            if fault=='initial_cache':
                initial=initial_configuration(objects)
                InitialConfiguration.change(objects,initial['address'],14584,b'\x00')
            else:
                at=(elf.functions['Reset'][0]&~1)+56;target=elf.functions['__pre_init'][0]&~1
                encoded=self.branch_link(at,target);self.put(objects,at,encoded)
                rows[at]=Instruction(at,encoded,'bl',hex(target))
            with self.subTest(fault=fault),self.assertRaises(AssertionError):startup_configuration_copy(objects,rows)
        objects,rows,item=self.fixture();initial=initial_configuration(objects)
        InitialConfiguration.change(objects,initial['address'],300,b'\xa5')
        proof=startup_configuration_copy(objects,rows)
        self.assertEqual(proof['phase'],'machine_copy_at_main_pending_application_owner')
        self.assertNotIn('acquired_owner',proof)

    def test_foreign_semantic_direct_entries_cannot_skip_machine_copy_or_hook(self):
        from config_startup import startup_configuration_copy
        for phase in ('Reset','__pre_init','main'):
            for interior in (0,2):
                objects,rows,item=self.fixture();source=objects.elf.functions['Reset'][0]&~1
                source=next(a for n,(a,z,ix) in objects.elf.functions.items() if 'load_at_boot' in n)&~1;source+=200
                target=(objects.elf.functions[phase][0]&~1)+interior
                rows[source]=Instruction(source,b'\x00\xbf','bne.w',hex(target))
                with self.subTest(phase=phase,interior=interior),self.assertRaisesRegex(
                        AssertionError,'foreign direct ingress bypasses startup'):
                    startup_configuration_copy(objects,rows)

    def test_current_copy_to_main_composes_the_same_once_Config_without_source_theorem(self):
        import importlib.util
        self.assertIsNotNone(importlib.util.find_spec('config_startup'),'missing admitted finite Reset copy bridge')
        from config_startup import startup_configuration_copy
        for mode in ('speed2','sizes','sizez'):
            objects,rows,item=self.fixture(mode);proof=startup_configuration_copy(objects,rows)
            self.assertEqual(proof['mode'],mode)
            self.assertEqual(proof['phase'],'machine_copy_at_main_pending_application_owner')
            self.assertEqual(proof['copy']['Config_address'],proof['entry']['initial']['address'])
            self.assertEqual(proof['reset']['frame'],0);self.assertEqual(proof['pre_init']['frame'],8)
            self.assertEqual(proof['copy']['step'],4)
            self.assertEqual(proof['source_qualification'],'deprecated_Rust_pre_init_emitted_machine_only')
            self.assertTrue(proof['requires_M2_pre_init_source_reconciliation'])
            self.assertTrue(proof['requires_first_batch_source_alias_and_specialized_loader_closure'])
            self.assertFalse(proof['hardware_qualified']);self.assertNotIn('acquired_owner',proof)


class LedPlatformFamilies(unittest.TestCase):
    def fixture(self,role,mode):
        from config_led_access import LedContext,LedFamily,SITE_SHAPES
        from config_regions import Storage,borrow
        parents={('result_words','speed2'):0x1003fbfc,('result_words','sizes'):0x100225c2,
            ('result_words','sizez'):0x1001b9ca,('pattern_words','sizez'):0x1001c654,
            ('normal_config_words','sizez'):0x1001c654}
        contexts=[];epochs=('normal','warning') if mode=='sizez' and role!='normal_config_words' else ('settings',)
        for epoch in epochs:
            phase='decoded_led_'+epoch;extent={'speed2':1288,'sizes':696,'sizez':456}[mode]
            loader=Storage('stack:configuration.default','loader',0,extent,8,phase,True)
            config=Storage('address','private unpublished ConfigSlot',{'speed2':0x2000064c,'sizes':0x20000388,'sizez':0x2000031c}[mode],14604,4,phase,True)
            visitor=Storage(loader.space,'Config visitor',-1608,1608,8,phase,True) if mode=='sizez' else None
            if mode=='sizez':
                frame=Storage(loader.space,'LED pattern',-1656,48,8,phase,True) if role=='result_words' else visitor
                owner=borrow(config,14544,10,phase,writable=True) if role=='normal_config_words' else borrow(visitor,1064,10,phase,writable=role=='result_words')
                saved=Storage(loader.space,'visitor saves',-96,96,8,phase,False)
                protected=(saved,)
                if role=='result_words':protected+=(Storage(loader.space,'LED pattern saves',-1640,32,8,phase,False),)
            else:
                frame=Storage(loader.space,'LED aggregate',-80,80,8,phase,True)
                owner=borrow(loader,996 if mode=='speed2' else 408,26,phase,writable=True)
                protected=(Storage(loader.space,'LED aggregate saves',-36,36,4,phase,False),)
            protected+=(Storage(loader.space,'loader saves',extent-104,104,4,phase,False),)
            contexts.append(LedContext(mode,epoch,loader,frame,borrow(config,0,14576,phase),owner,protected))
        parent=parents[role,mode]
        sites=tuple((parent+offset,raw,field,epoch) for offset,raw,field,epoch in SITE_SHAPES[role,mode])
        return LedFamily(role,mode,parent,sites,tuple(contexts))

    def dependencies(self,mode):
        roles=('result_words','pattern_words','normal_config_words') if mode=='sizez' else ('result_words',)
        return tuple(self.fixture(role,mode) for role in roles)

    def test_all12_LED_sites_keep_exact_modes_owners_epochs_and_strict_roundtrip(self):
        import importlib.util
        self.assertIsNotNone(importlib.util.find_spec('config_led_access'),'missing closed decoded LED dependency consumer')
        from config_led_access import read_led_family
        expected={'speed2':2,'sizes':2,'sizez':8};total=0
        for mode,count in expected.items():
            values=self.dependencies(mode);self.assertEqual(sum(len(v.sites) for v in values),count);total+=count
            for value in values:
                record=value.to_record();self.assertEqual(read_led_family(json.loads(json.dumps(record))),value)
                self.assertEqual(record['width'],4)
                self.assertEqual(record['source_phase'],'decoded_LED_source_success_still_requires_actual_producer')
                self.assertNotIn('source_proved',record)
                for context in value.contexts:
                    self.assertTrue(value.footprints(context.epoch))
        self.assertEqual(total,12)


    def test_LED_reader_rejects_wrong_mode_width_output_epoch_and_padding_aliases(self):
        from config_led_access import read_led_family
        from copy import deepcopy
        for role,mode in (('result_words','speed2'),('result_words','sizez'),('pattern_words','sizez'),('normal_config_words','sizez')):
            record=self.fixture(role,mode).to_record()
            for fault in ('width_float','site_bool','offset_float','direction','mode','duplicate','lost_epoch','owner',
                          'frame','save','protected_overlap','writable_bool','padding','error','unknown'):
                bad=deepcopy(record);c=bad['contexts'][0]
                if fault=='width_float':bad['width']=4.0
                elif fault=='site_bool':bad['sites'][0]['site']=True
                elif fault=='offset_float':bad['sites'][0]['offset']=float(bad['sites'][0]['offset'])
                elif fault=='direction':bad['direction']='write' if role=='pattern_words' else 'read'
                elif fault=='mode':bad['mode']='sizes' if mode=='sizez' else 'sizez'
                elif fault=='duplicate':bad['sites'][-1]=deepcopy(bad['sites'][0])
                elif fault=='lost_epoch':bad['contexts'].pop()
                elif fault=='owner':c['owner']['offset']+=10 if role=='normal_config_words' else 4
                elif fault=='frame':c['frame']['extent']+=8
                elif fault=='save':c['protected'].pop()
                elif fault=='protected_overlap':
                    owner=c['owner'];s=owner['storage'];c['protected'].append(dict(s,identity='overlap',
                        start=s['start']+owner['offset']+2,extent=4,alignment=1,writable=False))
                elif fault=='writable_bool':c['owner']['writable']=int(c['owner']['writable'])
                elif fault=='padding':bad['opaque_payload_padding'][0]=5.0
                elif fault=='error':bad['source_outcome']='anything_except_error_tag'
                else:bad['source_proved']=True
                with self.subTest(role=role,mode=mode,fault=fault),self.assertRaises(AssertionError):read_led_family(bad)

    def test_LED_source_fields_and_reused_result_epochs_cannot_form_cross_products(self):
        from dataclasses import replace
        for role in ('result_words','pattern_words','normal_config_words'):
            value=self.fixture(role,'sizez')
            for epoch in ('error','missing','retired','aggregate'):
                with self.subTest(role=role,epoch=epoch),self.assertRaises(AssertionError):value.footprints(epoch)
            if role=='normal_config_words':
                self.assertEqual(value.footprints('settings'),((0x2000031c+14546,0x2000031c+14550),(0x2000031c+14550,0x2000031c+14554)))
            else:
                self.assertEqual(tuple(sorted(value.footprints('normal'))),((-542,-538),(-538,-534)))
                self.assertEqual(tuple(sorted(value.footprints('warning'))),((-542,-538),(-538,-534)))
                with self.assertRaises(AssertionError):replace(value,contexts=(value.contexts[1],value.contexts[0]))
                c=value.contexts[1]
                other=replace(c.owner,storage=replace(c.owner.storage,identity='another numeric visitor owner'))
                with self.assertRaises(AssertionError):replace(value,contexts=(value.contexts[0],replace(c,owner=other)))
                c=value.contexts[1];owner=replace(c.owner,storage=replace(c.owner.storage,start=c.owner.storage.start-8))
                with self.assertRaises(AssertionError):replace(value,contexts=(value.contexts[0],replace(c,owner=owner)))
        for mode,offset in (('speed2',996),('sizes',408)):
            value=self.fixture('result_words',mode);c=value.contexts[0]
            self.assertEqual(value.footprints('settings'),((offset+2,offset+6),(offset+6,offset+10)))
            with self.assertRaises(AssertionError):replace(value,contexts=(replace(c,owner=replace(c.owner,extent=12)),))

    def artifact(self,mode):
        # Metadata and consumed-site bytes only; no complete LED producer body.
        from config_led_access import _PARENTS
        from config_entry import CONFIG
        values=self.dependencies(mode);start=min(v.parent for v in values);end=max(v.parent+_PARENTS[v.role,v.mode][1] for v in values)
        data=bytearray(end-start);functions={};sizes={}
        for value in values:
            role,extent,frame=_PARENTS[value.role,value.mode];functions[role]=(value.parent|1,extent,0);sizes[value.parent]=frame
            for at,raw,offset,epoch in value.sites:data[at-start:at-start+4]=bytes.fromhex(raw)
        base=values[0].contexts[0].config.start
        text=(0,1,6,start,0,len(data),0,0,2,0);slot=(0,1,7,base,len(data),14604,0,0,4,0)
        elf=SimpleNamespace(data=bytes(data)+bytes(14604),sections=[text,slot],functions=functions,
            symbol_entries=[(CONFIG,base,14604,1,1)],named={'.data':slot},mapping_symbols=[('$t',start,0)])
        return config_stack.DiagnosticObjects(elf,sizes)

    def test_current_LED_consumer_rebinds_all12_sites_and_rejects_metadata_or_bytes(self):
        from config_led_access import current_led_family
        for mode in ('speed2','sizes','sizez'):
            objects=self.artifact(mode);start=objects.elf.sections[0][3]
            for value in self.dependencies(mode):
                self.assertEqual(current_led_family(objects,value),value)
                for at,raw,offset,epoch in value.sites:
                    old=objects.elf.data;changed=bytearray(old);changed[at-start]^=1;objects.elf.data=bytes(changed)
                    with self.subTest(mode=mode,role=value.role,site=hex(at)),self.assertRaisesRegex(AssertionError,'LED operation bytes'):
                        current_led_family(objects,value)
                    objects.elf.data=old
            for fault in ('frame','extent','mapping','Config'):
                objects=self.artifact(mode);value=self.dependencies(mode)[0]
                if fault=='frame':objects.sizes[value.parent]+=8
                elif fault=='extent':
                    name=next(n for n,v in objects.elf.functions.items() if v[0]&~1==value.parent);a,z,ix=objects.elf.functions[name]
                    objects.elf.functions[name]=(a,z-2,ix)
                elif fault=='mapping':objects.elf.mapping_symbols=[('$d',start,0)]
                else:
                    n,a,z,t,ix=objects.elf.symbol_entries[0];objects.elf.symbol_entries=[(n,a,z-4,t,ix)]
                with self.subTest(mode=mode,fault=fault),self.assertRaises(AssertionError):current_led_family(objects,value)

    def test_v5_configuration_policy_cannot_drop_new_LED_families(self):
        import config_platform
        objects,view,context=ConditionalStackPolicy.fixture(self)
        for mode in ('speed2','sizes','sizez'):
            legacy=ValidatorPlatformFamilies.dependencies(ValidatorPlatformFamilies(),mode,include_led=False)
            with self.subTest(mode=mode),self.assertRaises(AssertionError):
                config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=(),owned_dependencies=legacy)


    def test_LED_code_and_owner_relocations_keep_sites_and_lifetimes_paired(self):
        from config_led_access import current_led_family
        from dataclasses import replace
        for mode in ('speed2','sizes','sizez'):
            for movement in ('code','owner','both'):
                objects=self.artifact(mode);elf=objects.elf;code_delta=0x2000 if movement!='owner' else 0
                owner_delta=0x4000 if movement!='code' else 0
                code=list(elf.sections[0]);data=list(elf.sections[1]);code[3]+=code_delta;data[3]+=owner_delta
                elf.sections=[tuple(code),tuple(data)];elf.named={'.data':tuple(data)}
                elf.functions={n:(a+code_delta,z,ix) for n,(a,z,ix) in elf.functions.items()}
                elf.mapping_symbols=[(n,a+code_delta,ix) for n,a,ix in elf.mapping_symbols]
                elf.symbol_entries=[(n,a+owner_delta,z,t,ix) for n,a,z,t,ix in elf.symbol_entries]
                objects.sizes={a+code_delta:z for a,z in objects.sizes.items()}
                for old in self.dependencies(mode):
                    contexts=[]
                    for c in old.contexts:
                        config=replace(c.config,storage=replace(c.config.storage,start=c.config.start+owner_delta))
                        owner=replace(c.owner,storage=config.storage) if c.owner.storage.space=='address' else c.owner
                        contexts.append(replace(c,config=config,owner=owner))
                    value=replace(old,parent=old.parent+code_delta,
                        sites=tuple((a+code_delta,raw,o,e) for a,raw,o,e in old.sites),contexts=tuple(contexts))
                    with self.subTest(mode=mode,movement=movement,role=value.role):
                        self.assertEqual(current_led_family(objects,value),value)
                        with self.assertRaises(AssertionError):current_led_family(objects,old)

    def test_v5_LED_dependencies_cannot_mix_same_width_owners_or_visitor_lineage(self):
        import config_platform
        from config_led_access import LedFamily
        from dataclasses import replace
        objects,view,context=ConditionalStackPolicy.fixture(self)
        for mode in ('speed2','sizes','sizez'):
            dependencies=ValidatorPlatformFamilies.dependencies(ValidatorPlatformFamilies(),mode)
            for fault in ('Config','loader','old_scope')+ (('visitor','parent') if mode=='sizez' else ()):
                if fault=='old_scope':
                    policy=config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=(),owned_dependencies=dependencies)
                    bad=policy.to_record();bad.update(schema=3,scope='h2-rp2350-secure-owned-access-v4',
                        scope_sha256='36881eb967833a9599cc72487f8156bb1a4bb2c8722cd100679e20c519af4dfa')
                    with self.subTest(mode=mode,fault=fault),self.assertRaises(AssertionError):config_platform.read_policy(bad,policy)
                    continue
                values=list(dependencies);index=next(i for i,v in enumerate(values) if isinstance(v,LedFamily) and
                    (v.role=='pattern_words' if fault in ('visitor','parent') else True))
                family=values[index];contexts=[]
                for c in family.contexts:
                    if fault=='Config':
                        config=replace(c.config,storage=replace(c.config.storage,start=c.config.start+4))
                        owner=replace(c.owner,storage=config.storage) if c.owner.storage.space=='address' else c.owner
                        c=replace(c,config=config,owner=owner)
                    elif fault=='loader':
                        loader=replace(c.loader,identity='another loader')
                        owner=replace(c.owner,storage=loader) if c.owner.storage==c.loader else c.owner
                        c=replace(c,loader=loader,owner=owner)
                    elif fault=='visitor':
                        frame=replace(c.frame,identity='another visitor')
                        c=replace(c,frame=frame,owner=replace(c.owner,storage=frame))
                    contexts.append(c)
                changed=replace(family,contexts=tuple(contexts))
                if fault=='parent':changed=replace(changed,parent=changed.parent+0x1000,
                    sites=tuple((a+0x1000,raw,o,e) for a,raw,o,e in changed.sites))
                values[index]=changed
                with self.subTest(mode=mode,fault=fault),self.assertRaises(AssertionError):
                    config_platform.reviewed_eav_policy(objects,view,context,contradictory_effects=(),owned_dependencies=tuple(values))


class PreclaimConfigurationPreservation(unittest.TestCase):
    def fixture(self,mode='sizez',delta=0):
        return StartupConfigurationCopy().fixture(mode,delta)

    def test_same_Reset_application_and_claim_intervals_in_all_compatible_modes(self):
        from config_preclaim import preclaim_configuration_preservation
        for mode,footprint,attempts,count in (('speed2',80,4,46),('sizes',80,1,34),('sizez',96,1,34)):
            for delta in (0,0x4000):
                objects,rows,item=self.fixture(mode,delta)
                proof=preclaim_configuration_preservation(objects,rows)
                self.assertEqual(proof['phase'],'normal_preclaim_preserved_pending_source_diagnostic_and_loader_closure')
                self.assertEqual(proof['mode'],mode)
                self.assertEqual(proof['executor'],(0x2007ffe8,8))
                self.assertEqual(proof['normal_stack_interval'],(0x20080000-footprint,0x20080000))
                self.assertEqual(len(proof['writes']),count)
                self.assertEqual(len(proof['publication_attempts']),attempts)
                config=proof['startup']['entry']['initial']['address']
                self.assertEqual(proof['Config_write_intersections'],((config+14600,1,'claim_transition'),))
                self.assertEqual(proof['claim']['loader_r0'],config+14600)
                self.assertEqual(proof['claim']['after_success_taken'],1)
                self.assertEqual(proof['claim']['failed_exclusive_write_bytes'],0)
                self.assertEqual(proof['callback_expiry'],'before next shared dequeue')
                self.assertTrue(proof['requires_M2_pre_init_source_reconciliation'])
                self.assertFalse(proof['hardware_qualified'])
                self.assertNotIn('acquired_owner',proof)
                self.assertTrue(proof['requires_complete_source_alias_and_diagnostic_closure'])
                self.assertEqual(set(v['kind'] for v in proof['pending_diagnostics']),{'Busy','resumed','already_taken'})
                self.assertTrue(all(len(v['edges'])==1 for v in proof['pending_diagnostics']))
                self.assertEqual(proof['claim']['required_before_taken'],0)

    def test_full_preclaim_stack_rejects_partial_allocations_below_the_old_hook_frame(self):
        from config_preclaim import preclaim_configuration_preservation
        from config_startup import startup_configuration_copy
        for mode in ('speed2','sizes','sizez'):
            for offset in (12,28,44,68,76)+( (92,) if mode=='sizez' else ()):
                objects,rows,item=self.fixture(mode)
                objects.elf.sections.append((0,8,3,0x20080000-offset,0,4,0,0,4,0))
                # Reset's eight-byte hook is disjoint. The newly composed
                # application stack footprint must close this later boundary.
                startup_configuration_copy(objects,rows)
                with self.subTest(mode=mode,offset=offset),self.assertRaisesRegex(AssertionError,'preclaim stack'):
                    preclaim_configuration_preservation(objects,rows)

    def test_stack_cannot_overlap_a_LOAD_memory_interval_without_a_section(self):
        from config_preclaim import preclaim_configuration_preservation
        for mode in ('speed2','sizes','sizez'):
            objects,rows,item=self.fixture(mode);address=0x20080000-(92 if mode=='sizez' else 76)
            # A section-less four-byte PT_LOAD memory extent still occupies
            # the newly composed stack footprint. No file bytes are needed.
            objects.elf.segments.append((1,0,address,address,0,4,6,4))
            with self.subTest(mode=mode),self.assertRaisesRegex(AssertionError,'preclaim stack'):
                preclaim_configuration_preservation(objects,rows)

    def test_independent_Restore_Reset_and_LMA_relocation_do_not_supply_a_stack_address(self):
        from config_preclaim import preclaim_configuration_preservation
        helper=StartupConfigurationCopy()
        for mode in ('speed2','sizes','sizez'):
            objects,rows,item=self.fixture(mode)
            helper.move_Reset(objects,rows);helper.move_source(objects,rows,0x100000);helper.move_hook(objects,rows)
            proof=preclaim_configuration_preservation(objects,rows)
            self.assertEqual(proof['startup']['reset']['function'],0x10070000)
            self.assertEqual(proof['executor'],(proof['startup']['copy']['initial_sp']-24,8))

    def test_external_direct_ingress_cannot_bypass_task_or_executor_construction(self):
        from config_preclaim import preclaim_configuration_preservation
        from config_startup import startup_configuration_copy
        for mode in ('speed2','sizes','sizez'):
            objects,rows,item=self.fixture(mode)
            before=startup_configuration_copy(objects,rows)
            phases=before['entry']['entry']['phases'];source=before['entry']['loader']['function']+200
            for phase in phases:
                if phase in ('main','configuration_initialize'):continue
                for offset in (0,2):
                    candidate=dict(rows);target=phases[phase]['function']+offset
                    candidate[source]=Instruction(source,b'\x00\xbf','bne.w',hex(target))
                    if offset==0 and phase in ('raw_spawn','raw_poll','queue_push','callback_install'):
                        proof=preclaim_configuration_preservation(objects,candidate)
                        self.assertIn((source,'bne.w',phase,target),proof['pending_other_shared_invocations'])
                        self.assertNotIn('acquired_owner',proof)
                    else:
                        with self.subTest(mode=mode,phase=phase,offset=offset),self.assertRaisesRegex(AssertionError,'preclaim.*ingress'):
                            preclaim_configuration_preservation(objects,candidate)

    def test_initializer_and_same_task_callback_effects_cannot_change_under_matching_roles(self):
        from config_preclaim import preclaim_configuration_preservation
        from config_startup import startup_configuration_copy
        for mode in ('speed2','sizes','sizez'):
            for phase,offset in (('boot_initializer',4 if mode=='sizes' else 6 if mode=='sizez' else 12),
                                 ('run',32 if mode=='sizez' else 42 if mode=='sizes' else 28),
                                 ('boot_poll',12),('cortex_main',12 if mode=='sizez' else 14 if mode=='sizes' else 8)):
                objects,rows,item=self.fixture(mode);initial=startup_configuration_copy(objects,rows)
                at=initial['entry']['entry']['phases'][phase]['function']+offset
                row=rows[at];raw=bytes([row.raw[0]^1])+row.raw[1:]
                StartupConfigurationCopy.put(objects,at,raw)
                # Keep the original decoded record: this is a byte/listing
                # identity negative, not a fresh semantic decoder execution.
                with self.subTest(mode=mode,phase=phase),self.assertRaises(AssertionError):
                    preclaim_configuration_preservation(objects,rows)

    def test_coherent_operand_mutations_reject_changed_register_and_queue_provenance(self):
        from config_preclaim import preclaim_configuration_preservation
        from config_startup import startup_configuration_copy
        from dataclasses import replace
        for mode in ('speed2','sizes','sizez'):
            for phase in ('boot_initializer','boot_poll','run','cortex_main'):
                objects,rows,item=self.fixture(mode);proof=startup_configuration_copy(objects,rows)
                offsets=dict(boot_initializer=12 if mode=='speed2' else 4 if mode=='sizes' else 6,
                    boot_poll=12,run=28 if mode=='speed2' else 42 if mode=='sizes' else 32,
                    cortex_main=8 if mode=='speed2' else 14 if mode=='sizes' else 12)
                at=proof['entry']['entry']['phases'][phase]['function']+offsets[phase];row=rows[at]
                raw=bytes([row.raw[0]^1])+row.raw[1:];args=row.operands.split('@',1)[0].strip()
                if phase=='cortex_main':args=args.replace('[sp]','[sp, #0x4]')
                elif phase=='boot_initializer':args=('r1'+args[2:] if mode=='speed2' else 'r0'+args[2:])
                elif phase=='boot_poll':args=args.replace('[r0,','[r1,')
                else:args=args.replace('[r5,','[r4,').replace('[r3]','[r2]')
                self.assertNotEqual(args,row.operands.split('@',1)[0].strip())
                StartupConfigurationCopy.put(objects,at,raw);rows[at]=replace(row,raw=raw,operands=args)
                with self.subTest(mode=mode,phase=phase),self.assertRaises(AssertionError):
                    preclaim_configuration_preservation(objects,rows)

    def test_full_typed_pool_backing_and_initial_claim_are_still_required(self):
        from config_preclaim import preclaim_configuration_preservation
        from config_entry import BOOT_POOL,static_role
        from config_initial import initial_configuration
        for fault in ('pool_extent','pool_alias','initial_taken'):
            objects,rows,item=self.fixture();elf=objects.elf
            if fault=='initial_taken':
                value=initial_configuration(objects);InitialConfiguration.change(objects,value['address'],14600,b'\x01')
            elif fault=='pool_extent':
                elf.symbol_entries=[(n,a,47 if static_role(n)==BOOT_POOL else z,t,ix) for n,a,z,t,ix in elf.symbol_entries]
            else:
                n,a,z,t,ix=next(v for v in elf.symbol_entries if static_role(v[0])==BOOT_POOL)
                elf.symbol_entries.append(('other_allocated_pool',a+20,4,1,ix))
            with self.subTest(fault=fault),self.assertRaises(AssertionError):preclaim_configuration_preservation(objects,rows)


class FirstBootSourceContract(unittest.TestCase):
    def sources(self):
        # Source-only supplied fixtures. Acceptance here does not establish
        # any relationship to an ELF or a compiler-consumed input set.
        root=Path(__file__).resolve().parents[1]/'pico2'/'src'
        return {name:(root/name).read_bytes() for name in ('main.rs','view_config.rs')}

    def test_exact_current_private_module_and_synchronous_boot_contract(self):
        from config_boot_source import first_boot_source_contract
        proof=first_boot_source_contract(self.sources())
        self.assertEqual(proof['kind'],'first_boot_source_semantics_only')
        self.assertEqual(proof['ordered_boot_operations'],('initialize','enqueue_Dashboard'))
        self.assertEqual(proof['Config_visibility'],'private_static_single_take_immutable_return')
        self.assertTrue(proof['requires_checked_build_source_ELF_binding'])
        self.assertTrue(proof['requires_actual_machine_and_first_batch_composition'])
        self.assertNotIn('acquired_owner',proof)

    def test_private_module_cannot_add_an_escape_or_mutable_return(self):
        from config_boot_source import first_boot_source_contract
        for old,new in (('static CONFIGURATION:','pub static CONFIGURATION:'),
            ("-> &'static BootConfig","-> &'static mut BootConfig"),
            ('CONFIGURATION.take().load_at_boot(EMBEDDED)','CONFIGURATION.take().load_at_boot(other())'),
            ('#[inline(never)]','#[inline(always)]')):
            documents=self.sources();text=documents['view_config.rs'].decode();self.assertIn(old,text)
            documents['view_config.rs']=text.replace(old,new).encode()
            with self.subTest(change=new),self.assertRaises(AssertionError):first_boot_source_contract(documents)
        for extra in ('pub fn leak() { CONFIGURATION.take(); }','pub use CONFIGURATION as EXPOSED;',
                      'mod extra;','#[cfg(feature="other")] fn initialize() {}'):
            documents=self.sources();documents['view_config.rs']+=extra.encode()
            with self.subTest(extra=extra),self.assertRaises(AssertionError):first_boot_source_contract(documents)

    def test_boot_body_cannot_escape_spawn_poll_or_reorder_before_initialize(self):
        from config_boot_source import first_boot_source_contract
        original='let configuration = view_config::initialize();'
        for change in ('let configuration = view_config::initialize().await;',
                       'publish(spawner); '+original,'poll_some_task(); '+original,
                       'let configuration = other::initialize();',
                       'spawner.spawn(other_task().unwrap()); '+original):
            documents=self.sources();text=documents['main.rs'].decode();self.assertIn(original,text)
            documents['main.rs']=text.replace(original,change).encode()
            with self.subTest(change=change),self.assertRaises(AssertionError):first_boot_source_contract(documents)

    def test_alternate_or_redirected_source_declarations_are_not_ignored(self):
        from config_boot_source import first_boot_source_contract
        for old,new in (('mod view_config;','#[path="other.rs"] mod view_config;'),
                        ('mod view_config;','pub mod view_config;'),
                        ('#[embassy_executor::main]','#[cfg(feature="other")] #[embassy_executor::main]'),
                        ('#[embassy_executor::main]','#[embassy_executor::main(pool_size=2)]'),
                        ('use embassy_executor::{Executor, Spawner};','use other::{Executor, Spawner};')):
            documents=self.sources();text=documents['main.rs'].decode();self.assertIn(old,text)
            documents['main.rs']=text.replace(old,new).encode()
            with self.subTest(change=new),self.assertRaises(AssertionError):first_boot_source_contract(documents)
        documents=self.sources();documents['main.rs']+=b'\n#[cfg(feature="other")] fn main() {}'
        with self.assertRaises(AssertionError):first_boot_source_contract(documents)

    def test_nested_declarations_cannot_borrow_top_level_source_identity(self):
        from config_boot_source import first_boot_source_contract
        for target in ('module','boot_main','executor_import'):
            documents=self.sources();main=documents['main.rs'].decode()
            if target=='module':main=main.replace('mod view_config;',
                'mod hidden { const X: u8=0; mod view_config; } use hidden::view_config;')
            elif target=='executor_import':main=main.replace('use embassy_executor::{Executor, Spawner};',
                'mod hidden { const X: u8=0; use embassy_executor::{Executor, Spawner}; } use hidden::*;')
            else:
                begin=main.index('#[embassy_executor::main]');end=main.index('\n}',begin)+2
                main=main[:begin]+'mod hidden { const X: u8=0; '+main[begin:end]+' }'+main[end:]
            documents['main.rs']=main.encode()
            with self.subTest(target=target),self.assertRaises(AssertionError):first_boot_source_contract(documents)

    def test_comments_whitespace_and_literal_contents_have_distinct_semantics(self):
        from config_boot_source import first_boot_source_contract
        documents=self.sources()
        documents={name:raw.replace(b'\r\n',b'\n').replace(b'let configuration',b'// no call or owner\n let configuration') for name,raw in documents.items()}
        self.assertEqual(first_boot_source_contract(documents)['ordered_boot_operations'],('initialize','enqueue_Dashboard'))
        documents=self.sources();documents['view_config.rs']=documents['view_config.rs'].replace(b'"/dashboard.eav"',b'"/other.eav"')
        with self.assertRaises(AssertionError):first_boot_source_contract(documents)
        for value in ({},dict(self.sources(),extra=b'ignored'),dict(self.sources(),**{'main.rs':'text'})):
            with self.subTest(value=tuple(value)),self.assertRaises(AssertionError):first_boot_source_contract(value)

    def test_raw_string_cannot_supply_private_external_module_identity(self):
        from config_boot_source import first_boot_source_contract
        documents=self.sources()
        spoof=b'const SPOOF: &str = r#" "; mod view_config; const END: &str = " "#;\n'
        inline=(b'pub mod view_config {\n'+documents['view_config.rs'].replace(
            b'static CONFIGURATION:',b'pub static CONFIGURATION:')+b'\n}')
        documents['main.rs']=documents['main.rs'].replace(b'mod view_config;',spoof+inline)
        with self.assertRaises(AssertionError):first_boot_source_contract(documents)

    def test_literal_contents_cannot_supply_executor_import_or_boot_main(self):
        from config_boot_source import first_boot_source_contract
        for target in ('import','main'):
            documents=self.sources();text=documents['main.rs'].decode()
            if target=='import':
                old='use embassy_executor::{Executor, Spawner};'
                fake=old;replacement='use other::{Executor, Spawner};'
            else:
                begin=text.index('#[embassy_executor::main]');end=text.index('\n}',begin)+2
                old=text[begin:end];fake=old
                replacement='async fn alternate(spawner: Spawner) { publish(spawner); }'
            text=text.replace(old,'const SPOOF: &str = r#" "; '+fake+' const END: &str = " "#;\n'+replacement)
            documents['main.rs']=text.encode()
            with self.subTest(target=target),self.assertRaises(AssertionError):first_boot_source_contract(documents)

    def test_unsupported_raw_families_and_malformed_literals_fail_before_matching(self):
        from config_boot_source import first_boot_source_contract
        literals=[prefix+'#'*count+'"ignored"'+'#'*count
            for prefix in ('r','br','cr') for count in (0,1,2,255,256)]
        literals+=['r###"unterminated','br#"wrong"##','cr##"wrong"#',
            '"unterminated','"dangling\\','"unsupported\\q"',"'x'","b'x'","'\\"]
        for literal in literals:
            documents=self.sources();documents['main.rs']+=('\nconst UNSUPPORTED: &str = '+literal+';\n').encode()
            with self.subTest(literal=literal[:45]),self.assertRaises(AssertionError):first_boot_source_contract(documents)

    def test_cooked_literals_comments_and_legitimate_lifetimes_stay_atomic(self):
        from config_boot_source import first_boot_source_contract
        documents=self.sources()
        documents['main.rs']+=(b'\n// r###"mod view_config; malformed literal and }\n'
            b'const TEXT: &str = "r#\\\" \\"; mod view_config; } \\"#";\n'
            b'fn borrow_text<\'a>(value: &\'a str) -> &\'a str { value }\n'
            b'fn lifetime_placeholder(value: &\'_ str) {}\n')
        self.assertEqual(first_boot_source_contract(documents)['ordered_boot_operations'],('initialize','enqueue_Dashboard'))


class BuildSourceContextInput(unittest.TestCase):
    def context(self):
        from config_build_source import make_source_context
        sources=FirstBootSourceContract().sources()
        before={'firmware/pico2/src/'+name:raw for name,raw in sources.items()}
        return make_source_context(b'ELF fixture','fixture-attempt',b'{"optimization":"z"}',before)

    def test_strict_roundtrip_binds_exact_ELF_attempt_options_and_source_bytes(self):
        from config_build_source import parse_source_context,validate_source_context
        value=self.context();raw=value.to_bytes();elf=SimpleNamespace(data=b'ELF fixture')
        checked=parse_source_context(raw,elf,'fixture-attempt',b'{"optimization":"z"}')
        self.assertEqual(checked,value)
        self.assertEqual(validate_source_context(checked,elf).documents(),FirstBootSourceContract().sources())
        self.assertNotIn('source_success',checked.to_record())
        for attempt,options,blob in (('other',b'{"optimization":"z"}',b'ELF fixture'),
            ('fixture-attempt',b'{ "optimization":"z"}',b'ELF fixture'),
            ('fixture-attempt',b'{"optimization":"z"}',b'other ELF')):
            with self.subTest(attempt=attempt,options=options),self.assertRaises(AssertionError):
                parse_source_context(raw,SimpleNamespace(data=blob),attempt,options)

    def test_missing_extra_duplicate_and_wrong_type_or_hash_records_reject(self):
        from config_build_source import parse_source_context
        value=self.context();record=value.to_record();elf=SimpleNamespace(data=b'ELF fixture')
        for change in ('schema_bool','extra','missing','elf','options_hash','source_hash','source_length',
                       'source_float','extra_source','duplicate_source','source_type','kind','attempt'):
            bad=json.loads(json.dumps(record))
            if change=='schema_bool':bad['schema']=True
            elif change=='extra':bad['owner']=True
            elif change=='missing':del bad['sources']
            elif change=='elf':bad['elf_sha256']='0'*64
            elif change=='options_hash':bad['build_options']['sha256']='0'*64
            elif change=='source_hash':bad['sources'][0]['sha256']='0'*64
            elif change=='source_length':bad['sources'][0]['length']+=1
            elif change=='source_float':bad['sources'][0]['length']=float(bad['sources'][0]['length'])
            elif change=='extra_source':bad['sources'].append(dict(bad['sources'][0],path='other.rs'))
            elif change=='duplicate_source':bad['sources'][1]=bad['sources'][0]
            elif change=='source_type':bad['sources'][0]['utf8']=[]
            elif change=='kind':bad['kind']='proved_source_owner'
            else:bad['attempt']='../other'
            with self.subTest(change=change),self.assertRaises(AssertionError):
                parse_source_context(json.dumps(bad).encode(),elf)
        raw=value.to_bytes();dup=raw.replace(b'"schema":1',b'"schema":1,"schema":1')
        self.assertNotEqual(raw,dup)
        with self.assertRaises(AssertionError):parse_source_context(dup,elf)
        with self.assertRaises(AssertionError):parse_source_context(b'\xff',elf)
        with self.assertRaises(AssertionError):parse_source_context(None,elf)

    def test_exact_source_and_serialized_input_caps_are_independent_of_memory_results(self):
        from config_build_source import make_source_context,parse_source_context
        before={'firmware/pico2/src/main.rs':b' '*65536,'firmware/pico2/src/view_config.rs':b'v'}
        value=make_source_context(b'ELF fixture','fixture',b'{}',before)
        raw=value.to_bytes();elf=SimpleNamespace(data=b'ELF fixture')
        self.assertEqual(len(parse_source_context(raw,elf).documents()['main.rs']),65536)
        padded=raw+b' '*(262144-len(raw))
        self.assertEqual(parse_source_context(padded,elf),value)
        with self.assertRaises(AssertionError):parse_source_context(padded+b' ',elf)
        before['firmware/pico2/src/main.rs']+=b' '
        with self.assertRaises(AssertionError):make_source_context(b'ELF fixture','fixture',b'{}',before)
        before['firmware/pico2/src/main.rs']=b'\xff'
        with self.assertRaises(AssertionError):make_source_context(b'ELF fixture','fixture',b'{}',before)

    def test_source_semantics_are_recomputed_not_supplied_by_the_context(self):
        from config_build_source import inspect_source_context,make_source_context
        value=self.context();elf=SimpleNamespace(data=b'ELF fixture')
        self.assertEqual(inspect_source_context(value,elf)['kind'],'first_boot_source_semantics_only')
        before={'firmware/pico2/src/'+name:raw for name,raw in value.documents().items()}
        before['firmware/pico2/src/view_config.rs']+=b' pub fn leak() { CONFIGURATION.take(); }'
        bad=make_source_context(elf.data,'fixture-attempt',value.build_options,before)
        with self.assertRaises(AssertionError):inspect_source_context(bad,elf)
        with self.assertRaises(AssertionError):inspect_source_context(value.to_record(),elf)
        with self.assertRaises(AssertionError):inspect_source_context(None,elf)

    def test_exclusive_sidecar_read_has_no_implicit_checkout_or_adjacent_fallback(self):
        import tempfile
        from config_build_source import write_source_context,load_source_context
        value=self.context();elf=SimpleNamespace(data=b'ELF fixture')
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'sources.json'
            with self.assertRaises(FileNotFoundError):load_source_context(path,elf)
            write_source_context(path,value)
            self.assertEqual(load_source_context(path,elf,'fixture-attempt',value.build_options),value)
            with self.assertRaises(FileExistsError):write_source_context(path,value)


class FirstBootHistory(unittest.TestCase):
    def fixture(self,mode='sizez',delta=0):
        from config_build_source import make_source_context,SOURCE_PATHS
        objects,rows,item=PreclaimConfigurationPreservation().fixture(mode,delta)
        documents=FirstBootSourceContract().sources()
        context=make_source_context(objects.elf.data,'supplied-history-fixture',b'{}',
            {path:documents[Path(path).name] for path in SOURCE_PATHS})
        return objects,rows,context

    def test_same_rooted_invocation_closes_ordinary_diagnostics_but_not_loader_or_owner(self):
        from config_boot_history import first_boot_history
        for mode,attempts in (('speed2',4),('sizes',1),('sizez',1)):
            for delta in (0,0x4000):
                objects,rows,context=self.fixture(mode,delta);proof=first_boot_history(objects,rows,context)
                self.assertEqual(proof['phase'],'first_boot_history_pending_specialized_loader_effects')
                self.assertEqual(proof['scope'],'selected ordinary Reset invocation through first successful once claim')
                self.assertEqual(len(proof['publication_attempts']),attempts)
                self.assertEqual({v['kind'] for v in proof['diagnostics']},{'Busy','resumed','already_taken'})
                self.assertTrue(all(v['status']=='source_infeasible_in_selected_ordinary_history' for v in proof['diagnostics']))
                self.assertEqual(proof['source_qualification'],'deprecated_Rust_pre_init_emitted_machine_only')
                self.assertFalse(proof['hardware_qualified']);self.assertNotIn('acquired_owner',proof)
                self.assertTrue(proof['requires_complete_same_B_loader_effects'])

    def test_source_bytes_and_exact_ELF_pairing_are_required_not_source_or_owner_flags(self):
        from config_boot_history import first_boot_history
        from config_build_source import make_source_context,SOURCE_PATHS
        objects,rows,context=self.fixture()
        for value in (None,context.to_record(),dict(source_success=True,owner=True)):
            with self.subTest(value=type(value)),self.assertRaises(AssertionError):first_boot_history(objects,rows,value)
        sources={path:raw for path,raw in context.sources};sources[SOURCE_PATHS[1]]=sources[SOURCE_PATHS[1]].replace(
            b'static CONFIGURATION:',b'pub static CONFIGURATION:')
        bad=make_source_context(objects.elf.data,context.attempt,context.build_options,sources)
        with self.assertRaises(AssertionError):first_boot_history(objects,rows,bad)
        objects.elf.data+=b'changed'
        with self.assertRaisesRegex(AssertionError,'different current ELF'):first_boot_history(objects,rows,context)

    def test_return_addresses_are_bound_to_selected_calls_and_once_restores_before_tail(self):
        from config_boot_history import first_boot_history
        for mode,count in (('speed2',2),('sizes',2),('sizez',5)):
            objects,rows,context=self.fixture(mode);proof=first_boot_history(objects,rows,context)
            returns=proof['selected_returns'];self.assertEqual(len(returns),count)
            self.assertTrue(all(v['continuation']==v['call_site']+4 for v in returns))
            for value in returns:
                self.assertEqual(value['LR_value'],value['continuation']|1)
                self.assertEqual(value['return_SP'],value['entry_SP'])
                if value['phase']=='callback_install':self.assertIsNone(value['saved_LR'])
                else:self.assertEqual(value['saved_LR'],(value['entry_SP']-4,value['entry_SP']))
            self.assertEqual(proof['once_return_transport']['continuation'],proof['boot_initialize_call']+4)
            self.assertEqual(proof['once_return_transport']['LR_value'],(proof['boot_initialize_call']+4)|1)
            self.assertEqual(proof['once_return_transport']['restored_SP'],0x20080000-(88 if mode=='sizez' else 72))
            self.assertTrue(proof['once_return_transport']['restored_before_specialized_tail'])
            self.assertTrue(proof['nonreturn_prefix']['main_cortex_and_run_have_no_return_before_claim'])
            self.assertTrue(proof['nonreturn_prefix']['Reset_UDF_not_reached_by_selected_prefix'])

    def test_all_publication_attempts_and_callback_or_guard_semantic_changes_reject(self):
        from config_boot_history import first_boot_history
        from config_preclaim import preclaim_configuration_preservation
        from dataclasses import replace
        for mode in ('speed2','sizes','sizez'):
            objects,rows,context=self.fixture(mode);p=preclaim_configuration_preservation(objects,rows)
            phases=p['startup']['entry']['entry']['phases']
            sites=p['publication_attempts']+(phases['boot_poll']['function']+18,p['callback'][0],p['claim']['store'])
            for site in sites:
                changed=dict(rows);old=rows[site]
                # Supplied semantic-record mutation with unchanged raw bytes;
                # this checks operand/control contracts, not fresh decoding.
                changed[site]=replace(old,operands='pc, r0',mnemonic='mov')
                with self.subTest(mode=mode,site=hex(site)),self.assertRaises(AssertionError):
                    first_boot_history(objects,changed,context)

    def test_outside_shared_calls_get_contextual_exclusion_not_global_caller_permission(self):
        from config_boot_history import first_boot_history
        from config_preclaim import preclaim_configuration_preservation
        objects,rows,context=self.fixture();p=preclaim_configuration_preservation(objects,rows)
        target=p['startup']['entry']['entry']['phases']['raw_spawn']['function']
        outside=p['startup']['entry']['loader']['function']+200
        rows[outside]=Instruction(outside,b'\x00\xbf','bl',hex(target))
        proof=first_boot_history(objects,rows,context)
        self.assertEqual(proof['outside_shared_invocations'][0]['site'],outside)
        self.assertEqual(proof['outside_shared_invocations'][0]['status'],'outside_closed_selected_prefix')
        self.assertEqual(proof['callback_expiry'],'before next shared dequeue')
        self.assertTrue(proof['later_invocations_require_separate_context'])

    def test_unconsumed_address_data_does_not_create_or_authorize_a_callback(self):
        from config_boot_history import first_boot_history
        from config_build_source import make_source_context
        objects,rows,context=self.fixture()
        objects.elf.data+=struct.pack('<I',0x10000001)  # outside allocated/selected bytes
        context=make_source_context(objects.elf.data,context.attempt,context.build_options,dict(context.sources))
        proof=first_boot_history(objects,rows,context)
        self.assertEqual(proof['callback_binding']['scope'],'same first detached Boot node only')
        self.assertFalse(proof['callback_binding']['global_whitelist'])

    def test_exception_and_input_qualifications_remain_explicit_without_new_assumption(self):
        from config_boot_history import first_boot_history
        objects,rows,context=self.fixture();proof=first_boot_history(objects,rows,context)
        self.assertEqual(proof['exception_graph_limit'],('NMI','HardFault','reset_reentry'))
        self.assertFalse(proof['exception_absence_established'])
        self.assertTrue(proof['requires_H8_consumed_input_closure'])
        self.assertTrue(proof['requires_M2_pre_init_source_reconciliation'])
        self.assertEqual(len(proof['external_qualifications']),3)
        self.assertIn('conditional, not measured',proof['external_qualifications'][1])

    def test_guard_family_cannot_exit_without_a_correlated_successful_exclusive(self):
        from copy import deepcopy
        from config_boot_history import first_boot_history,_family
        objects,rows,context=self.fixture('speed2');proof=first_boot_history(objects,rows,context)
        body=proof['preclaim']['startup']['entry']['entry']['phases']['run'];inventory=deepcopy(proof['successors'])
        # Internal proof-record fault injection, not a supplied public history
        # input or fresh decoded code: make the claim retry escape directly.
        changed=[]
        for item in inventory:
            if item['phase']=='run' and item['site']==body['function']+34:
                item=dict(item,successors=(body['function']+36,body['function']+36))
            changed.append(item)
        with self.assertRaisesRegex(AssertionError,'exit lacks exclusive success'):
            _family(rows,body,changed,('run','sole_zero_slot_claim'),((0,36),),0,36,
                {26:('source_false','same complete Boot pool zero; no earlier claimant',20),34:('exclusive',28,32)},
                proof['selected_returns'],True)

    def test_each_selected_return_and_once_restore_keep_their_exact_PC_transport(self):
        from dataclasses import replace
        from config_boot_history import first_boot_history
        for mode in ('speed2','sizes','sizez'):
            objects,rows,context=self.fixture(mode);proof=first_boot_history(objects,rows,context)
            sites=tuple(v['return_site'] for v in proof['selected_returns'])+(proof['once_return_transport']['restore'],)
            for at in sites:
                changed=dict(rows);changed[at]=replace(rows[at],mnemonic='bx',operands='r4')
                with self.subTest(mode=mode,at=hex(at)),self.assertRaises(AssertionError):
                    first_boot_history(objects,changed,context)

    def test_callback_load_and_initialized_completion_cannot_change_source(self):
        from dataclasses import replace
        from config_boot_history import first_boot_history
        for mode in ('speed2','sizes','sizez'):
            objects,rows,context=self.fixture(mode);proof=first_boot_history(objects,rows,context)
            initialize=proof['preclaim']['startup']['entry']['entry']['phases']['boot_initializer']
            completion=initialize['function']+(14 if mode=='speed2' else 6)
            for at,operand in ((proof['callback_binding']['site']-2,'r1, [r2, #0x24]'),
                (completion,'r0, #0x1' if mode=='speed2' else 'r1, #0x1')):
                changed=dict(rows);changed[at]=replace(rows[at],operands=operand)
                with self.subTest(mode=mode,at=hex(at)),self.assertRaises(AssertionError):
                    first_boot_history(objects,changed,context)

    def test_internal_write_envelopes_cannot_corrupt_saved_LR_or_initialized_completion(self):
        from copy import deepcopy
        from config_boot_history import first_boot_history,_edges,_save_protection,_state_writers
        objects,rows,context=self.fixture();proof=first_boot_history(objects,rows,context)
        p=proof['preclaim'];bodies,_=_edges(p);end=proof['protected_save_intervals']['boot_poll'][1]
        for fault in ('saved_LR','completion'):
            changed=deepcopy(p)
            value=dict(phase='configuration_initialize',instruction=bodies['configuration_initialize']['function']+12,
                scope='before',interval=(end-4,end))
            if fault=='completion':value.update(interval=(p['task'][0]+44,p['task'][0]+45),phase='run')
            changed['writes']=changed['writes']+(value,)
            with self.subTest(fault=fault),self.assertRaises(AssertionError):
                if fault=='saved_LR':_save_protection(changed,bodies)
                else:_state_writers(changed,bodies)


class SameBLoaderBoundary(unittest.TestCase):
    def fixture(self,mode='sizez',delta=0):
        from config_build_source import make_source_context
        objects,rows,context=FirstBootHistory().fixture(mode,delta);elf=objects.elf
        item=next(v for v in json.loads((Path(__file__).parent/'fixtures/config-loader-boundary.json').read_text()) if v['mode']==mode)
        for parent in item['parents']:
            a=parent['address'];index=next(i for i,s in enumerate(elf.sections) if s[2]&4 and s[3]<=a<a+parent['extent']<=s[3]+s[5])
            for name in parent['raw_roles']:
                elf.functions[name]=(a|1,parent['extent'],index)
            elf.mapping_symbols.extend((('$'+kind,a,index) for a,z,kind in parent['mapping']) )
            objects.sizes[a]=parent['frame']
        for at,encoded in item['segments']:StartupConfigurationCopy.put(objects,at,bytes.fromhex(encoded))
        for v in item['records']:rows[v['address']]=Instruction(v['address'],bytes.fromhex(v['bytes']),v['mnemonic'],v['operands'])
        if delta:
            for parent in item['parents']:
                for kind,lo,hi,register in parent['receivers']:
                    if kind=='pc_literal':StartupConfigurationCopy.put(objects,parent['address']+hi,struct.pack('<I',item['config']+delta))
                    else:
                        for offset,operation,value in ((lo,'movw',(item['config']+delta)&0xffff),(hi,'movt',(item['config']+delta)>>16)):
                            self.mov(objects,rows,parent['address']+offset,operation,register,value)
        context=make_source_context(elf.data,context.attempt,context.build_options,dict(context.sources))
        return objects,rows,context,item

    @staticmethod
    def mov(objects,rows,at,operation,register,value):
        rd=14 if register=='lr' else int(register[1:]);first=(0xf240 if operation=='movw' else 0xf2c0)|((value>>12)&15)|(((value>>11)&1)<<10)
        raw=struct.pack('<HH',first,(((value>>8)&7)<<12)|(rd<<8)|(value&255))
        StartupConfigurationCopy.put(objects,at,raw);rows[at]=Instruction(at,raw,operation,register+', #'+hex(value))

    @staticmethod
    def rebound(objects,context):
        from config_build_source import make_source_context
        return make_source_context(objects.elf.data,context.attempt,context.build_options,dict(context.sources))

    def test_current_modes_derive_restored_SP_and_same_static_points_without_an_owner(self):
        from config_loader_boundary import first_boot_loader_boundary
        for mode,frame,entry,count in (('speed2',1288,72,34),('sizes',696,72,33),('sizez',456,88,41)):
            for delta in (0,0x4000):
                objects,rows,context,item=self.fixture(mode,delta);proof=first_boot_loader_boundary(objects,rows,context)
                self.assertEqual(proof['phase'],'first_boot_loader_pending_consuming_phase_closure')
                self.assertEqual(proof['loader_geometry']['entry_SP'],0x20080000-entry)
                self.assertEqual(proof['loader_geometry']['frame'],(0x20080000-entry-frame,0x20080000-entry))
                self.assertEqual(len(proof['static_producers']),count)
                self.assertTrue(all(v['value']==item['config']+delta for v in proof['static_producers']))
                self.assertFalse(proof['consuming_phase_closure']);self.assertNotIn('acquired_owner',proof)
                self.assertFalse(proof['hardware_qualified'])

    def test_new_root_frame_rejects_physical_section_LOAD_and_OBJECT_aliases(self):
        from config_loader_boundary import first_boot_loader_boundary
        for mode in ('speed2','sizes','sizez'):
            for kind in ('section','LOAD','OBJECT'):
                objects,rows,context,item=self.fixture(mode);address=0x20080000-400
                if kind=='section':objects.elf.sections.append((0,8,3,address,0,8,0,0,4,0))
                elif kind=='LOAD':objects.elf.segments.append((1,0,address,address,0,8,6,4))
                else:
                    # Metadata-only forged object in an allocated section:
                    # unlike Storage.space names, physical addresses overlap.
                    index=next(i for i,s in enumerate(objects.elf.sections) if s[2]&2)
                    objects.elf.symbol_entries.append(('foreign_stack_object',address,8,17,index))
                with self.subTest(mode=mode,kind=kind),self.assertRaises(AssertionError):first_boot_loader_boundary(objects,rows,context)

    def test_actual_stack_bound_and_checked_metadata_extents_cannot_be_bypassed(self):
        from config_loader_boundary import first_boot_loader_boundary
        for kind in ('overflow_section','overflow_LOAD','overflow_OBJECT','stack_bottom'):
            objects,rows,context,item=self.fixture()
            if kind=='overflow_section':objects.elf.sections.append((0,8,3,0xfffffff0,0,32,0,0,4,0))
            elif kind=='overflow_LOAD':objects.elf.segments.append((1,0,0xfffffff0,0xfffffff0,0,32,6,4))
            elif kind=='overflow_OBJECT':objects.elf.symbol_entries.append(('overflow',0xfffffff0,32,17,2))
            else:
                # Root needs456 more bytes below its entry, beyond preclaim.
                elf=objects.elf;old=elf.named['.uninit'];index=elf.sections.index(old);limit=0x2007ff80
                new=tuple(limit-old[3] if i==5 else v for i,v in enumerate(old));elf.sections[index]=new;elf.named['.uninit']=new
                elf.symbol_entries=[(n,limit if n in ('_stack_end','__euninit') else a,z,t,i) for n,a,z,t,i in elf.symbol_entries]
                elf.segments=[tuple(new[5] if i==5 else v for i,v in enumerate(s)) if s[2]==old[3] else s for s in elf.segments]
                from config_boot_history import first_boot_history
                first_boot_history(objects,rows,context)  # The shorter prefix still fits.
            with self.subTest(kind=kind),self.assertRaises(AssertionError):first_boot_loader_boundary(objects,rows,context)

    def test_static_point_cannot_select_another_address_even_with_coherent_bytes(self):
        from config_loader_boundary import first_boot_loader_boundary
        for mode in ('speed2','sizes','sizez'):
            objects,rows,context,item=self.fixture(mode)
            for parent in item['parents']:
                for kind,lo,hi,register in parent['receivers'][:1]:
                    if kind=='pc_literal':StartupConfigurationCopy.put(objects,parent['address']+hi,struct.pack('<I',item['config']+4))
                    else:self.mov(objects,rows,parent['address']+lo,'movw',register,(item['config']+4)&0xffff)
                    with self.subTest(mode=mode,parent=parent['role']),self.assertRaises(AssertionError):
                        first_boot_loader_boundary(objects,rows,self.rebound(objects,context))
                    # Rebuild each parent case so unrelated mismatches cannot
                    # mask a validator/visitor point with an earlier root one.
                    objects,rows,context,item=self.fixture(mode)

    def test_root_prologue_frame_and_semantic_SP_changes_reject(self):
        from config_loader_boundary import first_boot_loader_boundary
        from dataclasses import replace
        for mode in ('speed2','sizes','sizez'):
            for kind in ('frame','SP'):
                objects,rows,context,item=self.fixture(mode);root=next(p for p in item['parents'] if 'load_at_boot' in p['role']);at=root['address']
                if kind=='frame':objects.sizes[at]+=8
                else:rows[at]=replace(rows[at],operands='{r0, r7, lr}')
                with self.subTest(mode=mode,kind=kind),self.assertRaises(AssertionError):first_boot_loader_boundary(objects,rows,context)

    def test_protected_shapes_keep_taken_padding_tasks_P_and_all_caller_cells(self):
        from config_loader_boundary import first_boot_loader_boundary
        for mode in ('speed2','sizes','sizez'):
            objects,rows,context,item=self.fixture(mode);proof=first_boot_loader_boundary(objects,rows,context)
            self.assertEqual(proof['taken_state'],dict(address=item['config']+14600,width=1,value=1,source='same successful once transition'))
            protected={v['kind']:v['interval'] for v in proof['physical_protected']}
            self.assertEqual(protected['taken_and_enclosing_padding'],(item['config']+14600,item['config']+14604))
            self.assertEqual(protected['executor_P'],(0x2007ffe8,0x2007fff0))
            self.assertEqual(protected['retained_callers'],(proof['loader_geometry']['entry_SP'],0x20080000))
            self.assertEqual(protected['Boot_task'][1]-protected['Boot_task'][0],48)
            self.assertTrue(proof['initial_file_facts_not_live_reinitialization'])
            self.assertEqual(proof['initial_fact_expiry'],'first relevant decode/refill/publication mutation')
            self.assertFalse(proof['exception_absence_established']);self.assertTrue(proof['requires_H8_consumed_input_closure'])
            self.assertTrue(proof['requires_M2_pre_init_source_reconciliation'])

    def test_context_or_supplied_history_record_cannot_substitute_for_internal_recomputation(self):
        from config_loader_boundary import first_boot_loader_boundary
        objects,rows,context,item=self.fixture()
        for supplied in (None,dict(owner=True,history_complete=True,B=item['config']),context.to_record()):
            with self.assertRaises(AssertionError):first_boot_loader_boundary(objects,rows,supplied)
        with self.assertRaises(TypeError):first_boot_loader_boundary(objects,rows,context,history=dict(owner=True))

    def test_independent_root_code_and_PC_pool_relocation_still_bind_same_B(self):
        from config_loader_boundary import first_boot_loader_boundary
        from config_boot_history import first_boot_history
        from config_headers import ROOT
        for mode in ('speed2','sizes','sizez'):
            objects,rows,context,item=self.fixture(mode);elf=objects.elf
            history=first_boot_history(objects,rows,context);old=history['preclaim']['startup']['entry']['loader']['function']
            name=next(n for n in elf.functions if config_stack.canonical_symbol(n)==ROOT)
            pointer,extent,index=elf.functions[name];section=elf.sections[index];at=section[4]+old-section[3]
            copied=elf.data[at:at+extent];new=0x10071000;file_at=(len(elf.data)+3)&~3
            elf.data+=bytes(file_at-len(elf.data))+copied;new_index=len(elf.sections)
            elf.sections.append((0,1,6,new,file_at,extent,0,0,4,0));elf.segments.append((1,file_at,new,new,extent,extent,5,4))
            elf.functions[name]=(new|1,extent,new_index);objects.sizes[new]=objects.sizes.pop(old)
            elf.symbol_entries=[(n,new|1,z,t,new_index) if n==name else (n,a,z,t,i) for n,a,z,t,i in elf.symbol_entries]
            root=next(p for p in item['parents'] if p['role']==ROOT)
            elf.mapping_symbols.extend(('$'+k,a+new-old,new_index) for a,z,k in root['mapping'])
            for address,row in list(rows.items()):
                if old<=address<old+extent:
                    del rows[address];moved=address+new-old;rows[moved]=Instruction(moved,row.raw,row.mnemonic,row.operands)
            tail=history['preclaim']['startup']['entry']['once']['success']['tail']
            low,high=struct.unpack('<HH',StartupConfigurationCopy.branch_link(tail,new));raw=struct.pack('<HH',low,high&~0x4000)
            StartupConfigurationCopy.put(objects,tail,raw);rows[tail]=Instruction(tail,raw,'b.w',hex(new))
            proof=first_boot_loader_boundary(objects,rows,self.rebound(objects,context))
            self.assertEqual(proof['loader_geometry']['function'],new)
            self.assertTrue(all(v['value']==item['config'] for v in proof['static_producers']))
        # Move one consumed PC pool independently of code/B. This is coherent
        # synthetic opcode+bytes, not a fresh linked or decoded image.
        objects,rows,context,item=self.fixture();root=next(p for p in item['parents'] if 'load_at_boot' in p['role'])
        _,offset,pool,register=root['receivers'][0];site=root['address']+offset;pool=root['address']+pool+4
        immediate=pool-((site+4)&~3);raw=struct.pack('<HH',0xf8df,(8<<12)|immediate)
        StartupConfigurationCopy.put(objects,site,raw);StartupConfigurationCopy.put(objects,pool,struct.pack('<I',item['config']))
        rows[site]=Instruction(site,raw,'ldr.w','r8, [pc, #'+hex(immediate)+']')
        proof=first_boot_loader_boundary(objects,rows,self.rebound(objects,context))
        self.assertTrue(any(v.get('pool')==pool for v in proof['static_producers']))

    def test_physical_allocation_adjacency_is_valid_but_partial_overlap_is_not(self):
        from config_loader_boundary import first_boot_loader_boundary
        for offset in (-8,0,1):
            objects,rows,context,item=self.fixture();low=0x20080000-88-456
            objects.elf.sections.append((0,8,3,low+offset,0,8,0,0,1,0))
            if offset==-8:
                self.assertEqual(first_boot_loader_boundary(objects,rows,context)['loader_geometry']['frame'][0],low)
            else:
                with self.assertRaisesRegex(AssertionError,'physical frame intersects'):first_boot_loader_boundary(objects,rows,context)

    def test_LOAD_file_bytes_cannot_exceed_the_declared_memory_extent(self):
        from config_loader_boundary import first_boot_loader_boundary
        objects,rows,context,item=self.fixture()
        objects.elf.segments.append((1,0,0x10090000,0x10090000,8,4,5,4))
        with self.assertRaisesRegex(AssertionError,'LOAD file/memory extent'):
            first_boot_loader_boundary(objects,rows,context)

    def test_Reset_SP_relocation_with_fixed_B_code_and_task_shifts_loader_intervals(self):
        from config_loader_boundary import first_boot_loader_boundary
        for mode in ('speed2','sizes','sizez'):
            for shift in (0x800,0x1000):
                objects,rows,context,item=self.fixture(mode);elf=objects.elf
                old=first_boot_loader_boundary(objects,rows,context);original=elf.data
                functions=dict(elf.functions);instructions=dict(rows)
                vector=elf.named['.vector_table'][3];section=elf.named['.vector_table'];file_at=section[4]+vector-section[3]
                sp=0x20080000-shift
                StartupConfigurationCopy.put(objects,vector,struct.pack('<I',sp))
                elf.symbol_entries=[(n,sp if n=='_stack_start' else a,z,t,i) for n,a,z,t,i in elf.symbol_entries]
                proof=first_boot_loader_boundary(objects,rows,self.rebound(objects,context))
                expected=bytearray(original);struct.pack_into('<I',expected,file_at,sp)
                self.assertEqual(elf.data,bytes(expected));self.assertEqual(elf.functions,functions);self.assertEqual(rows,instructions)
                self.assertEqual(proof['enclosing_allocation'],old['enclosing_allocation'])
                self.assertEqual(proof['taken_state'],old['taken_state'])
                self.assertEqual(proof['static_producers'],old['static_producers'])
                before=old['loader_geometry'];after=proof['loader_geometry']
                self.assertEqual(after['entry_SP'],before['entry_SP']-shift)
                for name in ('frame','saves','locals'):
                    self.assertEqual(after[name],tuple(v-shift for v in before[name]))
                for name in ('function','extent','root_frame_bytes','LR_value','once_frame_retained_bytes'):
                    self.assertEqual(after[name],before[name])
                self.assertEqual(proof['retained_save_constituents'],tuple((name,(lo-shift,hi-shift)) for name,(lo,hi) in old['retained_save_constituents']))
                prior={v['kind']:v['interval'] for v in old['physical_protected']}
                current={v['kind']:v['interval'] for v in proof['physical_protected']}
                for name in ('executor_P','retained_callers'):
                    self.assertEqual(current[name],tuple(v-shift for v in prior[name]))
                for name in ('Boot_task','taken_and_enclosing_padding'):self.assertEqual(current[name],prior[name])
                self.assertEqual(proof['history']['preclaim']['executor'],(sp-24,8))
                self.assertEqual(proof['history']['once_return_transport']['LR_value'],old['history']['once_return_transport']['LR_value'])
                self.assertEqual(proof['history']['selected_returns'],tuple(dict(v,entry_SP=v['entry_SP']-shift,
                    return_SP=v['return_SP']-shift,saved_LR=None if v['saved_LR'] is None else tuple(a-shift for a in v['saved_LR'])) for v in old['history']['selected_returns']))
                self.assertEqual(proof['phase'],'first_boot_loader_pending_consuming_phase_closure')
                self.assertNotIn('acquired_owner',proof);self.assertFalse(proof['consuming_phase_closure'])

    def test_Reset_SP_inconsistent_vector_or_symbol_rejects_with_current_source_context(self):
        from config_loader_boundary import first_boot_loader_boundary
        for mode in ('speed2','sizes','sizez'):
            for changed in ('vector_only','symbol_only'):
                objects,rows,context,item=self.fixture(mode);elf=objects.elf;sp=0x2007f800
                if changed=='vector_only':StartupConfigurationCopy.put(objects,elf.named['.vector_table'][3],struct.pack('<I',sp))
                else:elf.symbol_entries=[(n,sp if n=='_stack_start' else a,z,t,i) for n,a,z,t,i in elf.symbol_entries]
                # Rebinding avoids rejecting earlier merely for stale ELF SHA.
                with self.subTest(mode=mode,changed=changed),self.assertRaisesRegex(AssertionError,'vector SP'):
                    first_boot_loader_boundary(objects,rows,self.rebound(objects,context))


class ChecksumRefillHandoff(unittest.TestCase):
    def fixture(self):
        from llvm_stack import mapping_ranges
        objects,rows,context,item=SameBLoaderBoundary().fixture();elf=objects.elf
        donor,more,name,frame=DefaultSourceTerminal().fixture()
        # Compose already reviewed literal fixtures by physical address. The
        # unchanged entry/static-point fixture supplies startup and initial B.
        # Sparse fixtures must contribute only declared bytes, never their
        # zero-filled section holes over other already reviewed objects.
        for filename in ('config-headers.json','config-construction.json','config-default-preparation.json',
            'config-refill-children.json','config-default-widget.json','config-default-sources.json'):
            data=json.loads((Path(__file__).parent/'fixtures'/filename).read_text())
            selected=next(v for v in data if v['mode']=='sizez') if isinstance(data,list) else data
            for segment in selected['segments']:
                index,address,encoded=segment
                section=donor.elf.sections[index];n=len(bytes.fromhex(encoded));at=section[4]+address-section[3]
                StartupConfigurationCopy.put(objects,address,donor.elf.data[at:at+n])
        for n,(a,z,index) in donor.elf.functions.items():
            if not any((a&~1)<=at<(a&~1)+z for at in more):continue
            for lo,hi,kind in mapping_ranges(donor.elf,n):
                if kind=='d':StartupConfigurationCopy.put(objects,lo,donor.read_only(lo,hi-lo))
        for at,row in more.items():StartupConfigurationCopy.put(objects,at,row.raw)
        # The handoff binds the future validator's receiver prefix only.
        # Retain existing complete Result fixture bytes as input evidence;
        # later Result/visitor effects are not invoked by this producer.
        extra=json.loads((Path(__file__).parent/'fixtures/config-validator-result.json').read_text())
        for index,address,encoded in extra['segments']:StartupConfigurationCopy.put(objects,address,bytes.fromhex(encoded))
        for v in extra['records']:more[v['address']]=Instruction(v['address'],bytes.fromhex(v['bytes']),v['mnemonic'],v['operands'])
        # Restore actual selected point dependencies if an unused donor hole
        # overlapped them; every retained point uses the same original bytes.
        for at,encoded in item['segments']:StartupConfigurationCopy.put(objects,at,bytes.fromhex(encoded))
        elf.functions.update(donor.elf.functions);objects.sizes.update(donor.sizes)
        elf.symbol_entries=list(dict.fromkeys(elf.symbol_entries+donor.elf.symbol_entries))
        elf.mapping_symbols=list(dict.fromkeys(elf.mapping_symbols+donor.elf.mapping_symbols))
        rows.update(more)
        return objects,rows,SameBLoaderBoundary.rebound(objects,context)

    def test_public_checksum_alternative_reaches_complete_normal_refill_before_validator_only(self):
        from config_refill_handoff import checksum_refill_handoff
        objects,rows,context=self.fixture();proof=checksum_refill_handoff(objects,rows,context)
        self.assertEqual(proof['phase'],'checksum_refill_pending_validator_and_diagnostic_closure')
        self.assertEqual(proof['widgets'],tuple((i,c,444+220*i) for i,c in enumerate((0,7,6,2,1,3,4,5))))
        self.assertEqual(proof['sources'],tuple((i,x,32+22*i) for i,x in enumerate((5,3,1,14,2,15,4,13))))
        self.assertEqual(set(proof['source_groups']),{'fuel','sources','warnings','controls','views','widgets','warning_colors','history','led','version','catalogue_version','reduced_motion','boot_view'})
        P=0x20080000-88-456
        self.assertEqual(proof['before_validator']['result'],(P+236,12))
        self.assertEqual(proof['before_validator']['r8'],P+112)
        self.assertEqual(proof['before_validator']['config'],(0x2000031c,14576))
        self.assertEqual(proof['before_validator']['phase'],'default_validation')
        self.assertTrue(proof['before_validator']['readonly_Config'])
        self.assertFalse(proof['checksum_alternative_observed_taken']);self.assertNotIn('acquired_owner',proof)
        self.assertTrue(proof['diagnostic_closure_pending']);self.assertFalse(proof['hardware_qualified'])
        self.assertEqual(len(proof['pending_diagnostics']),4)
        self.assertGreater(len(proof['floating_carrier_preserving_child_bodies']),0)
        self.assertTrue(proof['target_access_requirements_unresolved'])

    def test_premature_Source_exit_cannot_reuse_live_ID_staging_as_Result(self):
        from config_refill_handoff import checksum_refill_handoff
        from dataclasses import replace
        objects,rows,context=self.fixture()
        S=struct.unpack('<I',objects.read_only(objects.elf.named['.vector_table'][3],4))[0]
        P=S-544;source=0x10020290
        ids=(P+244,P+252);result=(P+236,P+248)
        self.assertEqual((max(ids[0],result[0]),min(ids[1],result[1])),(P+244,P+248))
        # Actual initialization and pointer transport establish the live eight
        # ID bytes. R's first use as a Result overlaps the first four of them.
        expected={10:('movs','r5, #0x0'),18:('str','r0, [sp, #0xf4]'),
            14:('str','r0, [sp, #0xf8]'),32:('add','r0, sp, #0xec'),
            34:('add.w','r11, r0, #0x8'),42:('cmp','r5, #0xb0'),
            44:('beq',hex(source+206)),46:('ldrb','r1, [r11], #1'),
            198:('adds','r5, #0x16'),204:('b',hex(source+38)),
            328:('add','r4, sp, #0xec'),336:('mov','r0, r4')}
        for offset,pair in expected.items():
            row=rows[source+offset]
            self.assertEqual((row.mnemonic,row.operands.split('@',1)[0].split('<',1)[0].strip()),pair)
        self.assertEqual(rows[source+338].mnemonic,'bl')
        # Offset increments22 close at176 only after eight actual ID reads.
        # Index3 at this same postincrement site is the last overlapping byte
        # read (P247); index7 is the array's final read (P251). Neither occurs
        # if the initial zero counter takes the terminal exit immediately.
        self.assertEqual(tuple(ids[0]+i for i in range(4)),tuple(range(P+244,P+248)))
        self.assertEqual(ids[0]+3,result[1]-1)
        self.assertEqual(ids[0]+7,ids[1]-1)
        self.assertEqual(8*22,176)
        changed=dict(rows);at=source+42
        changed[at]=replace(rows[at],operands='r5, #0x0')
        self.assertEqual([a for a in rows if rows[a]!=changed[a]],[at])
        self.assertEqual(changed[at].raw,rows[at].raw)
        # Semantic-record mutation only: the actual exit/call relation now
        # bypasses those last reads while keeping R=P236 and the call intact.
        # ELF/source context are unchanged; no supplied context/owner/phase is
        # accepted and no newly decoded executable mutant is claimed.
        returned=None
        with self.assertRaises(AssertionError):returned=checksum_refill_handoff(objects,changed,context)
        self.assertIsNone(returned)

    def test_independent_Reset_S_and_Overview_object_pool_relocation_keep_B_code_fixed(self):
        from config_refill_handoff import checksum_refill_handoff
        objects,rows,context=self.fixture();elf=objects.elf
        # Independent lower Reset stack with unchanged B/task/code/LR.
        S=0x2007f000
        StartupConfigurationCopy.put(objects,elf.named['.vector_table'][3],struct.pack('<I',S))
        elf.symbol_entries=[(n,S if n=='_stack_start' else a,z,t,i) for n,a,z,t,i in elf.symbol_entries]
        # Move only the source Overview object and update its actually consumed
        # PC pool word, without moving code/pools or unrelated readonly objects.
        at=0x1001ff02+32;row=rows[at]
        displacement=int(re.search(r'#(0x[0-9a-f]+|[0-9]+)',row.operands)[1],0)
        pool=((at+4)&~3)+displacement;old=struct.unpack('<I',objects.read_only(pool,4))[0]
        section=elf.named['.rodata'];new=section[3]+section[5]-32
        self.assertEqual(objects.read_only(old,8),b'Overview')
        StartupConfigurationCopy.put(objects,new,b'Overview');StartupConfigurationCopy.put(objects,pool,struct.pack('<I',new))
        proof=checksum_refill_handoff(objects,rows,SameBLoaderBoundary.rebound(objects,context))
        P=S-544
        self.assertEqual(proof['before_validator']['result'],(P+236,12))
        self.assertEqual(proof['before_validator']['r8'],P+112)
        self.assertEqual(proof['before_validator']['config'],(0x2000031c,14576))
        self.assertEqual(proof['before_validator']['phase'],'default_validation')
        self.assertTrue(proof['before_validator']['readonly_Config'])
        self.assertEqual(proof['fault']['retained'],(P+8,16))
        self.assertEqual(proof['child_stack_low'],P-144)
        self.assertEqual(proof['before_validator']['receiver_prefix'],(0x1001ea20,22))

    def test_foreign_checksum_fault_entry_and_wrong_child_output_reject(self):
        from config_refill_handoff import checksum_refill_handoff
        from dataclasses import replace
        cases=((0x1001ff02+36,'r0, r8'),(0x1001ffb8+0x2a-2,'r0, r8'))
        # Operand-only records intentionally test finite semantic matching,
        # never claim a new LLVM decode or an executable mutant image.
        for at,operand in cases:
            objects,rows,context=self.fixture();self.assertIn(at,rows)
            changed=dict(rows);changed[at]=replace(rows[at],operands=operand)
            with self.subTest(site=hex(at)),self.assertRaises(AssertionError):checksum_refill_handoff(objects,changed,context)
        objects,rows,context=self.fixture()
        # An additional semantic branch record into the fault interior does not
        # acquire the source CRC guard by merely residing in the same parent.
        at=0x1001ff02+1500
        rows[at]=Instruction(at,bytes.fromhex('00e0'),'b',hex(0x1001ff00))
        with self.assertRaisesRegex(AssertionError,'foreign checksum-fault ingress'):checksum_refill_handoff(objects,rows,context)

    def test_fault_loop_and_reset_semantic_changes_cannot_manufacture_a_handoff(self):
        from config_refill_handoff import checksum_refill_handoff
        from dataclasses import replace
        for at,operand in ((0x1001ff00,'r0, [sp, #0xc]'),(0x1001ff34,'r5, r4, [sp, #20]'),
            (0x10020286,'r0, #0x1'),(0x10020358,'r8, sp, #0x74')):
            objects,rows,context=self.fixture();self.assertIn(at,rows)
            changed=dict(rows);changed[at]=replace(rows[at],operands=operand)
            with self.subTest(at=hex(at)),self.assertRaises(AssertionError):checksum_refill_handoff(objects,changed,context)

    def test_deeper_child_stack_cannot_alias_allocated_memory_below_root(self):
        from config_refill_handoff import checksum_refill_handoff
        from config_loader_boundary import first_boot_loader_boundary
        objects,rows,context=self.fixture();low=0x20080000-88-456
        objects.elf.sections.append((0,8,3,low-120,0,8,0,0,4,0))
        first_boot_loader_boundary(objects,rows,context)
        with self.assertRaises(AssertionError):checksum_refill_handoff(objects,rows,context)


class ChecksumValidatorReturn(unittest.TestCase):
    def fixture(self):
        objects,rows,context=ChecksumRefillHandoff().fixture()
        donor,name,more,supplied,call=ValidatorControls().fixture()
        for filename in ('config-validator-result.json','config-validator-pointers.json','config-validator-controls.json'):
            item=json.loads((Path(__file__).parent/'fixtures'/filename).read_text())
            for index,address,encoded in item['segments']:
                section=donor.elf.sections[index];count=len(bytes.fromhex(encoded));at=section[4]+address-section[3]
                StartupConfigurationCopy.put(objects,address,donor.elf.data[at:at+count])
        objects.elf.functions.update(donor.elf.functions);objects.sizes.update(donor.sizes)
        objects.elf.symbol_entries=list(dict.fromkeys(objects.elf.symbol_entries+donor.elf.symbol_entries))
        objects.elf.mapping_symbols=list(dict.fromkeys(objects.elf.mapping_symbols+donor.elf.mapping_symbols))
        rows.update(more)
        extra=json.loads((Path(__file__).parent/'fixtures'/'config-validator-traversal.json').read_text())
        for index,address,encoded in extra['segments']:StartupConfigurationCopy.put(objects,address,bytes.fromhex(encoded))
        objects.elf.functions.update({n:tuple(v) for n,v in extra['functions'].items()})
        objects.sizes.update({int(a):n for a,n in extra['sizes'].items()})
        objects.elf.symbol_entries=list(dict.fromkeys(objects.elf.symbol_entries+[tuple(v) for v in extra['symbol_entries']]))
        rows.update({a:Instruction(a,bytes.fromhex(raw),op,operand) for a,raw,op,operand in extra['instructions']})
        return objects,rows,SameBLoaderBoundary.rebound(objects,context)

    def test_actual_checksum_refill_joins_complete_normal_Result_and_sole_restore_only(self):
        import importlib.util
        self.assertIsNotNone(importlib.util.find_spec('config_validator_return'),'missing actual checksum validator-return consumer')
        from config_validator_return import checksum_refill_validator_return
        objects,rows,context=self.fixture();proof=checksum_refill_validator_return(objects,rows,context)
        self.assertEqual(proof['phase'],'checksum_refill_validator_return_pending_diagnostics_and_loader_continuation')
        self.assertEqual(proof['normal_return']['return_to'],0x100203e6)
        self.assertEqual(proof['normal_return']['restored_SP'],0x20080000-544)
        self.assertEqual(proof['normal_return']['original_r8'],0x20080000-544+112)
        self.assertEqual(proof['initialized_inputs'],dict(views=1,widgets=8,sources=8))
        self.assertEqual({a['kind'] for a in proof['normal_alternatives']},{'Err','Ok'})
        self.assertTrue(proof['diagnostic_closure_pending']);self.assertTrue(proof['loader_continuation_pending'])
        self.assertFalse(proof['hardware_qualified']);self.assertNotIn('acquired_owner',proof)

    def test_channel_domain_invocation_reads_each_Widget_Channel_not_its_id(self):
        from config_validator_return import checksum_refill_validator_return
        objects,rows,context=self.fixture();proof=checksum_refill_validator_return(objects,rows,context)
        calls=[n for n in proof['guard_stopped_normal_activation_closure']['invocations']
               if n['family']=='channel_domain']
        self.assertEqual(len(calls),1)
        argument=calls[0]['source_receiver_phase']
        # Fixture Config: Widgets start at444, stride220, Channel217; id212
        # is a separate u16. These eight offsets name the initialized prefix.
        base=0x2000031c
        self.assertEqual(argument['kind'],'Channel scalar')
        self.assertEqual(argument['readonly_ranges'],tuple((base+offset,1)
            for offset in (661,881,1101,1321,1541,1761,1981,2201)))

    def test_exported_summary_and_supplied_context_cannot_replace_internal_recomputation(self):
        from config_validator_return import checksum_refill_validator_return
        objects,rows,context=self.fixture()
        with self.assertRaises(AssertionError):checksum_refill_validator_return(objects,rows,dict(mode='sizez',phase='default_validation',source_valid=True))
        with self.assertRaises(TypeError):checksum_refill_validator_return(objects,rows,context,dict(completed=True))

    def test_early_late_capture_and_Result_spill_records_still_need_actual_transports(self):
        from config_validator_return import checksum_refill_validator_return
        from dataclasses import replace
        for at,operand in ((0x1001ea2e,'r5, r1'),(0x1001ea76,'r5, [sp, #0x40]'),
            (0x1001ebac,'r5, [sp, #0x40]'),(0x1001ea3e,'r5, [sp, #0x58]')):
            objects,rows,context=self.fixture();self.assertIn(at,rows)
            changed=dict(rows);changed[at]=replace(rows[at],operands=operand)
            with self.subTest(site=hex(at)),self.assertRaises(AssertionError):checksum_refill_validator_return(objects,changed,context)

    def test_live_edges_to_occupancy_and_ColorRole_selector_cannot_lose_their_source_phase(self):
        from config_validator_return import checksum_refill_validator_return
        from dataclasses import replace
        for at,operand in ((0x1001ef08,'r0, r0, [sp, #0x70]'),(0x1001f154,'r0, #0x10')):
            objects,rows,context=self.fixture();self.assertIn(at,rows)
            changed=dict(rows);changed[at]=replace(rows[at],operands=operand)
            with self.subTest(site=hex(at)),self.assertRaises(AssertionError):checksum_refill_validator_return(objects,changed,context)

    def test_child_output_cannot_reach_validator_saves_or_retained_original_fault(self):
        from config_validator_return import checksum_refill_validator_return
        from dataclasses import replace
        for offset in (216,328):
            objects,rows,context=self.fixture()
            candidates=[at for at,row in rows.items() if 0x1001ea20<=at<0x1001fdd8 and
                (row.mnemonic,row.operands)==('add','r0, sp, #0x70')]
            self.assertTrue(candidates);at=min(candidates)
            changed=dict(rows);changed[at]=replace(rows[at],operands='r0, sp, #'+hex(offset))
            with self.subTest(offset=offset),self.assertRaises(AssertionError):checksum_refill_validator_return(objects,changed,context)

    def test_payload_tag_and_restore_must_preserve_paired_normal_return(self):
        from config_validator_return import checksum_refill_validator_return
        from dataclasses import replace
        for at,operand in ((0x1001eb7a,'r0, [r5]'),(0x1001fbae,'s1, [r5, #4]'),
            (0x1002846c,'{r8, r9, r10}')):
            objects,rows,context=self.fixture();self.assertIn(at,rows)
            changed=dict(rows);changed[at]=replace(rows[at],operands=operand)
            with self.subTest(site=hex(at)),self.assertRaises(AssertionError):checksum_refill_validator_return(objects,changed,context)

    def test_missing_normal_child_paired_source_loop_and_return_cannot_complete(self):
        from config_validator_return import checksum_refill_validator_return
        from dataclasses import replace
        for label in ('missing_luminance','missing_text_return','source_Channel_pair','source_backedge','finite_flags'):
            objects,rows,context=self.fixture();changed=dict(rows)
            if label=='missing_luminance':changed.pop(0x1001b4b8+16)
            elif label=='missing_text_return':changed.pop(0x10028d3c+4)
            elif label=='source_Channel_pair':changed[0x1001f23e]=replace(rows[0x1001f23e],operands='r0, [sp, #0x20]')
            elif label=='source_backedge':changed[0x1001f9c6]=replace(rows[0x1001f9c6],operands=hex(0x1001f848))
            else:changed[0x10028d8a]=replace(rows[0x10028d8a],operands='r0, #0xff')
            with self.subTest(case=label),self.assertRaises(AssertionError):checksum_refill_validator_return(objects,changed,context)

    def test_child_cannot_read_range_padding_or_return_an_inactive_band(self):
        from config_validator_return import checksum_refill_validator_return
        from dataclasses import replace
        for at,operand in ((0x1002229c,'r2, [r0, #0x5]'),(0x1001c1e0,'r1, #0x5')):
            objects,rows,context=self.fixture();changed=dict(rows)
            changed[at]=replace(rows[at],operands=operand)
            with self.subTest(site=hex(at)),self.assertRaises(AssertionError):checksum_refill_validator_return(objects,changed,context)

    def test_reachable_internal_padding_is_a_coherent_encoded_counterexample(self):
        from config_validator_return import checksum_refill_validator_return
        objects,rows,context=self.fixture()
        # The reviewed source Access branch skips NOP+data. Encode two actual
        # Thumb16 B instructions: first enters the old pad; second leaves it.
        # Bytes/semantic records agree; this is synthetic encoding, not a fresh
        # LLVM decode or a linked executable build.
        for at,target in ((0x1001f8c4,0x1001f8c6),(0x1001f8c6,0x1001f90e)):
            delta=target-(at+4);self.assertEqual(delta%2,0);self.assertTrue(-2048<=delta<2048)
            raw=struct.pack('<H',0xe000|((delta//2)&0x7ff))
            StartupConfigurationCopy.put(objects,at,raw);rows[at]=Instruction(at,raw,'b',hex(target))
        self.assertEqual(objects.read_only(0x1001f8c4,4),bytes.fromhex('ffe722e0'))
        with self.assertRaises(AssertionError):checksum_refill_validator_return(objects,rows,SameBLoaderBoundary.rebound(objects,context))

    def test_deeper_validator_child_is_physically_protected_after_before_call_accepts(self):
        from config_refill_handoff import checksum_refill_handoff
        from config_validator_return import checksum_refill_validator_return
        objects,rows,context=self.fixture();P=0x20080000-544;Q=P-320
        objects.elf.sections.append((0,8,3,Q-64,0,8,0,0,4,0))
        # Old boundary reaches Q only; actual normal memcpy reaches Q-72.
        before=checksum_refill_handoff(objects,rows,context)
        self.assertEqual(before['before_validator']['result'],(P+236,12))
        with self.assertRaisesRegex(AssertionError,'physical frame intersects allocated section'):
            checksum_refill_validator_return(objects,rows,context)

    def test_independent_Reset_stack_readonly_object_and_nested_body_relocations(self):
        from config_validator_return import checksum_refill_validator_return
        from config_stack import canonical_symbol
        from dataclasses import replace
        for kind in ('stack','readonly_table','nested_body'):
            objects,rows,context=self.fixture();elf=objects.elf;S=0x20080000
            if kind=='stack':
                S=0x2007f000
                StartupConfigurationCopy.put(objects,elf.named['.vector_table'][3],struct.pack('<I',S))
                elf.symbol_entries=[(n,S if n=='_stack_start' else a,z,t,i) for n,a,z,t,i in elf.symbol_entries]
            else:
                # A fresh declared section, LOAD and bytes avoid borrowing
                # unrelated fixture slack. B/ROOT/task/stack remain fixed.
                new=0x10071002 if kind=='readonly_table' else 0x10070000
                old,n=(0x10034c34,64) if kind=='readonly_table' else (0x1001b4b8,72)
                content=objects.read_only(old,n);file_at=(len(elf.data)+3)&~3;index=len(elf.sections)
                elf.data=elf.data.ljust(file_at,b'\0')+content
                elf.sections.append((0,1,2 if kind=='readonly_table' else 6,new,file_at,n,0,0,2 if kind=='readonly_table' else 4,0))
                elf.segments.append((1,file_at,new,new,n,n,4 if kind=='readonly_table' else 5,2 if kind=='readonly_table' else 4))
                if kind=='readonly_table':
                    StartupConfigurationCopy.put(objects,0x1001b4b8+68,struct.pack('<I',new))
                    self.assertEqual(objects.read_only(new,n),content)
                else:
                    roles=[name for name,e in elf.functions.items() if e[0]&~1==old]
                    self.assertTrue(roles)
                    for name in roles:elf.functions[name]=(new|1,n,index)
                    elf.symbol_entries=[(name,new|1,z,t,index) if name in roles else (name,a,z,t,i)
                                        for name,a,z,t,i in elf.symbol_entries]
                    elf.mapping_symbols.extend((('$t',new,index),('$d',new+64,index)))
                    objects.sizes[new]=objects.sizes.pop(old)
                    for at in tuple(rows):
                        if old<=at<old+n:
                            row=rows.pop(at);rows[new+at-old]=replace(row,address=new+at-old)
                    for at in (0x1001b450,0x1001b458):
                        delta=new-(at+4);self.assertEqual(delta%2,0);self.assertTrue(-(1<<24)<=delta<(1<<24))
                        value=delta&0x1ffffff;s=(value>>24)&1;i1=(value>>23)&1;i2=(value>>22)&1
                        j1=1^(i1^s);j2=1^(i2^s)
                        raw=struct.pack('<HH',0xf000|(s<<10)|((value>>12)&0x3ff),
                                        0xd000|(j1<<13)|(j2<<11)|((value>>1)&0x7ff))
                        StartupConfigurationCopy.put(objects,at,raw);rows[at]=Instruction(at,raw,'bl',hex(new))
                    self.assertEqual(elf.functions[roles[0]][0]&~1,new)
                    self.assertTrue(all('luminance' in canonical_symbol(name) for name in roles))
            proof=checksum_refill_validator_return(objects,rows,SameBLoaderBoundary.rebound(objects,context))
            with self.subTest(kind=kind):
                self.assertEqual(proof['normal_return']['restored_SP'],S-544)
                self.assertEqual(proof['normal_return']['original_r8'],S-544+112)
                self.assertEqual(proof['normal_return']['return_to'],0x100203e6)
                self.assertEqual(proof['validator']['function'],0x1001ea20)
                self.assertEqual(proof['normal_child_low'],S-544-320-72)
                self.assertEqual(proof['normal_typed_call_count'],27)

    def test_public_result_has_no_internal_mutable_View_or_context_capability(self):
        from config_validator_return import checksum_refill_validator_return
        from config_regions import View,Storage
        from config_validator_access import ValidatorContext
        objects,rows,context=self.fixture();proof=checksum_refill_validator_return(objects,rows,context)
        def walk(value):
            self.assertNotIsInstance(value,(View,Storage,ValidatorContext))
            if isinstance(value,dict):
                for item in value.values():walk(item)
            elif isinstance(value,(tuple,list)):
                for item in value:walk(item)
        walk(proof)
        self.assertFalse(proof['normal_return']['architectural_LR_preservation_claimed'])
        self.assertFalse(proof['exception_absence_established'])

    def test_guarded_activations_pair_nested_returns_and_stop_before_diagnostic_effects(self):
        from config_validator_return import checksum_refill_validator_return
        objects,rows,context=self.fixture();proof=checksum_refill_validator_return(objects,rows,context)
        closure=proof['guard_stopped_normal_activation_closure']
        self.assertEqual(closure['return_to'],0x100203e6)
        self.assertEqual(closure['scope'],'selected ordinary execution before any pending diagnostic cut')
        contrasts=[n for n in closure['invocations'] if n['family']=='contrasts']
        self.assertTrue(contrasts)
        for parent in contrasts:
            nested=[n for n in closure['invocations'] if n['parent']==parent['identity'] and n['family']=='luminance']
            self.assertEqual({n['return_to']-parent['function'] for n in nested},{10,18})
            self.assertEqual(len({n['return_token'] for n in nested}),2)
        restores=[n for n in closure['invocations'] if n['family']=='text_restore']
        self.assertTrue(restores)
        self.assertTrue(all(n['entry_SP']+32==n['return_SP'] and n['transfer']=='tail' for n in restores))
        self.assertTrue(closure['pending_diagnostic_cuts'])
        for cut in closure['pending_diagnostic_cuts']:
            self.assertEqual(cut['kind'],'pending_diagnostic_cut')
            self.assertIsNone(cut['return_token'])
            self.assertFalse(cut['machine_nonreturn_established'])

    def test_disconnected_foreign_table_is_bound_but_cannot_be_activated_by_retarget(self):
        from config_validator_return import checksum_refill_validator_return
        objects,rows,context=self.fixture();elf=objects.elf;at=0x10010000
        raw=bytes.fromhex('dfe810f000007047')
        index=next(i for i,s in enumerate(elf.sections) if s[2]&6==6 and s[3]<=at<at+len(raw)<=s[3]+s[5])
        StartupConfigurationCopy.put(objects,at,raw)
        elf.functions['foreign_fixture_table']=(at|1,len(raw),index)
        elf.symbol_entries.append(('foreign_fixture_table',at|1,len(raw),2,index))
        elf.mapping_symbols.extend((('$t',at,index),('$d',at+4,index),('$t',at+6,index)))
        rows[at]=Instruction(at,raw[:4],'tbh','[pc, r0, lsl #1]')
        rows[at+6]=Instruction(at+6,raw[6:],'bx','lr')
        bound=SameBLoaderBoundary.rebound(objects,context)
        proof=checksum_refill_validator_return(objects,rows,bound)
        outside=[v for v in proof['computed_exclusions'] if v['site']==at]
        self.assertEqual(len(outside),1)
        self.assertEqual(outside[0]['status'],'outside_selected_guard_stopped_prefix')
        self.assertEqual(outside[0]['raw'],'dfe810f0')
        self.assertFalse(outside[0]['foreign_context_validated'])
        # Coherent Thumb2 BL retarget from an activated contrast caller.
        site=0x1001b450;delta=at-site-4;value=delta&0x1ffffff
        s=(value>>24)&1;i1=(value>>23)&1;i2=(value>>22)&1
        encoded=struct.pack('<HH',0xf000|(s<<10)|((value>>12)&0x3ff),
            0xd000|((1^(i1^s))<<13)|((1^(i2^s))<<11)|((value>>1)&0x7ff))
        StartupConfigurationCopy.put(objects,site,encoded);rows[site]=Instruction(site,encoded,'bl',hex(at))
        with self.assertRaises(AssertionError):
            checksum_refill_validator_return(objects,rows,SameBLoaderBoundary.rebound(objects,context))

    def test_normal_child_PC_and_parent_owned_tail_changes_cannot_become_pending_cuts(self):
        from config_validator_return import checksum_refill_validator_return
        from dataclasses import replace
        for at,operation,operand in ((0x1001c148+60,'bl',hex(0x10028d3c)),
            (0x10028d3c,'pop.w','{r8, r9, r10}'),(0x1001b450,'bl',hex(0x1001b45c)),
            (0x1001b4b8+12,'mov','pc, r4')):
            objects,rows,context=self.fixture();self.assertIn(at,rows)
            changed=dict(rows);changed[at]=replace(rows[at],mnemonic=operation,operands=operand)
            with self.subTest(site=hex(at)),self.assertRaises(AssertionError):
                checksum_refill_validator_return(objects,changed,context)

    def test_active_parent_table_bytes_cannot_change_the_source_valid_tuple(self):
        from config_validator_return import checksum_refill_validator_return
        objects,rows,context=self.fixture();at=0x1001f164+2
        original=objects.read_only(at,1);StartupConfigurationCopy.put(objects,at,bytes([original[0]^1]))
        # A data mutation leaves all decoded instruction bytes coherent; the
        # current readonly table/source contract must reject its new target.
        with self.assertRaises(AssertionError):
            checksum_refill_validator_return(objects,rows,SameBLoaderBoundary.rebound(objects,context))

    def test_control_table_and_parent_owned_restoring_tail_relocate_by_current_relationships(self):
        from config_validator_return import checksum_refill_validator_return
        from config_stack import canonical_symbol
        from dataclasses import replace
        def branch(site,target,link):
            value=(target-site-4)&0x1ffffff;s=(value>>24)&1;i1=(value>>23)&1;i2=(value>>22)&1
            return struct.pack('<HH',0xf000|(s<<10)|((value>>12)&0x3ff),
                (0xd000 if link else 0x9000)|((1^(i1^s))<<13)|((1^(i2^s))<<11)|((value>>1)&0x7ff))
        for kind in ('control_table','text_restore','validator_restore'):
            objects,rows,context=self.fixture();elf=objects.elf
            if kind=='control_table':
                names=[n for n in elf.functions if canonical_symbol(n).endswith('ControlAction10allowed_on')]
                self.assertEqual(len(names),1);old=elf.functions[names[0]][0]&~1
                partitions=((0,'$t'),(12,'$d'),(32,'$t'));new=0x10072000
            else:
                parent=0x1001c148+60 if kind=='text_restore' else 0x1001eb82
                self.assertIn(parent,rows);old=int(rows[parent].operands.split('@',1)[0].split('<',1)[0].strip(),16)
                names=[n for n,e in elf.functions.items() if e[0]&~1==old]
                partitions=((0,'$t'),);new=0x10072100
            self.assertTrue(names);n=elf.functions[names[0]][1];content=objects.read_only(old,n)
            file_at=(len(elf.data)+3)&~3;index=len(elf.sections)
            elf.data=elf.data.ljust(file_at,b'\0')+content
            elf.sections.append((0,1,6,new,file_at,n,0,0,4,0));elf.segments.append((1,file_at,new,new,n,n,5,4))
            for name in names:elf.functions[name]=(new|1,n,index)
            elf.symbol_entries=[(name,new|1,z,t,index) if name in names else (name,a,z,t,i)
                                for name,a,z,t,i in elf.symbol_entries]
            elf.mapping_symbols.extend((label,new+off,index) for off,label in partitions)
            if old in objects.sizes:objects.sizes[new]=objects.sizes.pop(old)
            for at in tuple(rows):
                if old<=at<old+n:
                    row=rows.pop(at);operand=row.operands.split('@',1)[0].split('<',1)[0].strip()
                    if row.mnemonic.startswith(('b','cb')):
                        found=re.fullmatch(r'(0x[0-9a-f]+)',operand)
                        if found:
                            target=int(found[1],16);local=old<=target<old+n;target=target+new-old if local else target
                            operand=hex(target)
                            if not local:
                                raw=branch(new+at-old,target,row.mnemonic=='bl')
                                StartupConfigurationCopy.put(objects,new+at-old,raw);row=replace(row,raw=raw)
                    rows[new+at-old]=replace(row,address=new+at-old,operands=operand)
            for at,row in tuple(rows.items()):
                if new<=at<new+n:continue
                if row.mnemonic in ('bl','b.w') and row.operands.split('@',1)[0].split('<',1)[0].strip()==hex(old):
                    raw=branch(at,new,row.mnemonic=='bl');StartupConfigurationCopy.put(objects,at,raw)
                    rows[at]=Instruction(at,raw,row.mnemonic,hex(new))
            proof=checksum_refill_validator_return(objects,rows,SameBLoaderBoundary.rebound(objects,context))
            nodes=proof['guard_stopped_normal_activation_closure']['invocations']
            moved=[v for v in nodes if v['function']==new];self.assertTrue(moved)
            with self.subTest(kind=kind):
                if kind=='control_table':
                    table=[v for v in proof['computed_exclusions'] if v['site']==new+8]
                    self.assertEqual(table[0]['status'],'selected_current_source_targets')
                    self.assertTrue(all(new<=v<new+n for v in table[0]['targets']))
                else:
                    self.assertTrue(all(v['transfer']=='tail' for v in moved))
                    self.assertTrue(all(v['entry_SP']<v['return_SP'] for v in moved))

    def test_preserved_flags_from_an_isolated_child_seam_cannot_replace_its_return_cells(self):
        import config_validator_return as public
        from unittest.mock import patch
        from config_validator_children import _normal_body
        from dataclasses import replace
        objects,rows,context=self.fixture()
        name=next(n for n,e in objects.elf.functions.items() if e[0]&~1==0x1001b44a)
        original=_normal_body(objects,name,rows,'contrasts')
        # Supplied/mocked component seam: keep a previously checked proof and
        # its preservation claims while changing the nested return record.
        # The enclosing activation still must consume actual SP/saved-PC.
        changed=dict(rows);at=0x1001b4b8+62
        changed[at]=replace(rows[at],mnemonic='bx',operands='lr')
        actual=public._normal_body
        def supplied(objects,name,records,family):
            return original if family=='contrasts' else actual(objects,name,records,family)
        with patch.object(public,'_normal_body',side_effect=supplied),self.assertRaisesRegex(
            AssertionError,'exact SP/save step'):
            public.checksum_refill_validator_return(objects,changed,context)

    def test_decoded_Result_epoch_and_forged_source_binding_cannot_enter_guarded_default(self):
        from config_validator_return import checksum_refill_validator_return
        from dataclasses import replace
        objects,rows,context=self.fixture();at=0x100203d8
        self.assertEqual(rows[at].mnemonic,'add')
        changed=dict(rows);changed[at]=replace(rows[at],operands='r4, sp, #0x70')
        with self.assertRaises(AssertionError):checksum_refill_validator_return(objects,changed,context)
        # The original source context is bound to the original bytes, so even
        # an otherwise disconnected data-byte change cannot reuse that input.
        StartupConfigurationCopy.put(objects,0x10010000,b'\x00\xbf')
        with self.assertRaises(AssertionError):checksum_refill_validator_return(objects,rows,context)


class ActualChecksumLoaderReturn(unittest.TestCase):
    def fixture(self):
        objects,rows,context=ChecksumValidatorReturn().fixture()
        item=json.loads((Path(__file__).parent/'fixtures/config-loader-continuation.json').read_text())
        for index,address,encoded in item['segments']:StartupConfigurationCopy.put(objects,address,bytes.fromhex(encoded))
        objects.elf.functions.update({n:tuple(v) for n,v in item['functions'].items()})
        objects.sizes.update({int(a):n for a,n in item['sizes'].items()})
        objects.elf.symbol_entries=list(dict.fromkeys(objects.elf.symbol_entries+[tuple(v) for v in item['symbol_entries']]))
        for r in item['records']:
            parts=r['text'].split(None,1);a=r['address']
            rows[a]=Instruction(a,bytes.fromhex(r['bytes']),parts[0],parts[1] if len(parts)>1 else '')
        return objects,rows,SameBLoaderBoundary.rebound(objects,context)

    def consume(self,objects,rows,context):
        from config_actual_loader_return import checksum_refill_loader_return
        return checksum_refill_loader_return(objects,rows,context)

    def test_same_private_chain_pairs_Result_fault_issuer_and_protected_Boot_return(self):
        import importlib.util
        self.assertIsNotNone(importlib.util.find_spec('config_actual_loader_return'),'missing actual private loader continuation')
        proof=self.consume(*self.fixture());P=0x20080000-544
        self.assertEqual(proof['phase'],'checksum_refill_loader_return_pending_diagnostics_and_other_loader_routes')
        self.assertEqual(proof['return']['SP'],P+456)
        self.assertEqual(proof['return']['saved_PC_slot'],P+452)
        self.assertEqual(proof['return']['PC']|1,proof['return']['saved_value'])
        self.assertEqual(proof['fault_installation']['constructor'],'ConfigError::Checksum')
        self.assertEqual(proof['result_consumption']['tags'],(0,1,2))
        self.assertEqual({(v['status'],v['identity']) for v in proof['publication_pairs']},
                         {(0,'nonzero successfully issued previous'),(2,0)})
        self.assertTrue(proof['issuer_retry']['mismatch_and_store_failure'])
        self.assertFalse(proof['issuer_retry']['termination_established'])
        self.assertEqual(proof['return']['r0'],'issued previous identity or zero; not Config pointer')
        self.assertFalse(proof['hardware_qualified']);self.assertNotIn('acquired_owner',proof)

    def test_guard_retirement_preserves_Config_cache_padding_taken_and_outer_padding(self):
        proof=self.consume(*self.fixture());B=0x2000031c
        self.assertIn((B,14576),proof['protected_static_ranges'])
        self.assertIn((B+14585,3),proof['protected_static_ranges'])
        self.assertIn((B+14600,4),proof['protected_static_ranges'])
        self.assertEqual(set(proof['metadata_writes']),{(B+14576,4),(B+14580,4),(B+14584,1),
            (B+14588,4),(B+14592,4),(B+14596,4)})
        for a,n in proof['metadata_writes']:
            for p,z in proof['protected_static_ranges']:self.assertFalse(a<p+z and p<a+n)

    def test_Result_fault_and_stoich_transports_cannot_be_replaced_by_neighbor_cells(self):
        from dataclasses import replace
        for at,operand in ((0x100203e6,'r0, [sp, #0x70]'),(0x10020404,'r1, [sp, #0xf4]'),
            (0x10020414,'r0, r1, #0x0'),(0x100209aa,'r8, {r0, r1, r2}')):
            objects,rows,context=self.fixture();self.assertIn(at,rows)
            rows[at]=replace(rows[at],operands=operand)
            with self.subTest(site=hex(at)),self.assertRaises(AssertionError):self.consume(objects,rows,context)

    def test_coherent_stoich_load_image_and_record_change_rejects_the_actual_transport(self):
        from config_build_source import validate_source_context
        objects,rows,context=self.fixture()
        positive=self.consume(objects,rows,context)
        self.assertEqual(positive['phase'],'checksum_refill_loader_return_pending_diagnostics_and_other_loader_routes')
        original=objects.elf.data;at=0x10020404;old=rows[at]
        # Thumb16 LDR (SP-relative): Rt1, imm8 scaled by4. Both image and
        # record change P240 (meaningful Ok f32) -> neighboring P244.
        # This is manually encoded coherent synthetic evidence, no decoder.
        self.assertEqual((old.mnemonic,old.operands),('ldr','r1, [sp, #0xf0]'))
        self.assertEqual(old.raw,struct.pack('<H',0x9800|(1<<8)|(240//4)))
        changed=struct.pack('<H',0x9800|(1<<8)|(244//4))
        StartupConfigurationCopy.put(objects,at,changed)
        rows[at]=Instruction(at,changed,'ldr','r1, [sp, #0xf4]')
        self.assertEqual(objects.read_only(at,2),rows[at].raw)
        rebound=SameBLoaderBoundary.rebound(objects,context)
        self.assertEqual(validate_source_context(rebound,objects.elf),rebound)
        with self.assertRaisesRegex(AssertionError,'^payload reference/cell/ABI/default/return operands differ$'):
            self.consume(objects,rows,rebound)
        section=next(s for s in objects.elf.sections if s[3]<=at<at+2<=s[3]+s[5])
        offset=section[4]+at-section[3]
        self.assertEqual(original[offset:offset+2],old.raw)
        self.assertNotEqual(original,objects.elf.data)

    def test_issuer_compare_retry_overflow_and_publication_pair_are_current_operands(self):
        from dataclasses import replace
        for at,operand in ((0x10020434,'r0, r2'),(0x1002043c,'r0, '+hex(0x1002044a)),
            (0x10020450,'r0, #0x1'),(0x10020464,'r0, [r3, r2]')):
            objects,rows,context=self.fixture();self.assertIn(at,rows)
            rows[at]=replace(rows[at],operands=operand)
            with self.subTest(site=hex(at)),self.assertRaises(AssertionError):self.consume(objects,rows,context)

    def test_diagnostic_cuts_and_foreign_suffix_ingress_never_issue_return_tokens(self):
        from dataclasses import replace
        proof=self.consume(*self.fixture())
        cuts=proof['continuation_closure']['pending_diagnostic_cuts']
        self.assertEqual({v['kind'] for v in cuts},{'pending_diagnostic_cut'})
        for cut in cuts:self.assertIsNone(cut['return_token']);self.assertFalse(cut['machine_nonreturn_established'])
        for site,target in ((0x1001feee,0x100209c8),(0x10060000,0x100209d0)):
            objects,rows,context=self.fixture();rows[site]=Instruction(site,b'\x00\xbf','b',hex(target))
            with self.subTest(target=hex(target)),self.assertRaises(AssertionError):self.consume(objects,rows,context)
        objects,rows,context=self.fixture();at=0x100209d2
        rows[at]=replace(rows[at],mnemonic='mov',operands='pc, r0')
        with self.assertRaises(AssertionError):self.consume(objects,rows,context)

    def test_loader_restore_and_saved_PC_cannot_borrow_another_frame_or_return(self):
        from dataclasses import replace
        for at,operation,operand in ((0x1002046c,'add','sp, #0x164'),(0x1002846c,'pop.w','{r8, r9, r10}'),
            (0x10028470,'pop','{r4, r5, r6, r7, lr}'),(0x10020472,'bl',hex(0x1002846a))):
            objects,rows,context=self.fixture();rows[at]=replace(rows[at],mnemonic=operation,operands=operand)
            with self.subTest(site=hex(at)),self.assertRaises(AssertionError):self.consume(objects,rows,context)

    def test_Reset_S_and_issuer_pool_relocations_are_independent_of_B_and_code(self):
        for kind in ('stack','issuer_pool','issuer_object','restore_code'):
            objects,rows,context=self.fixture();S=0x20080000
            if kind=='stack':
                S-=0x1000;StartupConfigurationCopy.put(objects,objects.elf.named['.vector_table'][3],struct.pack('<I',S))
                objects.elf.symbol_entries=[(n,S if n=='_stack_start' else a,z,t,i) for n,a,z,t,i in objects.elf.symbol_entries]
            elif kind=='issuer_pool':
                # Swap the two consumed words and independently update their
                # LDRs; neither the issuer nor expect-message producer is lost.
                at=0x10020424;row=rows[at];offset=int(row.operands.split('#')[1].split(']')[0],0)+4
                slot=((at+4)&~3)+offset;old=slot-4;left=objects.read_only(old,4);right=objects.read_only(slot,4)
                StartupConfigurationCopy.put(objects,slot,left);StartupConfigurationCopy.put(objects,old,right)
                for site,reg,pool in ((at,1,slot),(0x100209c8,0,old)):
                    immediate=pool-((site+4)&~3);encoded=struct.pack('<HH',0xf8df,(reg<<12)|immediate)
                    StartupConfigurationCopy.put(objects,site,encoded)
                    rows[site]=Instruction(site,encoded,'ldr.w','r'+str(reg)+', [pc, #'+hex(immediate)+']')
            elif kind=='issuer_object':
                from config_entry import ISSUER,static_role
                old,new=0x200127a0,0x200127a4
                s=objects.elf.named['.data'];left=s[4]+old-s[3];right=s[4]+new-s[3]
                initial=objects.elf.data[left:left+4];other=objects.elf.data[right:right+4]
                StartupConfigurationCopy.put(objects,old,other);StartupConfigurationCopy.put(objects,new,initial)
                objects.elf.symbol_entries=[(n,new if static_role(n)==ISSUER else old if a==new and t&15==1 else a,z,t,i)
                    for n,a,z,t,i in objects.elf.symbol_entries]
                StartupConfigurationCopy.put(objects,0x10020ed0,struct.pack('<I',new))
            else:
                elf=objects.elf;new=0x10072000;code=objects.read_only(0x1002846a,8)
                file_at=(len(elf.data)+3)&~3;index=len(elf.sections);elf.data=elf.data.ljust(file_at,b'\0')+code
                elf.sections.append((0,1,6,new,file_at,8,0,0,4,0));elf.segments.append((1,file_at,new,new,8,8,5,4))
                name='OUTLINED_FUNCTION_9999';elf.functions[name]=(new|1,8,index)
                elf.symbol_entries.append((name,new|1,8,2,index));elf.mapping_symbols.append(('$t',new,index));objects.sizes[new]=0
                for at in (0x1002846a,0x1002846c,0x10028470):
                    row=rows[at];rows[new+at-0x1002846a]=Instruction(new+at-0x1002846a,row.raw,row.mnemonic,row.operands)
                at=0x10020472;low,high=struct.unpack('<HH',StartupConfigurationCopy.branch_link(at,new))
                encoded=struct.pack('<HH',low,high&~0x4000);StartupConfigurationCopy.put(objects,at,encoded)
                rows[at]=Instruction(at,encoded,'b.w',hex(new))
            proof=self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))
            with self.subTest(kind=kind):self.assertEqual(proof['return']['SP'],S-88)

    def test_issuer_wrong_object_and_cross_space_physical_alias_cannot_gain_write_permission(self):
        from config_entry import ISSUER,static_role
        for kind in ('another_object','stack_alias'):
            objects,rows,context=self.fixture()
            target=0x200127a4 if kind=='another_object' else 0x20080000-92
            if kind=='stack_alias':
                objects.elf.symbol_entries=[(n,target if static_role(n)==ISSUER else a,z,t,i)
                    for n,a,z,t,i in objects.elf.symbol_entries]
            StartupConfigurationCopy.put(objects,0x10020ed0,struct.pack('<I',target))
            with self.subTest(kind=kind),self.assertRaises(AssertionError):
                self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))

    def test_active_catalogue_target_and_cache_or_taken_suffix_writes_reject(self):
        from dataclasses import replace
        for fault in ('table','cache_padding','taken_boundary'):
            objects,rows,context=self.fixture()
            if fault=='table':
                table=0x1001be30+48;target=0x10020424
                StartupConfigurationCopy.put(objects,table,struct.pack('<H',(target-table)//2))
                context=SameBLoaderBoundary.rebound(objects,context)
            else:
                at=0x10020460 if fault=='cache_padding' else 0x100203f2
                rows[at]=replace(rows[at],operands='r2, #0x38f9' if fault=='cache_padding' else 'r0, #0x3905')
            with self.subTest(fault=fault),self.assertRaises(AssertionError):self.consume(objects,rows,context)

    def test_independent_B_relocation_reaches_actual_continuation_and_rejects_one_stale_producer(self):
        from config_entry import ISSUER,static_role
        donor,more,unused=self.fixture();delta=0x4000
        objects,prefix,context,item=SameBLoaderBoundary().fixture('sizez',delta);elf=objects.elf
        # Reuse the reviewed relocated startup/task/B fixture, then compose
        # complete readonly consuming bodies without copying stale RAM images.
        pools={}
        for at,row in prefix.items():
            match=re.search(r'\[pc, #(0x[0-9a-f]+|[0-9]+)\]',row.operands)
            if row.mnemonic in ('ldr','ldr.w') and match:
                slot=((at+4)&~3)+int(match[1],0);pools[slot]=objects.read_only(slot,4)
        for section in donor.elf.sections:
            if section[1]==1 and section[2]&2 and not section[2]&1 and section[5]:
                StartupConfigurationCopy.put(objects,section[3],donor.elf.data[section[4]:section[4]+section[5]])
        for at,row in prefix.items():StartupConfigurationCopy.put(objects,at,row.raw)
        for at,value in pools.items():StartupConfigurationCopy.put(objects,at,value)
        rows=dict(more);rows.update(prefix)
        for n,(a,z,i) in donor.elf.functions.items():
            elf.functions[n]=(a+(delta if donor.elf.sections[i][2]&1 else 0),z,i)
        objects.sizes.update(donor.sizes)
        shifted=[(n,a+(delta if 0<=i<len(donor.elf.sections) and donor.elf.sections[i][2]&1 else 0),z,t,i)
                 for n,a,z,t,i in donor.elf.symbol_entries]
        elf.symbol_entries=list(dict.fromkeys(elf.symbol_entries+shifted))
        elf.mapping_symbols=list(dict.fromkeys(elf.mapping_symbols+[(n,a+(delta if donor.elf.sections[i][2]&1 else 0),i)
            for n,a,i in donor.elf.mapping_symbols]))
        B=item['config']+delta
        for parent in item['parents']:
            for kind,lo,hi,register in parent['receivers']:
                self.assertEqual(kind,'pc_literal')
                StartupConfigurationCopy.put(objects,parent['address']+hi,struct.pack('<I',B))
        issuer=next(a for n,a,z,t,i in elf.symbol_entries if static_role(n)==ISSUER)
        self.assertEqual(issuer,0x200127a0+delta)
        StartupConfigurationCopy.put(objects,0x10020ed0,struct.pack('<I',issuer))
        proof=self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))
        self.assertIn((B,14576),proof['protected_static_ranges'])
        self.assertEqual(proof['issuer_retry']['owner'],(issuer,4))
        self.assertEqual(proof['return']['SP'],0x20080000-88)
        self.assertEqual(proof['validator']['validator']['function'],0x1001ea20)
        self.assertEqual(set(proof['metadata_writes']),{(B+14576,4),(B+14580,4),(B+14584,1),
            (B+14588,4),(B+14592,4),(B+14596,4)})
        # Coherently stale publication receiver with every other producer
        # still selecting relocated B. This is a data/pointer negative.
        site=0x1002045c;match=re.search(r'#(0x[0-9a-f]+|[0-9]+)',rows[site].operands)
        slot=((site+4)&~3)+int(match[1],0)
        StartupConfigurationCopy.put(objects,slot,struct.pack('<I',item['config']))
        with self.assertRaises(AssertionError):self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))

    def test_supplied_owner_or_description_cannot_replace_same_invocation(self):
        objects,rows,context=self.fixture()
        with self.assertRaises(AssertionError):self.consume(objects,rows,dict(phase='returned',owner=True))
        from config_actual_loader_return import checksum_refill_loader_return
        with self.assertRaises(TypeError):checksum_refill_loader_return(objects,rows,context,dict(completed=True))




class ActualDecodedU16Prefix(unittest.TestCase):
    def fixture(self,delta=0):
        objects,rows,context,item=SameBLoaderBoundary().fixture('sizez',delta);elf=objects.elf
        donor,more,root,visitor,frame,child=OrdinaryFirstFieldBridge().fixture()
        # Preserve the already admitted entry/static-producer bytes and their
        # actual pools while adding only the readonly header/helper fixture.
        pools={}
        for at,row in rows.items():
            m=re.search(r'\[pc, #(0x[0-9a-f]+|[0-9]+)\]',row.operands)
            if row.mnemonic in ('ldr','ldr.w') and m:
                slot=((at+4)&~3)+int(m[1],0);pools[slot]=objects.read_only(slot,4)
        for file in ('config-headers.json','config-small-helpers.json'):
            fixture=json.loads((Path(__file__).parent/'fixtures'/file).read_text())[0]
            for index,at,encoded in fixture['segments']:
                StartupConfigurationCopy.put(objects,at,bytes.fromhex(encoded))
        for at,row in rows.items():StartupConfigurationCopy.put(objects,at,row.raw)
        for at,value in pools.items():StartupConfigurationCopy.put(objects,at,value)
        combined=dict(more);combined.update(rows);rows=combined
        elf.functions.update(donor.elf.functions);objects.sizes.update(donor.sizes)
        elf.symbol_entries=list(dict.fromkeys(elf.symbol_entries+donor.elf.symbol_entries))
        elf.mapping_symbols=list(dict.fromkeys(elf.mapping_symbols+donor.elf.mapping_symbols))
        return objects,rows,SameBLoaderBoundary.rebound(objects,context),item

    @staticmethod
    def consume(objects,rows,context):
        from config_decoded_prefix import first_boot_decoded_u16_prefix
        return first_boot_decoded_u16_prefix(objects,rows,context)

    def test_unknown_u16s_have_paired_prefix_counts_cursors_and_preserved_remainder(self):
        objects,rows,context,item=self.fixture();result=self.consume(objects,rows,context)
        self.assertEqual(result['phase'],'first_boot_decoded_u16_prefix_pending_remaining_visitor_canonical_and_loader_routes')
        exits=sorted(result['outcomes'],key=lambda v:v['completed_fields']);self.assertEqual(len(exits),3)
        self.assertEqual([(v['completed_fields'],v['count'],v['cursor'],v['error']) for v in exits],
            [(0,(12,12),(3,3),5),(1,(11,11),(4,6),5),(2,(11,11),(2,6),None)])
        self.assertEqual([v['frontier'] for v in exits],[0x1001c68a,0x1001c68a,0x1001c6a6])
        success=exits[-1];self.assertEqual(success['field_domains'],(('version',(0,65535)),('catalogue_version',(0,65535))))
        self.assertEqual(success['initial_facts_retired'],((14570,14572),(14572,14574)))
        self.assertEqual(success['initial_active_bytes_preserved'],180) # taken1 replaces one of185 before decode; two u16 retire4.
        self.assertEqual(result['initial_collection_lengths'],(0,0));self.assertFalse(result['hardware_qualified'])
        self.assertNotIn('acquired_owner',result);self.assertNotIn('validated_config',result)
        self.assertEqual(result['nested_return_sites'][0]['destination'],0x1001c670)
        self.assertEqual(result['nested_return_sites'][1]['destination'],0x1001c686)
        self.assertEqual(result['deepest_child_SP'],0x20080000-544-1608-8-8-24-16)

    def test_source_context_and_same_B_producers_cannot_be_supplied_or_substituted(self):
        objects,rows,context,item=self.fixture()
        for fake in (None,dict(owner=True,phase='decoded'),context.to_record()):
            with self.subTest(fake=type(fake).__name__),self.assertRaises(AssertionError):self.consume(objects,rows,fake)
        # Existing actual static-point binding catches a coherent alternate B.
        StartupConfigurationCopy.put(objects,0x10020d74,struct.pack('<I',item['config']+4))
        with self.assertRaises(AssertionError):self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))

    def test_field_store_error_guard_and_nested_return_changes_reject(self):
        for at,operation,operands in ((0x1001c67e,'str','r0, [r4, r1]'),
            (0x1001c686,'lsls','r1, r0, #0x1e'),(0x1001c680,'add','r0, sp, #0x124'),
            (0x1001bdb2,'pop','{r4, pc}'),(0x1001bda8,'and.w','r0, r2, r0, lsr #7')):
            objects,rows,context,item=self.fixture();row=rows[at];rows[at]=Instruction(at,row.raw,operation,operands)
            with self.subTest(at=hex(at)),self.assertRaises(AssertionError):self.consume(objects,rows,context)

    def test_foreign_interior_computed_and_implicit_PC_cannot_enter_selected_prefix(self):
        for operation,operands in (('b','0x1001c67e'),('tbb','[pc, r0]'),('mov','pc, r4'),('bxns','r4'),('svcne','#0x0')):
            objects,rows,context,item=self.fixture();at=0x1001c6a8 if operation!='tbb' else 0x1001c690
            rows[at]=Instruction(at,bytes.fromhex('dfe800f0') if operation=='tbb' else bytes(2),operation,operands)
            with self.subTest(operation=operation),self.assertRaises(AssertionError):self.consume(objects,rows,context)

    def test_nested_physical_stack_and_protected_suffix_cannot_alias_an_object(self):
        for address in (0x20080000-2200,0x20080000-544-1608-52):
            objects,rows,context,item=self.fixture();index=next(i for i,s in enumerate(objects.elf.sections) if s[2]&2)
            objects.elf.symbol_entries.append(('physical_child_alias',address,4,17,index))
            with self.subTest(address=hex(address)),self.assertRaisesRegex(AssertionError,'physical'):
                self.consume(objects,rows,context)

        objects,rows,context,item=self.fixture()
        low=0x20080000-544-1608-56;index=next(i for i,s in enumerate(objects.elf.sections) if s[2]&2)
        objects.elf.symbol_entries.append(('adjacent_below_child',low-4,4,17,index))
        self.assertEqual(self.consume(objects,rows,context)['deepest_child_SP'],low)

    def test_independent_B_and_Reset_SP_relocation_reaches_new_public_consumer(self):
        objects,rows,context,item=self.fixture(0x4000);B=item['config']+0x4000
        result=self.consume(objects,rows,context);self.assertEqual(result['config'],(B,14604))
        self.assertEqual(result['possible_field_writes'],((B+14570,2),(B+14572,2)))
        sp=0x2007f800;StartupConfigurationCopy.put(objects,objects.elf.named['.vector_table'][3],struct.pack('<I',sp))
        objects.elf.symbol_entries=[(n,sp if n=='_stack_start' else a,z,t,i) for n,a,z,t,i in objects.elf.symbol_entries]
        moved=self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))
        self.assertEqual(moved['root_frame'],(sp-544,456));self.assertEqual(moved['visitor_frame'],(sp-2152,1608))
        self.assertEqual(moved['config'],result['config']);self.assertEqual(moved['possible_field_writes'],result['possible_field_writes'])


    def test_independent_root_code_and_consumed_B_pool_relocation(self):
        from config_headers import ROOT
        from config_stack import canonical_symbol
        objects,rows,context,item=self.fixture();elf=objects.elf
        name=next(n for n in elf.functions if canonical_symbol(n)==ROOT);pointer,extent,index=elf.functions[name]
        old=pointer&~1;new=0x10071000;file_at=(len(elf.data)+3)&~3;section=elf.sections[index]
        raw=elf.data[section[4]+old-section[3]:section[4]+old-section[3]+extent]
        elf.data+=bytes(file_at-len(elf.data))+raw;newindex=len(elf.sections)
        elf.sections.append((0,1,6,new,file_at,extent,0,0,4,0));elf.segments.append((1,file_at,new,new,extent,extent,5,4))
        elf.functions[name]=(new|1,extent,newindex);objects.sizes[new]=objects.sizes.pop(old)
        elf.symbol_entries=[(n,new|1,z,t,newindex) if n==name else (n,a,z,t,i) for n,a,z,t,i in elf.symbol_entries]
        elf.mapping_symbols.extend((n,a+new-old,newindex) for n,a,i in list(elf.mapping_symbols) if i==index and old<=a<old+extent)
        for at,row in list(rows.items()):
            if not old<=at<old+extent:continue
            del rows[at];site=at+new-old;op=row.operands;encoded=row.raw
            family=config_stack.transfer_family(row.mnemonic.removesuffix('.w'))
            if family in ('b','bl','cbz','cbnz'):
                m=re.search(r'0x([0-9a-f]+)',op);target=int(m[1],16)
                if old<=target<old+extent:target+=new-old
                elif family in ('b','bl'):
                    encoded=StartupConfigurationCopy.branch_link(site,target)
                    if family=='b':lo,hi=struct.unpack('<HH',encoded);encoded=struct.pack('<HH',lo,hi&~0x4000)
                op=op[:m.start()]+hex(target)+op[m.end():]
            rows[site]=Instruction(site,encoded,row.mnemonic,op);StartupConfigurationCopy.put(objects,site,encoded)
        # Actual once tail follows relocated root, with B/Reset/visitor fixed.
        oldtail=next(at for at,r in rows.items() if r.mnemonic=='b.w' and re.match(re.escape(hex(old))+r'(?:\s|$)',r.operands) is not None)
        lo,hi=struct.unpack('<HH',StartupConfigurationCopy.branch_link(oldtail,new));encoded=struct.pack('<HH',lo,hi&~0x4000)
        StartupConfigurationCopy.put(objects,oldtail,encoded);rows[oldtail]=Instruction(oldtail,encoded,'b.w',hex(new))
        result=self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))
        self.assertEqual(result['guarded_CRC_success'][0],new+114)
        self.assertEqual(result['config'],(item['config'],14604));self.assertEqual(result['visitor_frame'],(0x2007f798,1608))
        # Move visitor's actually consumed B pool independently of both code
        # and object. This coherent encoding still selects the same typed B.
        site=0x1001c674;row=rows[site];m=re.search(r'#(0x[0-9a-f]+|[0-9]+)',row.operands)
        oldpool=((site+4)&~3)+int(m[1],0);pool=oldpool+4;immediate=pool-((site+4)&~3)
        encoded=struct.pack('<HH',0xf8df,0x4000|immediate)
        StartupConfigurationCopy.put(objects,site,encoded);StartupConfigurationCopy.put(objects,pool,struct.pack('<I',item['config']))
        rows[site]=Instruction(site,encoded,'ldr.w','r4, [pc, #'+hex(immediate)+']')
        moved=self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))
        self.assertEqual(moved['possible_field_writes'],result['possible_field_writes'])

    def test_abstract_outer_adapter_keeps_none_error_and_unknown_success_ABIs_distinct(self):
        from dataclasses import replace
        from config_abstract import DecoderInterface,SequenceInterface,Span
        from config_headers import root_visitor_headers,_next_u16_interface,NEXT_U16
        from config_regions import Storage
        objects,rows,root,visitor,frame,child=OrdinaryFirstFieldBridge().fixture()
        headers=root_visitor_headers(objects,root,rows,frame,());before=headers['visitor']['sequence_state'];d=before.decoder
        state=SequenceInterface(before.header,DecoderInterface(d.header,d.payload,Span(d.payload.extent,d.payload.extent)),Span(0,0))
        name=next(n for n in objects.elf.functions if config_stack.canonical_symbol(n)==NEXT_U16)
        nested=Storage(frame.space,'abstract wrapper',child.start-8,8,8,frame.phase,True)
        none=_next_u16_interface(objects,name,rows,state,nested,headers['visitor']['protected'])['outcomes']
        self.assertEqual([(v['error'],v['packed_r0'],v['state'].count) for v in none],[(14,0xe01,Span(0,0))])
        state=replace(state,count=Span(1,1));error=_next_u16_interface(objects,name,rows,state,nested,headers['visitor']['protected'])['outcomes']
        self.assertEqual([(v['error'],v['packed_r0'],v['state'].count) for v in error],[(4,0x401,Span(0,0))])
        state=replace(state,decoder=replace(state.decoder,cursor=Span(0,0)))
        result=_next_u16_interface(objects,name,rows,state,nested,headers['visitor']['protected'])['outcomes']
        self.assertEqual({v['error'] for v in result},{None,5})
        self.assertEqual(next(v['value_domain'] for v in result if v['error'] is None),(0,65535))


    def test_current_D_end_Q_pointer_count_and_initial_epoch_cannot_be_forged(self):
        for at,operation,operands in ((0x1001fedc,'strd','r8, r1, [sp, #236]'),
            (0x1001c664,'movs','r1, #0xc'),(0x1001c666,'strd','r1, r0, [sp, #288]'),
            (0x1001c66a,'add','r0, sp, #0x124')):
            objects,rows,context,item=self.fixture();row=rows[at];rows[at]=Instruction(at,row.raw,operation,operands)
            with self.subTest(at=hex(at)),self.assertRaises(AssertionError):self.consume(objects,rows,context)
        objects,rows,context,item=self.fixture();StartupConfigurationCopy.put(objects,item['config']+14600,b'\x01')
        with self.assertRaisesRegex(AssertionError,'initial Config active constructor'):
            self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))

    def test_changed_payload_and_CRC_do_not_choose_a_concrete_version(self):
        import zlib
        for payload in (b'\0\0',b'\xff\xff\x03\xff\xff\x03'):
            objects,rows,context,item=self.fixture();address=0x10032b1e;blob=bytearray(objects.read_only(address,1157))
            blob[12:12+len(payload)]=payload;crc=zlib.crc32(blob[:8]+blob[12:]);struct.pack_into('<I',blob,8,crc)
            StartupConfigurationCopy.put(objects,address,bytes(blob));StartupConfigurationCopy.put(objects,0x100200bc,struct.pack('<I',crc^0xffffffff))
            result=self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))
            success=next(v for v in result['outcomes'] if v['error'] is None)
            self.assertEqual(success['field_domains'],(('version',(0,65535)),('catalogue_version',(0,65535))))
            self.assertEqual(success['cursor'],(2,6));self.assertFalse(result['CRC_success_observed_taken'])


    def test_coherent_store_and_return_image_changes_reject_after_source_rebinding(self):
        for at,raw,op,operands in ((0x1001c67e,bytes.fromhex('6050'),'str','r0, [r4, r1]'),
            (0x1001bdb2,bytes.fromhex('10bd'),'pop','{r4, pc}')):
            objects,rows,context,item=self.fixture();old=objects.elf.data
            StartupConfigurationCopy.put(objects,at,raw);rows[at]=Instruction(at,raw,op,operands)
            self.assertNotEqual(objects.elf.data,old)
            bound=SameBLoaderBoundary.rebound(objects,context)
            with self.subTest(at=hex(at)),self.assertRaisesRegex(AssertionError,'payload (source/return/selector instruction differs|reference/cell/ABI/default/return operands differ)'):
                self.consume(objects,rows,bound)


    def test_short_current_payload_exhaustion_pairs_error_count_and_completed_prefix(self):
        import zlib
        objects,rows,context,item=self.fixture();address=0x10032b1e
        blob=bytearray(objects.read_only(address,13));struct.pack_into('<H',blob,6,1)
        crc=zlib.crc32(blob[:8]+blob[12:]);struct.pack_into('<I',blob,8,crc)
        StartupConfigurationCopy.put(objects,address,bytes(blob));StartupConfigurationCopy.put(objects,0x100200bc,struct.pack('<I',crc^0xffffffff))
        SameBLoaderBoundary.mov(objects,rows,0x1001fe76,'movw','r0',1)
        SameBLoaderBoundary.mov(objects,rows,0x1001fef8,'movw','r0',1)
        raw=bytes.fromhex('0cf20d00');StartupConfigurationCopy.put(objects,0x1001fed8,raw)
        rows[0x1001fed8]=Instruction(0x1001fed8,raw,'addw','r0, r12, #0xd')
        result=self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))
        exits=sorted(result['outcomes'],key=lambda v:v['completed_fields'])
        self.assertEqual([(v['completed_fields'],v['error'],v['count'],v['cursor'],len(v['calls'])) for v in exits],
            [(0,4,(12,12),(1,1),1),(1,4,(11,11),(1,1),2)])
        self.assertEqual([v['writes'] for v in exits],[(),((item['config']+14570,2),)])
        self.assertTrue(all(v['frontier']==0x1001c68a for v in exits))


    def test_unselected_fallthrough_cannot_bypass_second_field_success_guard(self):
        from config_build_source import validate_source_context
        site=0x1001c698;target=0x100289f0
        # Manually encoded Thumb32 NOP, BNE.W and BL, not new decoder output.
        # BNE T3: cond1/imm6=0xc/imm11=0x1aa -> same outline target.
        cases=((bytes.fromhex('aff30080'),'nop.w',''),
            (bytes.fromhex('4cf0aa81'),'bne.w',hex(target)),
            (StartupConfigurationCopy.branch_link(site,target),'bl',hex(target)))
        for raw,operation,operands in cases:
            objects,rows,context,item=self.fixture();old=rows[site]
            self.assertEqual((old.raw,old.mnemonic),(bytes.fromhex('0cf0aab9'),'b.w'))
            self.assertEqual(self.consume(objects,rows,context)['phase'],
                'first_boot_decoded_u16_prefix_pending_remaining_visitor_canonical_and_loader_routes')
            # The mutation replaces actual image bytes and their record. It
            # must be rejected after a valid current-source rebind, not for a
            # stale ELF hash, stale listing, or altered selected-store bytes.
            original=objects.elf.data;StartupConfigurationCopy.put(objects,site,raw)
            rows[site]=Instruction(site,raw,operation,operands)
            self.assertEqual(objects.read_only(site,4),raw);self.assertNotEqual(objects.elf.data,original)
            current=SameBLoaderBoundary.rebound(objects,context);validate_source_context(current,objects.elf)
            with self.subTest(operation=operation),self.assertRaisesRegex(AssertionError,
                '^unselected fallthrough enters decoded selected window$'):
                self.consume(objects,rows,current)


class ActualDecodedViews(unittest.TestCase):
    def fixture(self):
        objects,rows,context,item=ActualDecodedU16Prefix().fixture()
        extra=json.loads((Path(__file__).parent/'fixtures/config-decoded-views.json').read_text())
        for at,raw in extra['segments']:StartupConfigurationCopy.put(objects,at,bytes.fromhex(raw))
        for f in extra['functions']:
            for name,address,extent,index in f['aliases']:
                objects.elf.functions[name]=(address,extent,index)
            objects.elf.symbol_entries=list(dict.fromkeys(objects.elf.symbol_entries+[tuple(v) for v in f['symbols']]))
            if f['frame'] is not None:objects.sizes[f['address']]=f['frame']
            objects.elf.mapping_symbols.extend(tuple(v) for v in f['mapping'])
        for row in extra['records']:
            at=row['address'];rows[at]=Instruction(at,bytes.fromhex(row['bytes']),row['mnemonic'],row['operands'])
        return objects,rows,SameBLoaderBoundary.rebound(objects,context),item

    @staticmethod
    def consume(objects,rows,context):
        from config_decoded_views import first_boot_decoded_views
        return first_boot_decoded_views(objects,rows,context)

    def test_actual_group_keeps_prefix_errors_and_stops_Widget_ready(self):
        objects,rows,context,item=self.fixture();result=self.consume(objects,rows,context)
        self.assertEqual(result['phase'],'first_boot_decoded_views_pending_widgets_remaining_visitor_canonical_and_loader_routes')
        self.assertEqual([(v['completed_fields'],v['frontier']) for v in result['prefix_errors']],[(0,0x1001c68a),(1,0x1001c68a)])
        self.assertEqual(result['boot_view_domain'],(0,255));self.assertEqual(result['Widget_ready']['frontier'],0x1001c7f4)
        self.assertEqual(result['Widget_ready']['top_count'],8)
        self.assertEqual({v['initialized_views'] for v in result['successes']},set(range(9)))
        self.assertFalse(result['hardware_qualified']);self.assertNotIn('acquired_owner',result)
        self.assertTrue(all(v['frontier']==0x1001c68c for v in result['group_errors']))

    def test_parent_publication_stride_and_outline_flags_mutations_reject(self):
        for at,operation,operands in ((0x1001c7d8,'str.w','r9, [r4, #0xf8]'),
            (0x1001c7dc,'add.w','r8, r8, #0x14'),(0x1001c7d4,'strb.w','r0, [r8, #0x1]'),
            (0x10028778,'cmp','r0, #0x0')):
            objects,rows,context,item=self.fixture();old=rows[at];rows[at]=Instruction(at,old.raw,operation,operands)
            with self.subTest(at=hex(at)),self.assertRaises(AssertionError):self.consume(objects,rows,context)

    def test_public_summary_or_reinitialized_source_cannot_authorize_the_group(self):
        objects,rows,context,item=self.fixture()
        for fake in (dict(phase='before_boot_view',complete=True),None,context.to_record()):
            with self.assertRaises(AssertionError):self.consume(objects,rows,fake)
        StartupConfigurationCopy.put(objects,item['config']+14600,b'\x01')
        with self.assertRaises(AssertionError):self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))


    @staticmethod
    def move_function(objects,rows,role,new):
        elf=objects.elf;names=[n for n in elf.functions if config_stack.canonical_symbol(n)==role]
        if len(names)!=1:raise AssertionError('test function role')
        name=names[0];pointer,extent,index=elf.functions[name];old=pointer&~1
        raw=objects.read_only(old,extent);file_at=(len(elf.data)+3)&~3;newindex=len(elf.sections)
        elf.data+=bytes(file_at-len(elf.data))+raw;elf.sections.append((0,1,6,new,file_at,extent,0,0,4,0))
        elf.segments.append((1,file_at,new,new,extent,extent,5,4));elf.functions[name]=(new|1,extent,newindex)
        if old in objects.sizes:objects.sizes[new]=objects.sizes.pop(old)
        elf.symbol_entries=[(n,new|1,z,t,newindex) if n==name else (n,a,z,t,i) for n,a,z,t,i in elf.symbol_entries]
        elf.mapping_symbols.extend((n,a+new-old,newindex) for n,a,i in list(elf.mapping_symbols) if i==index and old<=a<old+extent)
        for at,row in list(rows.items()):
            moving=old<=at<old+extent;site=at+new-old if moving else at;args=row.operands;encoded=row.raw
            family=config_stack.transfer_family(row.mnemonic.removesuffix('.w'));changed=moving
            if family in ('b','bl','cbz','cbnz'):
                m=re.search(r'0x([0-9a-f]+)',args);target=int(m[1],16);internal=old<=target<old+extent
                if internal:target+=new-old;changed=True
                if changed:
                    if moving!=internal:
                        if family not in ('b','bl') or row.mnemonic not in ('b.w','bl'):raise AssertionError('test external branch encoding')
                        encoded=StartupConfigurationCopy.branch_link(site,target)
                        if family=='b':lo,hi=struct.unpack('<HH',encoded);encoded=struct.pack('<HH',lo,hi&~0x4000)
                    args=args[:m.start()]+hex(target)+args[m.end():]
            if changed:
                if moving:del rows[at]
                rows[site]=Instruction(site,encoded,row.mnemonic,args);StartupConfigurationCopy.put(objects,site,encoded)
        return old,new

    def test_independent_UTF8_code_table_and_visitor_parent_relocation(self):
        from config_utf8 import UTF8
        from config_headers import VISITOR
        objects,rows,context,item=self.fixture()
        self.move_function(objects,rows,UTF8,0x10071000)
        table=0x10074003;data=objects.read_only(0x10033e22,256);off=len(objects.elf.data)
        objects.elf.data+=data;objects.elf.sections.append((0,1,2,table,off,256,0,0,1,0))
        SameBLoaderBoundary.mov(objects,rows,0x10071000+26,'movw','r8',table&0xffff)
        SameBLoaderBoundary.mov(objects,rows,0x10071000+34,'movt','r8',table>>16)
        moved=self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))
        utf8=[v for v in moved['control']['nested_returns'] if v['function']==0x10071000]
        self.assertTrue(utf8);self.assertTrue(all(v['return_SP']==0x2007f798 for v in utf8))
        self.move_function(objects,rows,VISITOR,0x10076000)
        result=self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))
        self.assertEqual(result['Widget_ready']['frontier'],0x10076000+416)
        self.assertTrue(all(v['frontier']==0x10076000+56 for v in result['group_errors']))
        self.assertEqual({at-0x10076000 for at,target,why in result['control']['outside_shared_contexts']},
            {at-0x1001c654 for at,target,why in moved['control']['outside_shared_contexts']})

    def test_independent_B_and_stack_keep_exact_element_metadata_and_deepest_child(self):
        # Reuse actual initial/static relocation, then add this group's own
        # functions and bytes. No public boundary description supplies B.
        moved,more,bound,moveditem=ActualDecodedU16Prefix().fixture(0x4000)
        extra=json.loads((Path(__file__).parent/'fixtures/config-decoded-views.json').read_text())
        for at,raw in extra['segments']:StartupConfigurationCopy.put(moved,at,bytes.fromhex(raw))
        for f in extra['functions']:
            for name,address,extent,index in f['aliases']:moved.elf.functions[name]=(address,extent,index)
            moved.elf.symbol_entries=list(dict.fromkeys(moved.elf.symbol_entries+[tuple(v) for v in f['symbols']]))
            if f['frame'] is not None:moved.sizes[f['address']]=f['frame']
            moved.elf.mapping_symbols.extend(tuple(v) for v in f['mapping'])
        for r in extra['records']:
            more[r['address']]=Instruction(r['address'],bytes.fromhex(r['bytes']),r['mnemonic'],r['operands'])
        B=moveditem['config']+0x4000
        # Both visitor pool consumers must reload the actual independently B.
        StartupConfigurationCopy.put(moved,0x1001c654+3508,struct.pack('<I',B))
        sp=0x2007f800;StartupConfigurationCopy.put(moved,moved.elf.named['.vector_table'][3],struct.pack('<I',sp))
        moved.elf.symbol_entries=[(n,sp if n=='_stack_start' else a,z,t,i) for n,a,z,t,i in moved.elf.symbol_entries]
        result=self.consume(moved,more,SameBLoaderBoundary.rebound(moved,bound))
        self.assertEqual(result['deepest_child_SP'],sp-544-1608-72)
        eighth=[v for v in result['iterations'] if v['index']==7][0]
        self.assertEqual(eighth['Config_writes'],((B+416,1),(B+417,19),(B+436,1),(B+244,4)))
        self.assertFalse(any(v['Config_writes'] for v in result['iterations'] if v['index']==8))

    def test_all_error_epochs_counts_capacity_guards_and_publication_are_paired(self):
        objects,rows,context,item=self.fixture();result=self.consume(objects,rows,context)
        errors=result['group_errors'];self.assertEqual({v['error'] for v in errors},{4,5,8,14})
        self.assertTrue(all(v['top_Q_physical_count']==10 for v in errors))
        # The long payload cannot exhaust at boot_view. A separate shortened
        # actual source fixture below covers that paired error alternative.
        early=[v for v in errors if v['stage']=='boot_view'];self.assertFalse(early)
        self.assertTrue(all(v['initial_active_bytes_preserved']==180 and not v['boot_view_assigned'] for v in early))
        preclear=[v for v in errors if v['stage']=='Views pre-clear capacity']
        self.assertTrue(all(not v['views_cleared'] and v['guard']=='N>8 AND remaining>=N' for v in preclear))
        post=[v for v in errors if v['stage'] in ('View UTF8','View String14 capacity')]
        self.assertTrue(post);self.assertTrue(all(v['views_cleared'] and v['no_rollback'] and v['unfinished_element_not_published'] for v in post))
        self.assertTrue(all(v['initial_active_bytes_preserved']==175 for v in post))
        self.assertTrue(any(v['stage']=='View precommit capacity' and v['initialized_views']==8 for v in errors))
        for iteration in result['iterations']:
            self.assertEqual(iteration['stage_copies'],((1065,1000,19),(1000,940,19)))
            self.assertEqual(iteration['scratch_epochs'],((1064,1072,'varint Result8'),(1064,1076,'UTF8 Result12'),(1064,1084,'String14 Vec')))
            if iteration['index']<8:self.assertEqual(iteration['publication'],0x1001c654+388)
        self.assertFalse(result['control']['pending_contexts_confer_state'])
        self.assertTrue(result['shared_error_suffix_pending']);self.assertTrue(result['full_visitor_return_pending'])

    def test_coherent_foreign_shared_context_interior_and_fallthrough_reject(self):
        from config_build_source import validate_source_context
        V=0x1001c654
        # Existing later-field branch, manually encoded after source rebinding.
        for target in (V+200,V+142):
            objects,rows,context,item=self.fixture();site=V+424
            raw=BoundedParentTargets.conditional_branch(site,target,0)
            StartupConfigurationCopy.put(objects,site,raw);rows[site]=Instruction(site,raw,'beq.w',hex(target))
            current=SameBLoaderBoundary.rebound(objects,context);validate_source_context(current,objects.elf)
            with self.subTest(target=hex(target)),self.assertRaisesRegex(AssertionError,'foreign (Views|direct ingress)'):
                self.consume(objects,rows,current)
        objects,rows,context,item=self.fixture();site=V+4186;raw=bytes.fromhex('00bf')
        StartupConfigurationCopy.put(objects,site,raw);rows[site]=Instruction(site,raw,'nop','')
        with self.assertRaisesRegex(AssertionError,'unselected fallthrough enters Views'):
            self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))
        # Keep the allowed branch address/target but change its predecessor:
        # neither an address anchor nor the destination grants context/state.
        objects,rows,context,item=self.fixture();site=V+420;old=rows[site]
        raw=bytes.fromhex('aff30080');StartupConfigurationCopy.put(objects,site,raw);rows[site]=Instruction(site,raw,'nop.w','')
        with self.assertRaisesRegex(AssertionError,'foreign Views shared-error predecessor'):
            self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))

    def test_coherent_group_store_and_child_return_mutations_reject_for_ABI(self):
        for at,raw,operation,operands in ((0x1001c7d8,bytes.fromhex('c4f8f890'),'str.w','r9, [r4, #0xf8]'),
            (0x10016cdc,bytes.fromhex('f0bc'),'pop','{r4, r5, r6, r7}'),
            (0x10028ce6+10,bytes.fromhex('cdf82c04'),'str.w','r0, [sp, #0x42c]')):
            objects,rows,context,item=self.fixture();StartupConfigurationCopy.put(objects,at,raw);rows[at]=Instruction(at,raw,operation,operands)
            with self.subTest(site=hex(at)),self.assertRaisesRegex(AssertionError,'payload (source/return/selector instruction differs|reference/cell/ABI/default/return operands differ)'):
                self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))

    def test_short_actual_payload_keeps_boot_and_View_exhaustion_paired(self):
        import zlib
        for n in (3,4,6):
            objects,rows,context,item=self.fixture();address=0x10032b1e
            blob=bytearray(objects.read_only(address,12+n));struct.pack_into('<H',blob,6,n)
            crc=zlib.crc32(blob[:8]+blob[12:]);struct.pack_into('<I',blob,8,crc)
            StartupConfigurationCopy.put(objects,address,blob);StartupConfigurationCopy.put(objects,0x100200bc,struct.pack('<I',crc^0xffffffff))
            SameBLoaderBoundary.mov(objects,rows,0x1001fe76,'movw','r0',n)
            SameBLoaderBoundary.mov(objects,rows,0x1001fef8,'movw','r0',n)
            raw=struct.pack('<HH',0xf20c,12+n);StartupConfigurationCopy.put(objects,0x1001fed8,raw)
            rows[0x1001fed8]=Instruction(0x1001fed8,raw,'addw','r0, r12, #'+hex(12+n))
            result=self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))
            early=[v for v in result['group_errors'] if v['stage']=='boot_view']
            self.assertTrue(early);self.assertTrue(all(v['cursor']==(n,n) and not v['boot_view_assigned'] and
                v['initial_active_bytes_preserved']==180 for v in early))
            self.assertTrue(all(v['cursor'][1]<=n for v in result['group_errors']))
            if n==3:self.assertFalse(result['successes'])
            else:
                self.assertEqual({v['initialized_views'] for v in result['successes']},set(range(1+(n-4)//2)))
                self.assertTrue(any(v['stage']=='View id' and v['error']==4 for v in result['group_errors']))

    def test_deeper_memcpy_stack_overlap_rejects_and_lower_adjacency_accepts(self):
        low=0x20080000-544-1608-72
        for address,reject in ((low,True),(low+4,True),(low-4,False)):
            objects,rows,context,item=self.fixture();index=next(i for i,s in enumerate(objects.elf.sections) if s[2]&2)
            objects.elf.symbol_entries.append(('Views_child_overlap',address,4,17,index))
            if reject:
                with self.subTest(address=hex(address)),self.assertRaisesRegex(AssertionError,'physical'):
                    self.consume(objects,rows,context)
            else:self.assertEqual(self.consume(objects,rows,context)['deepest_child_SP'],low)

    def test_data_only_encoded_View_variants_preserve_abstract_union(self):
        import zlib
        # Encoded inputs are diagnostic witnesses; the consumer never chooses
        # one scalar execution or claims that a resealed CRC edge ran.
        variants=(b'\0\0\xff\0',b'\0\0\0\x01\x01\x01\xff',
            b'\0\0\0\x01\x01\x02\xc2',b'\0\0\0\x01\x01\x0f'+b'a'*15,
            b'\0\0\0\x09'+b'\0\0'*9)
        for prefix in variants:
            objects,rows,context,item=self.fixture();address=0x10032b1e;blob=bytearray(objects.read_only(address,1157))
            blob[12:12+len(prefix)]=prefix;crc=zlib.crc32(blob[:8]+blob[12:]);struct.pack_into('<I',blob,8,crc)
            StartupConfigurationCopy.put(objects,address,blob);StartupConfigurationCopy.put(objects,0x100200bc,struct.pack('<I',crc^0xffffffff))
            result=self.consume(objects,rows,SameBLoaderBoundary.rebound(objects,context))
            self.assertEqual(result['boot_view_domain'],(0,255));self.assertEqual({v['initialized_views'] for v in result['successes']},set(range(9)))
            self.assertTrue({8,14}<={v['error'] for v in result['group_errors']})


class SourceBoundUTF8(unittest.TestCase):
    def fixture(self,raw=b'hello',residue=0):
        from config_abstract import Span
        from config_regions import Storage,borrow
        from config_utf8 import UTF8Slice
        objects,rows,context,item=ActualDecodedViews().fixture()
        address=0x10071000+residue;file_at=len(objects.elf.data);data=raw or b'\0'
        objects.elf.data+=data;objects.elf.sections.append((0,1,2,address,file_at,len(data),0,0,1,0))
        source=Storage('address','test initialized bytes',address,len(data),1,'utf8_test',False)
        parent=Storage('stack:utf8','caller',0x2007e000,1608,8,'utf8_test',True)
        frame=Storage(parent.space,'utf8',parent.start-24,24,8,parent.phase,True)
        return objects,rows,UTF8Slice(borrow(source,0,len(data),source.phase),Span(0,0),Span(len(raw),len(raw))),borrow(parent,1064,12,parent.phase,True),frame

    @staticmethod
    def consume(objects,rows,source,output,frame):
        from config_utf8 import from_utf8_interface
        return from_utf8_interface(objects,rows,source,output,frame,())

    def test_empty_and_unknown_nonempty_results_keep_source_pair_and_inactive_None_distinct(self):
        for raw in (b'',b'a',b'a'*7,b'a'*8,b'a'*9,b'a'*15,b'a'*16,b'\0',b'\xc2\x80',b'\xed\xa0\x80',b'\xf4\x8f\xbf\xbf',b'\xff'):
            for residue in range(4):
                with self.subTest(raw=raw,residue=residue):
                    objects,rows,source,output,frame=self.fixture(raw,residue);result=self.consume(objects,rows,source,output,frame)
                    self.assertEqual(result['success']['source'],source)
                    self.assertEqual(bool(result['errors']),bool(raw))
                    self.assertEqual(result['return']['SP'],frame.start+24)
                    self.assertEqual(result['return']['saved_PC'],(frame.start+20,frame.start+24))
                    if raw:
                        none=next(v for v in result['errors'] if v['option_tag']==0)
                        self.assertIsNone(none['error_length']);self.assertNotIn(9,none['active_output_offsets'])
                    self.assertEqual(result['write_footprint'],((output.start+4,8),(output.start,4)))

    def test_width_table_and_actual_word_read_or_return_mutations_reject(self):
        for site,operation,operands in ((0x10016b7c,'subs.w','r12, r2, #0x6'),
            (0x10016bc4,'ldr','r5, [r1, #0x4]'),(0x10016cdc,'pop','{r4, r5, r6, r7, lr}')):
            objects,rows,source,output,frame=self.fixture();old=rows[site];rows[site]=Instruction(site,old.raw,operation,operands)
            with self.subTest(site=hex(site)),self.assertRaises(AssertionError):self.consume(objects,rows,source,output,frame)
        objects,rows,source,output,frame=self.fixture();StartupConfigurationCopy.put(objects,0x10033e22+0xc2,b'\0')
        with self.assertRaisesRegex(AssertionError,'UTF8 width table'):self.consume(objects,rows,source,output,frame)

    def test_misaligned_short_or_physically_aliased_output_rejects(self):
        from config_regions import borrow,Storage
        for kind in ('misaligned','short','frame_alias','cross_space'):
            objects,rows,source,output,frame=self.fixture()
            if kind=='misaligned':output=borrow(output.storage,1065,12,frame.phase,True)
            elif kind=='short':output=borrow(output.storage,1064,8,frame.phase,True)
            elif kind=='frame_alias':frame=Storage(frame.space,'bad child',output.start,24,8,frame.phase,True)
            else:frame=Storage('stack:other','hidden same address',output.start,24,8,frame.phase,True)
            with self.subTest(kind=kind),self.assertRaises(AssertionError):self.consume(objects,rows,source,output,frame)


class BoundedParentTargets(unittest.TestCase):
    @staticmethod
    def conditional_branch(site,target,condition=3):
        disp=target-site-4
        if disp%2 or not -(1<<20)<=disp<(1<<20):raise ValueError('Thumb conditional target')
        bits=disp&((1<<21)-1)
        return struct.pack('<HH',0xf000|((bits>>20)&1)<<10|condition<<6|((bits>>12)&63),
            0x8000|((bits>>19)&1)<<13|((bits>>18)&1)<<11|((bits>>1)&2047))

    @staticmethod
    def match(objects,rows,offset=4188):
        from config_finite import _literal_body
        from config_headers import VISITOR
        name=next(n for n in objects.elf.functions if config_stack.canonical_symbol(n)==VISITOR)
        parent=objects.elf.functions[name][0]&~1
        def no_child(*args):raise AssertionError('unexpected parent-target child')
        contract=dict(extent=4,instructions=((0,4,'blo.w @parent+'+format(offset,'x')),),objects=[])
        return _literal_body(objects,name,rows,VISITOR,'bounded parent branch test',contract,no_child,no_child,window=parent+264)

    def test_same_parent_cross_window_target_binds_current_instruction(self):
        objects,rows,context,item=ActualDecodedViews().fixture()
        self.assertEqual(self.match(objects,rows)['function'],0x1001c75c)
        self.assertEqual(self.conditional_branch(0x1001c75c,0x1001d6b0),bytes.fromhex('c0f0a887'))

    def test_wrong_parent_offset_extent_data_or_instruction_interior_rejects(self):
        for target,expected in ((0x10016b74,4188),(0x1001c654+4194,4188),
            (0x1001c654+7128,7128),(0x1001c654+3508,3508),(0x1001c654+4192,4192)):
            objects,rows,context,item=ActualDecodedViews().fixture();site=0x1001c75c
            raw=self.conditional_branch(site,target);StartupConfigurationCopy.put(objects,site,raw)
            rows[site]=Instruction(site,raw,'blo.w',hex(target))
            with self.subTest(target=hex(target)),self.assertRaisesRegex(AssertionError,'parent-relative'):
                self.match(objects,rows,expected)


if __name__=='__main__':unittest.main()
