"""Finite alternate configuration roles; machine closure is required separately."""
import re
import struct
from validation import require

CONDITIONS = r'(?:eq|ne|hs|lo|hi|ls|ge|lt|gt|le|cs|cc|mi|pl|vs|vc)?'


def transfer_family(mnemonic):
    """Finite selected-LLVM transfer spellings, including predicated forms."""
    for family in ('blxns', 'bxns', 'blx', 'bx', 'bl', 'b', 'cbnz', 'cbz', 'tbb', 'tbh',
                   'svc', 'hvc', 'smc', 'bkpt', 'udf', 'trap', 'eret'):
        if re.fullmatch(family + CONDITIONS, mnemonic):
            return family
    return None


def canonical_symbol(symbol):
    # Preserve the complete v0 receiver/trait/generic backreference structure.
    # Only crate disambiguators and LLVM's numeric clone suffix are unstable.
    return re.sub(r'Cs[A-Za-z0-9]+_', 'Cs_', re.sub(r'\.\d+$','',symbol))


ERROR_ROOTS = {
    '_RNvXsk_NtCs_16dashboard_config8validateNtB5_15ValidationErrorNtNtCs_4core3fmt5Debug3fmt': 'validation_error',
    '_RNvXs2_NtCs_16dashboard_config5codecNtB5_11ConfigErrorNtNtCs_4core3fmt5Debug3fmt': 'config_error',
    '_RNvXs0_Cs_8heaplessNtB5_13CapacityErrorNtNtCs_4core3fmt5Debug3fmt': 'capacity_error',
    '_RNvXs9_NtNtCs_4core3str5errorNtB5_9Utf8ErrorNtNtB9_3fmt5Debug3fmt': 'utf8_error',
}


def error_root_role(symbol):
    role=ERROR_ROOTS.get(canonical_symbol(symbol))
    require(role is not None,'unreviewed configuration diagnostic callback root: '+symbol)
    return role


ERROR_LAYOUTS = {'capacity_error': (0, 1), 'validation_error': (12, 2), 'utf8_error': (8, 4)}
# Exact boot payload interfaces from the source-reviewed per-mode contracts.
# A role is selected by its originating field, never a union of nearby vtables.
PAYLOAD_ROLES = {
    '_RNvXs6_NtCs_16dashboard_config8validateNtB5_9ErrorCodeNtNtCs_4core3fmt5Debug3fmt': 'error_code',
    '_RNvXsd_NtCs_16dashboard_config8validateNtB5_5FieldNtNtCs_4core3fmt5Debug3fmt': 'field',
    '_RNvXsR_NtCs_4core6optionINtB5_6OptionhENtNtB7_3fmt5Debug3fmtCs_16dashboard_config': 'option_u8',
    '_RNvXsR_NtCs_4core6optionINtB5_6OptiontENtNtB7_3fmt5Debug3fmtCs_16dashboard_config': 'option_u16',
    '_RNvXs1g_NtCs_4core3fmtRINtNtB8_6option6OptionNtNtCs_16dashboard_config5model7ChannelENtB6_5Debug3fmtBW_': 'option_channel_ref',
    '_RNvXs1g_NtCs_4core3fmtRNtNtCs_16dashboard_config8controls12ControlErrorNtB6_5Debug3fmtBA_': 'control_error_ref',
    '_RNvXs1g_NtCs_4core3fmtRNtNtCs_16dashboard_config10appearance9ColorRoleNtB6_5Debug3fmtBA_': 'color_role_ref',
    '_RNvXs1g_NtCs_4core3fmtRNtNtCs_16dashboard_config5model7ChannelNtB6_5Debug3fmtBA_': 'channel_ref',
    '_RNvXs1g_NtCs_4core3fmtRINtNtB8_6option6OptionhENtB6_5Debug3fmtCs_16dashboard_config': 'option_u8_ref',
    '_RNvXsZ_NtNtCs_4core3fmt3numjNtB7_5Debug3fmt': 'usize',
    '_RNvXs6_NtCs_16dashboard_config8controlsNtB5_11ControlPageNtNtCs_4core3fmt5Debug3fmt': 'control_page',
    '_RNvXse_NtCs_16dashboard_config8controlsNtB5_8ButtonIdNtNtCs_4core3fmt5Debug3fmt': 'button_id',
    '_RNvXs1g_NtCs_4core3fmtRNtNtCs_16dashboard_config8controls11ControlPageNtB6_5Debug3fmtBA_': 'control_page_ref',
    '_RNvXs1g_NtCs_4core3fmtRbNtB6_5Debug3fmtCs_16dashboard_config': 'bool_ref',
    '_RNvXs1g_NtCs_4core3fmtRbNtB6_5Debug3fmtCs_6rp_pac': 'bool_ref',
    '_RNvXs1g_NtCs_4core3fmtRhNtB6_5Debug3fmtCs_14dashboard_core': 'u8_ref',
    '_RNvXs1g_NtCs_4core3fmtRhNtB6_5Debug3fmtCs_16dashboard_config': 'u8_ref',
    '_RNvXs1g_NtCs_4core3fmtRhNtB6_5Debug3fmtCs_20embassy_hal_internal': 'u8_ref',
    '_RNvXs1g_NtCs_4core3fmtRtNtB6_5Debug3fmtCs_16dashboard_config': 'u16_ref',
    '_RNvXs1g_NtCs_4core3fmtRtNtB6_5Debug3fmtCs_6rp_pac': 'u16_ref',
}
PAYLOAD_LAYOUTS = {
    'error_code': (1, 1), 'field': (3, 1), 'option_u8': (2, 1), 'option_u16': (4, 2),
    'control_page': (1, 1), 'button_id': (1, 1),
    **{role: (4, 4) for role in ('option_channel_ref', 'control_error_ref', 'color_role_ref',
                               'channel_ref', 'option_u8_ref', 'usize', 'control_page_ref',
                               'bool_ref', 'u8_ref', 'u16_ref')},
}
FIXED_LITERALS = {'view_count': b'built-in view count', 'widget_count': b'built-in widget count'}
# Pinned core::fmt Arguments templates: C0 is one default placeholder; a
# following byte is the next literal length, not an argument index.
DEFAULT_TEMPLATES = {
    'unwrap_failed': (b'\xc0\x02: \xc0\x00', 2),
    'expect_failed': (b'\xc0\x00', 1),
    'slice_start': (b'\x12range start index \xc0\x22 out of range for slice of length \xc0\x00', 2),
    'slice_order': (b'\x16slice index starts at \xc0\x0d but ends at \xc0\x00', 2),
    'slice_end': (b'\x10range end index \xc0\x22 out of range for slice of length \xc0\x00', 2),
    'bounds': (b'\x20index out of bounds: the len is \xc0\x12 but the index is \xc0\x00', 2),
}


