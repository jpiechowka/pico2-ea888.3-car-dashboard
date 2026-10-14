"""Finite alternate configuration roles; machine closure is required separately."""
import re
import struct
from validation import require
from llvm_stack import mapping_ranges

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


# Source-created enum layouts only. The caller must bind Field to
# ValidationError.field, and ControlError to the Field.Controls reference cell.
# This table/prefix witness does not prove any case's payload or suffix binding.
ENUM_PREFIXES = {
    'field': (
        bytes.fromhex('80b5 6f46 82b0 0278 083a 98bf 0022 dfe812f0'),
        bytes.fromhex('e0b5 02af 0278 083a 98bf 0022 dfe812f0'),
    ),
    'control_error_ref': (
        bytes.fromhex('f0b5 03af 4df804bd 8ab0 0268 1078 0238 38bf 0220 c0b2 dfe800f0'),
        bytes.fromhex('f0b5 03af 4df804bd 88b0 0568 0c46 2878 0238 38bf 0220 c0b2 dfe800f0'),
        bytes.fromhex('f0b5 03af 2de9000f 89b0 0568 0e46 2878 0238 38bf 0220 c0b2 dfe800f0'),
    ),
}


def enum_dispatch(objects, name, instructions):
    """Bind one finite typed enum prefix and current code/data entry table.

    It does not accept a raw table hash, prove arbitrary invalid enum values,
    or discharge subsequent typed cases/ordinary callback closure.
    """
    elf = objects.elf
    require(name in elf.functions, 'missing typed enum callback')
    role = PAYLOAD_ROLES.get(canonical_symbol(name))
    require(role in ENUM_PREFIXES, 'unreviewed typed enum dispatch role')
    pointer, extent, index = elf.functions[name]
    start = pointer & ~1
    require(pointer & 1 and extent > 0 and 0 <= index < len(elf.sections),
            'invalid typed enum callback extent')
    aliases = [(n, e) for n, e in elf.functions.items() if e[0] & ~1 == start]
    require(all(e == (pointer, extent, index) and PAYLOAD_ROLES.get(canonical_symbol(n)) == role
                for n, e in aliases), 'typed enum callback aliases differ')
    section = elf.sections[index]
    require(section[1] == 1 and section[2] & 6 == 6 and not section[2] & 1 and
            section[3] <= start < start + extent <= section[3] + section[5],
            'typed enum body is outside its declared read-only executable section')
    require(start in objects.sizes, 'missing typed enum frame metadata')
    raw = objects.read_only(start, extent)
    matches = [p for p in ENUM_PREFIXES[role] if raw.startswith(p)]
    require(len(matches) == 1, 'unreviewed typed enum tag/index transform')
    prefix = matches[0]
    records = {i.address: i for i in instructions}
    require(len(records) == len(instructions), 'duplicate typed enum instruction')
    ranges = mapping_ranges(elf, name)
    code = {a for lo, hi, kind in ranges if kind == 't' for a in range(lo, hi)}
    covered = set()
    for address, record in records.items():
        require(len(record.raw) in (2, 4) and address % 2 == 0 and
                start <= address < address + len(record.raw) <= start + extent,
                'invalid typed enum instruction extent')
        points = set(range(address, address + len(record.raw)))
        require(points <= code and not points & covered and
                record.raw == raw[address - start:address - start + len(record.raw)],
                'typed enum instruction is not current mapped bytes')
        covered |= points
    require(covered == code, 'incomplete typed enum code coverage')
    table = start + len(prefix)
    count, width = (58, 2) if role == 'field' else (7, 1)
    require(table + count * width <= start + extent <= 0x100000000,
            'typed enum table extent overflow')
    require(any(kind == 'd' and lo <= table < table + count * width <= hi
                for lo, hi, kind in ranges), 'typed enum table is not mapped read-only data')
    offsets = objects.read_only(table, count * width)
    offsets = struct.unpack('<58H', offsets) if width == 2 else tuple(offsets)
    entries = tuple(table + 2 * offset for offset in offsets)
    require(len(set(entries)) == count and all(table + count * width <= target < start + extent
                                              and target in records for target in entries),
            'typed enum table target is not a distinct current case entry')
    # Alignment after the seven TBB entries is not an eighth source variant.
    if width == 1:
        require(objects.read_only(table + count, 1) == b'\0', 'typed enum table alignment differs')
    return {'role': role, 'function': start, 'table': table, 'entries': entries,
            'prefix_bytes': len(prefix), 'epilogue':
            ('classic' if prefix == ENUM_PREFIXES['field'][0] else 'scratch') if role == 'field' else None}


