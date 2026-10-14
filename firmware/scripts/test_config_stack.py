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