class DiagnosticObjects:
    """Typed data checks, not proof of argument producers or callback closure.

    The completed machine contract must bind these objects to the consuming
    instructions. Merely finding a valid object nearby establishes no edge.
    """
    def __init__(self, elf, sizes):
        self.elf, self.sizes = elf, sizes

    def read_only(self, address, length):
        require(isinstance(address, int) and isinstance(length, int) and length > 0,
                'invalid diagnostic object extent')
        matches = [s for s in self.elf.sections if s[1] == 1 and s[2] & 2 and not s[2] & 1
                   and s[3] <= address < address + length <= s[3] + s[5]]
        require(len(matches) == 1, 'diagnostic object is not uniquely backed read-only data')
        section = matches[0]; offset = section[4] + address - section[3]
        data = self.elf.data[offset:offset + length]
        require(len(data) == length, 'truncated diagnostic object')
        return data

    def debug_vtable(self, address, role):
        layouts = ERROR_LAYOUTS | PAYLOAD_LAYOUTS
        require(role in layouts and address % 4 == 0, 'unreviewed or unaligned diagnostic vtable')
        drop, size, align, pointer = struct.unpack('<4I', self.read_only(address, 16))
        require((drop, size, align) == (0, *layouts[role]), 'diagnostic vtable type layout differs')
        require(pointer & 1, 'diagnostic callback is not Thumb')
        target = pointer & ~1
        aliases = [(name, entry) for name, entry in self.elf.functions.items() if entry[0] & ~1 == target]
        require(aliases, 'diagnostic callback is not a function entry')
        require(len({entry for _, entry in aliases}) == 1, 'diagnostic callback aliases disagree')
        for name, (value, extent, index) in aliases:
            actual_role = (ERROR_ROOTS | PAYLOAD_ROLES).get(canonical_symbol(name))
            require(value & 1 and extent > 0 and actual_role == role,
                    'diagnostic callback role differs from originating object')
            require(0 <= index < len(self.elf.sections), 'diagnostic callback section is missing')
            section = self.elf.sections[index]
            require(section[1] == 1 and section[2] & 6 == 6 and not section[2] & 1,
                    'diagnostic callback is not allocated read-only executable code')
            require(section[3] <= target < target + extent <= section[3] + section[5],
                    'diagnostic callback extent is outside its declared section')
            self.read_only(target, extent)
        require(target in self.sizes, 'missing diagnostic callback frame metadata')
        return target

    def default_template(self, address, role):
        require(role in DEFAULT_TEMPLATES, 'unreviewed diagnostic template role')
        expected, count = DEFAULT_TEMPLATES[role]
        require(self.read_only(address, len(expected)) == expected,
                'diagnostic template options, literals or arity differ')
        return tuple(range(count))

    def tagged_literal(self, address, word, role):
        require(role in FIXED_LITERALS and isinstance(word, int) and 0 < word <= 0xffffffff and word & 1,
                'unreviewed or non-tagged fixed diagnostic Arguments')
        expected = FIXED_LITERALS[role]
        require(word >> 1 == len(expected), 'tagged diagnostic literal length differs')
        actual = self.read_only(address, len(expected))
        require(actual == expected, 'fixed diagnostic literal differs')
        return actual