FIELD_PAYLOADS = {'Controls': ('control_error_ref', 0), 'HistorySeconds': ('channel_ref', 1),
                  'Color': ('color_role_ref', 1),
                  **{n: ('u8_ref', 1) for n in ('BandLabel', 'BandMinimum', 'BandMaximum', 'BandOrder', 'BandColor')},
                  **{n: ('bool_ref', 1) for n in ('LedBrightness', 'LedPeriod', 'LedOnPercent')}}
TUPLE1_ROLE = '_RNvMsa_NtCs_4core3fmtNtB5_9Formatter25debug_tuple_field1_finish'
FIELD_CASES = tuple('Controls Version CatalogueVersion Views ViewId ViewName BootView WidgetId '
                   'WidgetView Placement Label Decimals Afr Readout Secondary Smoothing PeakHold '
                   'Reference BandCount BandLabel BandMinimum BandMaximum BandOrder BandColor '
                   'HistorySeconds WarningTriggerHigh WarningClearHigh WarningSnooze WarningAcknowledge '
                   'LedBrightness LedPeriod LedOnPercent GraphRange GaugeRange Color WarningColors '
                   'WarningMode WarningTrigger WarningClear WarningLoad WarningOn WarningOff '
                   'WarningMessage WarningPage SourceDefinition SourceAccess SourceDecoder '
                   'SourceEvidence SourceRoute SourcePoll SourceFreshness FuelRatio FuelPercent '
                   'FuelGasolineRatio FuelEthanolRatio FuelGasolineDensity FuelEthanolDensity '
                   'FuelReference'.split())


