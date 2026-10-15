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


if __name__=='__main__':unittest.main()