def constant_before(instructions, consumer, register, objects):
    """Match one bounded straight-line constant producer at a call boundary.

    Input must be the byte-checked caller extent from the resolved adapter.
    This component does not select that extent or admit a callback/control edge.
    Branches, calls, unknown definitions and runtime memory producers fail;
    it is not a register executor or a reaching-definition merge algorithm.
    """
    require(re.fullmatch(r'r(?:[0-9]|1[0-2])', register) is not None, 'unsupported constant argument register')
    records = sorted(instructions, key=lambda i: i.address)
    require(len({i.address for i in records}) == len(records), 'ambiguous boundary instruction record')
    sites = [i for i, record in enumerate(records) if record.address == consumer]
    require(len(sites) == 1 and records[sites[0]].mnemonic in ('bl', 'bl.w'), 'missing direct consuming call')
    boundary = sites[0]
    start = max(0, boundary - 64)
    transfers = {'b', 'bl', 'bx', 'blx', 'beq', 'bne', 'bhs', 'blo', 'bhi', 'bls',
                 'bge', 'blt', 'bgt', 'ble', 'bcs', 'bcc', 'bmi', 'bpl', 'bvs', 'bvc',
                 'cbz', 'cbnz', 'tbb', 'tbh'}
    plain = {'mov', 'movs', 'movw', 'movt', 'ldr', 'ldrb', 'ldrh', 'ldrd',
             'str', 'strb', 'strh', 'strd', 'stm', 'stmia', 'ldm', 'ldmia',
             'add', 'adds', 'sub', 'subs', 'cmp', 'cmn', 'tst', 'and', 'ands',
             'orr', 'orrs', 'eor', 'eors', 'lsl', 'lsls', 'lsr', 'lsrs', 'asr',
             'asrs', 'mvn', 'mvns', 'uxtb', 'uxth', 'sxtb', 'sxth', 'nop'}

    def producer(before, wanted, nesting):
        require(nesting <= 8, 'constant argument dependency bound exceeded')
        next_address = records[before].address
        for index in range(before - 1, start - 1, -1):
            record = records[index]
            require(record.address % 2 == 0 and len(record.raw) in (2, 4)
                    and record.address + len(record.raw) == next_address,
                    'noncontiguous constant producer block')
            next_address = record.address
            mnemonic = record.mnemonic.removesuffix('.w')
            operands = record.operands.split('@', 1)[0].strip()
            require(mnemonic not in transfers and not mnemonic.startswith('it'),
                    'constant producer crosses unproved control boundary')
            require(mnemonic in plain, 'unreviewed instruction in constant producer block')
            destination, _, source = operands.partition(',')
            destination = destination.strip(); source = source.strip()
            require(destination != 'pc' and not (mnemonic in ('ldm', 'ldmia') and re.search(r'\bpc\b', source)),
                    'constant producer crosses implicit control boundary')
            require(not (re.search(r'\[' + wanted + r'(?:\s*,|\s*\])', operands)
                         and re.search(r'\]\s*,', operands)),
                    'constant argument base has post-index writeback')
            if destination == wanted:
                if mnemonic in ('mov', 'movs', 'movw', 'movt'):
                    if re.fullmatch(r'#(?:0x[0-9a-f]+|[0-9]+)', source):
                        value = int(source[1:], 0)
                        require(value <= (0xffff if mnemonic in ('movw', 'movt') else 0xffffffff),
                                'constant argument immediate differs')
                        if mnemonic == 'movt':
                            low, evidence = producer(index, wanted, nesting + 1)
                            return value << 16 | low & 0xffff, (record.address, *evidence)
                        return value, (record.address,)
                    require(mnemonic == 'mov' and re.fullmatch(r'r(?:[0-9]|1[0-2])', source),
                            'unreviewed constant move producer')
                    value, evidence = producer(index, source, nesting + 1)
                    return value, (record.address, *evidence)
                if mnemonic == 'ldr':
                    literal = re.fullmatch(r'\[pc(?:, #(-?0x[0-9a-f]+|-?[0-9]+))?\]', source)
                    require(literal is not None, 'constant argument has runtime memory producer')
                    offset = int(literal[1], 0) if literal[1] else 0
                    address = ((record.address + 4) & ~3) + offset
                    return struct.unpack('<I', objects.read_only(address, 4))[0], (record.address,)
                require(mnemonic in ('str', 'strb', 'strh', 'strd', 'cmp', 'tst'),
                        'unreviewed constant argument definition')
            if re.search(r'\b' + wanted + r'\b', operands):
                require(mnemonic in ('str', 'strb', 'strh', 'strd', 'cmp', 'tst') and '!' not in operands,
                        'unreviewed constant argument clobber')
        raise AssertionError('missing current constant argument producer')

    value, evidence = producer(boundary, register, 0)
    first = min(evidence)
    # Check the entire supplied caller extent. Stopping at the nearest producer
    # cannot establish that another predecessor did not jump past it.
    # The closed caller ABI permits BX LR, POP {..., PC}, and LDMIA SP!,
    # {..., PC} as returns. They terminate a path; they are not arbitrary ingress
    # targets. Full callback closure must separately prove ordinary call edges.
    for record in records:
        mnemonic = record.mnemonic.removesuffix('.w')
        transfer = transfer_family(mnemonic)
        operands = record.operands.split('@', 1)[0].strip()
        destination, _, source = operands.partition(',')
        destination = destination.strip()
        pop = re.fullmatch('pop' + CONDITIONS, mnemonic) is not None
        ldm = re.fullmatch(r'ldm(?:ia|ib|da|db)?' + CONDITIONS, mnemonic) is not None
        list_pc = (pop or ldm) and re.search(r'\bpc\b', operands) is not None
        return_list = r'\{(?:r(?:[0-9]|1[0-2])\s*,\s*)*pc\}'
        return_form = (pop and re.fullmatch(return_list, operands)
                       or mnemonic == 'ldmia' and destination == 'sp!'
                       and re.fullmatch(return_list, source.strip()))
        require(not (destination == 'pc' or list_pc) or return_form,
                'constant producer has unresolved implicit PC ingress')
        require(not mnemonic.startswith('rfe') and transfer not in
                ('svc', 'hvc', 'smc', 'bkpt', 'udf', 'trap'),
                'constant producer has unresolved exception ingress')
        if mnemonic == 'tbb':
            # Architectural exclusion only: an unsigned byte gives PC+4+2*x,
            # x in 0..255. This proves no ingress into this protected interval,
            # NOT table/index provenance, valid enum objects or ordinary closure.
            index = re.fullmatch(r'\[pc, r([0-9]|1[0-2])\]', operands)
            require(index is not None and len(record.raw) == 4 and
                    record.raw == struct.pack('<HH', 0xe8df, 0xf000 | int(index[1])),
                    'unreviewed PC-relative TBB ingress encoding')
            require(0 <= record.address <= 0xffffffff - 514 and record.address % 2 == 0,
                    'PC-relative TBB destination extent wraps')
            lo, hi = record.address + 4, record.address + 514
            require(hi <= first or lo > consumer,
                    'PC-relative TBB destination interval overlaps argument producer')
            continue
        require(transfer not in ('tbb', 'tbh', 'blx', 'bxns', 'blxns', 'eret') and
                not (transfer == 'bx' and (mnemonic != 'bx' or operands != 'lr')),
                'constant producer has unresolved caller ingress')
        if transfer in ('b', 'bl', 'cbz', 'cbnz'):
            target_text = operands.split(',')[-1].strip() if transfer in ('cbz', 'cbnz') else operands
            target = re.match(r'(0x[0-9a-f]+)(?:\s|$)', target_text)
            require(target is not None, 'constant producer has unresolved direct ingress')
            target = int(target[1], 16)
            require(not (first < target <= consumer and not first <= record.address < consumer),
                    'alternate ingress bypasses current argument producer')
    return value, evidence