def _field_outline(objects, target, outlines):
    """Complete zero-growth Field adapters; ordinals are not acceptance keys."""
    aliases = [(n, e) for n, e in objects.elf.functions.items() if e[0] & ~1 == target]
    require(aliases and all(re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+', n) for n, _ in aliases),
            'unreviewed Field outline origin')
    pointer, extent, index = aliases[0][1]
    require(pointer & 1 and all(e == (pointer, extent, index) for _, e in aliases) and
            objects.sizes.get(target) == 0 and 0 <= index < len(objects.elf.sections),
            'Field outline aliases/frame/Thumb differ')
    section = objects.elf.sections[index]
    require(section[1] == 1 and section[2] & 6 == 6 and not section[2] & 1 and
            section[3] <= target < target + extent <= section[3] + section[5],
            'Field outline is outside its declared RO executable section')
    require(outlines is not None and target in outlines, 'missing current Field outline body')
    records = sorted(outlines[target], key=lambda i: i.address)
    raw = objects.read_only(target, extent)
    require(mapping_ranges(objects.elf, aliases[0][0]) == [(target, target + extent, 't')],
            'Field outline contains unreviewed mapping')
    address = target
    for record in records:
        require(record.address == address and len(record.raw) in (2, 4) and
                record.raw == raw[address - target:address - target + len(record.raw)],
                'Field outline lacks complete current-byte coverage')
        address += len(record.raw)
    require(address == target + extent, 'incomplete Field outline extent')
    rows = [(len(i.raw), i.mnemonic.removesuffix('.w'), i.operands) for i in records]
    sink = [(4, 'ldrd', 'r0, r1, [r1]'), (2, 'ldr', 'r3, [r1, #0xc]'), (2, 'bx', 'lr')]
    transport = [(2, 'str', 'r0, [sp]'), (2, 'mov', 'r0, r1'),
                 (2, 'mov', 'r1, r2'), (2, 'bx', 'lr')]
    if rows == transport: return 'vtable_transport', None
    if rows == sink: return 'sink_extract', None
    for length in (6, 15):
        if rows == [sink[0], (2, 'movs', 'r2, #0x' + format(length, 'x')), *sink[1:]]:
            return 'sink_extract', length
    raise AssertionError('unreviewed Field zero-growth outline body')


def _field_case(objects, records, entry, case, outlines=None, epilogue='classic'):
    """Closed Field operand-materialization vocabulary; no general executor.

    Input is a typed, byte-checked case entry from enum_dispatch. Registers carry
    only Field/Formatter/sink identities, readonly constants or fixed stack cells.
    Each predecessor is walked separately and cannot borrow another's producer.
    This does not prove the subsequently selected tuple/sink bridge body.
    """
    values = {'r0': ('field', 0), 'r1': ('formatter', 0)}
    require(epilogue in ('classic', 'scratch'), 'unreviewed Field prologue/epilogue family')
    cells = {}; sp = 0; visited = set(); binding = None; address = entry; outline_calls = []; history = []

    def get(register):
        if register == 'sp': return ('stack', sp)
        require(register in values, 'missing typed Field operand producer')
        return values[register]

    def number(text):
        require(re.fullmatch(r'#(?:0x[0-9a-f]+|[0-9]+)', text), 'unreviewed Field immediate')
        return int(text[1:], 0)

    def plus(term, amount):
        require(isinstance(term, tuple) and term[0] in ('field', 'stack'), 'untyped Field address arithmetic')
        return term[0], term[1] + amount

    def memory(text):
        match = re.fullmatch(r'\[(r(?:[0-9]|1[0-2])|sp|pc)(?:, (#(?:0x[0-9a-f]+|[0-9]+)))?\]', text)
        require(match is not None, 'unreviewed Field memory operand/writeback')
        return match[1], number(match[2]) if match[2] else 0

    for _ in range(48):
        require(address in records and address not in visited, 'unknown or cyclic Field suffix')
        record = records[address]; visited.add(address)
        mnemonic = record.mnemonic.removesuffix('.w')
        operands = record.operands.split('@', 1)[0].strip()
        history.append((mnemonic, operands))
        destination, _, source = operands.partition(','); source = source.strip()
        next_address = address + len(record.raw)
        if mnemonic in ('movw', 'movt', 'mov', 'movs', 'ldr', 'ldrd'):
            require(re.fullmatch(r'r[0-3]|r12|lr', destination),
                    'Field materialization writes implicit control/stack state')
        if mnemonic in ('add', 'adds'):
            require(re.fullmatch(r'r[0-3]|r12|lr|sp', destination),
                    'Field address arithmetic writes implicit control state')
        if mnemonic in ('movw', 'movt', 'mov', 'movs'):
            if source.startswith('#'):
                immediate = number(source)
                require(immediate <= (0xffff if mnemonic in ('movw', 'movt') else 0xffffffff),
                        'Field address immediate overflow')
                if mnemonic == 'movt':
                    previous = get(destination)
                    require(isinstance(previous, int), 'missing Field low-address producer')
                    values[destination] = previous & 0xffff | immediate << 16
                else: values[destination] = immediate
            else:
                require(mnemonic == 'mov', 'unreviewed Field register move')
                values[destination] = get(source)
        elif mnemonic in ('add', 'adds'):
            parts = source.split(', ')
            term = get(destination) if len(parts) == 1 else get(parts[0])
            result = plus(term, number(parts[-1]))
            if destination == 'sp':
                require(result[0] == 'stack', 'Field stack restoration lacks known SP origin')
                sp = result[1]
            else: values[destination] = result
        elif mnemonic == 'str':
            base, offset = memory(source)
            require(base == 'sp' and sp == 0 and offset in (0, 4) and case in FIELD_PAYLOADS,
                    'Field store leaves its owned argument cells')
            cells[sp + offset] = get(destination)
        elif mnemonic == 'ldr':
            base, offset = memory(source)
            if base == 'pc':
                values[destination] = struct.unpack('<I', objects.read_only(((address + 4) & ~3) + offset, 4))[0]
            else:
                require(get(base) == ('sink_vtable', 0) and offset == 12,
                        'Field sink callback lacks live Formatter origin')
                values[destination] = ('write_str', 0)
        elif mnemonic == 'ldrd':
            second, _, operand = source.partition(', ')
            require(re.fullmatch(r'r[0-3]|r12|lr', second),
                    'Field paired load writes implicit control/stack state')
            base, offset = memory(operand)
            require(get(base) == ('formatter', 0) and offset == 0,
                    'Field sink extraction lacks Formatter origin')
            values[destination] = ('sink_data', 0); values[second] = ('sink_vtable', 0)
        elif mnemonic == 'b':
            target = re.match(r'0x([0-9a-f]+)(?:\s|$)', operands)
            require(target is not None, 'unresolved Field shared-suffix branch')
            next_address = int(target[1], 16)
        elif mnemonic == 'bl':
            require(sp == 0, 'Field call moved incoming SP away from its argument cells')
            values.pop('lr', None)  # Every real BL replaces LR, including outlines.
            target = re.match(r'0x([0-9a-f]+)(?:\s|$)', operands)
            require(target is not None, 'unresolved Field callback bridge')
            target = int(target[1], 16)
            if any(n.startswith('OUTLINED_FUNCTION_') and e[0] & ~1 == target
                   for n, e in objects.elf.functions.items()):
                effect, length = _field_outline(objects, target, outlines)
                require(get('r1') == ('formatter', 0), 'Field outline lost Formatter origin')
                if effect == 'vtable_transport':
                    require(case in FIELD_PAYLOADS and isinstance(get('r0'), int) and isinstance(get('r2'), int),
                            'Field outline lost current vtable/name producers')
                    cells[sp] = get('r0'); values['r0'] = get('r1'); values['r1'] = get('r2')
                else:
                    values['r0'] = ('sink_data', 0); values['r1'] = ('sink_vtable', 0)
                    values['r3'] = ('write_str', 0)
                    if length is not None: values['r2'] = length
                outline_calls.append({'target': target, 'effect': effect, 'frame': 0, 'site': address})
                address = next_address
                continue
            aliases = [n for n, e in objects.elf.functions.items() if e[0] & ~1 == target]
            require(aliases and all(canonical_symbol(n) == TUPLE1_ROLE for n in aliases)
                    and target in objects.sizes, 'unreviewed Field tuple bridge')
            pointer, extent, index = objects.elf.functions[aliases[0]]
            require(pointer & 1 and extent > 0 and 0 <= index < len(objects.elf.sections)
                    and all(objects.elf.functions[n] == (pointer, extent, index) for n in aliases),
                    'Field tuple bridge aliases/Thumb extent differ')
            section = objects.elf.sections[index]
            require(section[1] == 1 and section[2] & 6 == 6 and not section[2] & 1 and
                    section[3] <= target < target + extent <= section[3] + section[5],
                    'Field tuple bridge is outside its declared RO executable section')
            objects.read_only(target, extent)
            require(case in FIELD_PAYLOADS and binding is None and get('r0') == ('formatter', 0)
                    and get('r3') == ('stack', 4) and cells.get(4) == ('field', FIELD_PAYLOADS[case][1]),
                    'Field tuple reference does not match source case')
            pointer, length = get('r1'), get('r2')
            require(isinstance(pointer, int) and isinstance(length, int) and
                    objects.read_only(pointer, length) == case.encode(), 'Field tuple name differs')
            require(isinstance(cells.get(0), int), 'missing current Field vtable stack producer')
            callback = objects.debug_vtable(cells[0], FIELD_PAYLOADS[case][0])
            binding = {'case': case, 'role': FIELD_PAYLOADS[case][0], 'callback': callback,
                       'vtable': cells[0], 'bridge': target, 'site': address}
            for register in ('r0', 'r1', 'r2', 'r3', 'r12', 'lr'): values.pop(register, None)
        elif mnemonic == 'pop':
            registers = re.fullmatch(r'\{([^{}]+)\}', operands)
            require(registers is not None, 'unreviewed Field return list')
            registers = registers[1].split(', ')
            require(registers in (['r7', 'pc'], ['r7', 'lr'], ['r5', 'r6', 'r7', 'lr'],
                                  ['r2', 'r3', 'r7', 'pc']),
                    'unreviewed Field return ABI')
            before_sp = sp
            sp += len(registers) * 4
            for register in registers: values.pop(register, None)
            if 'pc' in registers:
                expected = (epilogue == 'classic' and registers == ['r7', 'pc'] and before_sp == 8
                            and history[-2] == ('add', 'sp, #0x8')) or (
                            epilogue == 'scratch' and registers == ['r2', 'r3', 'r7', 'pc'] and before_sp == 0)
                require(expected, 'Field tuple lacks its reviewed restoring epilogue')
                require(sp == 16 and binding is not None, 'Field tuple return lacks balanced binding')
                binding['outlines'] = outline_calls
                return binding, visited
        elif mnemonic == 'bx':
            # Both current families discard eight local/scratch bytes, then
            # restore R7 and the original saved LR before the sink tail.
            expected = history[-3:-1] == [('add', 'sp, #0x8'), ('pop', '{r7, lr}')]
            require(expected and operands == 'r3', 'Field sink lacks its reviewed restoring tail epilogue')
            require(get(operands) == ('write_str', 0) and case not in FIELD_PAYLOADS
                    and sp == 16 and get('r0') == ('sink_data', 0), 'unbound Field sink tail')
            pointer, length = get('r1'), get('r2')
            require(isinstance(pointer, int) and isinstance(length, int) and
                    objects.read_only(pointer, length) == case.encode(), 'Field unit name differs')
            return {'case': case, 'role': 'live_write_str', 'site': address,
                    'outlines': outline_calls}, visited
        else: raise AssertionError('unreviewed instruction in Field materialization: ' + mnemonic)
        address = next_address
    raise AssertionError('Field producer/suffix bound exceeded')


def field_case_bindings(objects, name, instructions, outlines=None):
    dispatch = enum_dispatch(objects, name, instructions)
    require(dispatch['role'] == 'field' and len(FIELD_CASES) == len(dispatch['entries']),
            'wrong typed Field case family')
    records = {i.address: i for i in instructions}
    covered = {a for a in records if a < dispatch['table']}
    bindings = []
    for case, entry in zip(FIELD_CASES, dispatch['entries'], strict=True):
        binding, visited = _field_case(objects, records, entry, case, outlines, dispatch['epilogue'])
        covered |= visited; bindings.append(binding)
    require(all(a in covered or i.raw == b'\x00\xbf' and i.mnemonic == 'nop'
                for a, i in records.items()), 'unreviewed Field body outside typed cases')
    return bindings
