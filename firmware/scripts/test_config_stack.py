"""Finite configuration stack roles and diagnostic root regressions."""
import unittest
from types import SimpleNamespace
import struct
import config_stack
from llvm_stack import Instruction
from config_stack import error_root_role

VALIDATION = '_RNvXsk_NtCshxbT6mSCLYQ_16dashboard_config8validateNtB5_15ValidationErrorNtNtCs6m2Zc6X1WBq_4core3fmt5Debug3fmt'
WIDGET = '_RNvXsc_NtCshxbT6mSCLYQ_16dashboard_config5modelNtB5_6WidgetNtNtCs6m2Zc6X1WBq_4core3fmt5Debug3fmt'
CONTROL = '_RNvXs1g_NtCs123_4core3fmtRNtNtCs456_16dashboard_config8controls12ControlErrorNtB6_5Debug3fmtBA_'
U8 = '_RNvXs1g_NtCs123_4core3fmtRhNtB6_5Debug3fmtCs456_14dashboard_core'


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


if __name__=='__main__':unittest.main()
