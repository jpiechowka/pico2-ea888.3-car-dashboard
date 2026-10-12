"""Reject reviewed ARM stack-frame regressions using LLVM ELF metadata.

This is a conservative admission gate for named, source-reviewed call paths,
not a whole-program stack proof or a device high-water measurement. Changes to
those paths/dependencies require disassembly review of the child allowances.
See docs/configurator/configuration-storage.md. Never increase budgets to pass.
"""
import struct
from validation import require

CORE1_BYTES = 32768
CORE1_RESERVE = 4096
CORE0_RESERVE = 8192


def frames(data):
    require(data, 'missing stack frame metadata')
    result, cursor = {}, 0
    while cursor < len(data):
        require(cursor + 4 <= len(data), 'truncated stack function address')
        address = struct.unpack_from('<I', data, cursor)[0] & ~1
        cursor += 4
        size = 0
        for index in range(5):
            require(cursor < len(data), 'truncated stack size')
            byte = data[cursor]
            cursor += 1
            size |= (byte & 127) << (index * 7)
            require(size <= 0xffffffff, 'stack size overflow')
            if byte < 128:
                require(index == 0 or byte != 0, 'noncanonical stack size')
                break
        else:
            raise AssertionError('unterminated stack size')
        require(address not in result or result[address] == size, 'conflicting stack sizes')
        result[address] = size
    return result


def budgets(boot, constructor, runtime, dashboard, headroom):
    require(boot + CORE0_RESERVE <= headroom, 'boot stack path lacks 8 KiB reserve')
    require(dashboard + CORE0_RESERVE <= headroom, 'dashboard stack path lacks 8 KiB reserve')
    require(constructor + CORE1_RESERVE <= CORE1_BYTES, 'core1 constructor stack lacks 4 KiB reserve')
    require(runtime + CORE1_RESERVE <= CORE1_BYTES, 'core1 runtime stack lacks 4 KiB reserve')


def task_frames(functions, sizes, name):
    matches = {address & ~1 for symbol, (address, _, _) in functions.items()
               if name in symbol and 'write_in_place' not in symbol}
    polls = [symbol for symbol in functions
             if name in symbol and 'TaskStorage' in symbol and '4poll' in symbol]
    require(len(polls) == 1, 'missing or ambiguous task poll: ' + name)
    require(all(address in sizes for address in matches), 'missing task frame metadata: ' + name)
    # LLVM may outline the future body behind a tiny TaskStorage::poll shim.
    # Count all task-associated bodies/closures conservatively, not only the shim.
    return sum(sizes[address] for address in matches)


def configuration_frames(functions, sizes):
    """Reviewed slot call tree, with sibling branches taking their maximum.

    Required emitted boundaries come from the traced LLVM 23 ARM captures.
    Header/ConfigSeed/VecSeed/default filling are inlined in the slot loader;
    if emitted, their frames are added at their source call-tree positions.
    Unknown config helpers, including validation descendants, require review.
    This is intentionally a named-path gate, not an instruction decoder.
    """
    covered = set()

    def select(*tokens, optional=False):
        names = [n for n in functions if all(t in n for t in tokens)]
        addresses = {functions[n][0] & ~1 for n in names}
        require(optional or len(addresses) == 1,
                'missing or ambiguous config stack path: ' + str(tokens))
        require(optional or bool(names), 'missing config stack path')
        require(all(a in sizes for a in addresses), 'missing config frame metadata: ' + str(tokens))
        covered.update(names)
        return addresses

    def weight(path):
        return sum(sizes[a] for a in path)

    def branch(parent, *children):
        # Deduplicate aliases across the complete simultaneously resident path.
        return max((parent | c for c in children), key=weight) if children else parent

    def dec(token):
        return select('dashboard_config', '12next_element', token)

    def ser(token):
        return select('dashboard_config', '15serialize_field', token)

    def outlined(*tokens):
        return select('dashboard_config', *tokens, optional=True)

    loader = select('dashboard_config', 'ConfigSlot', '12load_at_boot')
    validate = select('dashboard_config', '12validate_ref')
    canonical = select('dashboard_config', '17compare_canonical')
    utf8 = select('4core', '9from_utf8')
    # The only boot children without .stack_sizes are compiler-builtins memory
    # routines: memcpy 72 B, memcpy4 <=16 B then tail-calls memcpy, memcmp 32 B.
    # They and panic formatting/entry/executor children fit the separate 1024 B
    # allowance in check(). UTF-8 has metadata and is counted explicitly here.
    view = branch(dec('4View'), utf8)
    text = branch(dec('11StringInner'), utf8)
    color = dec('5ColorE')
    warning_colors = dec('13WarningColors')
    warning = branch(dec('15WarningSettings'), text, color)
    widget_colors = branch(dec('12WidgetColors'), color, warning_colors)
    widget = branch(dec('6WidgetE'), utf8, warning, widget_colors, dec('10GraphRange'), dec('5Gauge'))
    source = branch(select('dashboard_config', '12next_elementNt', '13ChannelSource'), color)
    sources = branch(select('dashboard_config', '12next_elementA', '13ChannelSource'), source)
    controls = branch(select('dashboard_config', '15decode_in_place4next', '10ControlMap'),
                      branch(dec('14ScreenBindings'), dec('13ControlAction')))
    fields = [sources, controls]
    for token in ('19WarningPresentation', '4Fuel', '13WarningColors'):
        fields.append(select('dashboard_config', '15decode_in_place4next', token))
    # ConfigSeed and VecSeed may each emit deserialize and visit_seq frames.
    # The two vector instantiations are siblings, never simultaneous.
    fields.extend([branch(outlined('VecSeed', '4View'), view),
                   branch(outlined('VecSeed', '6Widget'), widget)])
    seed = branch(outlined('ConfigSeed'), *fields)
    header = outlined('16inspect_envelope') | outlined('9crc_parts')
    serial_widget = branch(ser('6Widget'), ser('5ColorE'), ser('13WarningColors'))
    serial_fuel = branch(ser('4Fuel'), ser('serialize_fieldf'))
    serial_controls = branch(ser('10ControlMap'), ser('13ControlAction'))
    canonical = branch(canonical, serial_widget, serial_fuel, serial_controls,
                       ser('13ChannelSource'), ser('19WarningPresentation'), ser('13WarningColors'))
    validation = branch(validate, outlined('8controls17validate_controls'),
                        outlined('8validate10text_valid'),
                        *[select('dashboard_config', token) for token in (
        '9Placement8occupied', '12WidgetColors12invalid_role', '5Gauge5valid',
        '5Color9contrasts', '7catalog10definition', '4fuel13stoichiometry',
        '6AccessNt')])
    decoding = branch(outlined('19decode_current_into'), header, seed, canonical, validation)
    fallback = branch(outlined('19fill_default_config'), outlined('14default_widget'),
                      select('dashboard_config', '7catalog10definition'))
    # Validation follows decode/canonical or refill after their children return.
    result = branch(loader, decoding, validation, fallback)
    # Error Debug formatting is already covered by check()'s reviewed 1024 B
    # panic/entry allowance. Still require metadata and retain its observed
    # maximum single frame (80 B); changes require re-review of that allowance.
    debug = outlined('5Debug3fmt')
    require(all(sizes[a] <= 80 for a in debug), 'config panic formatting frame changed; review allowance')
    # These three equality implementations serve dashboard cache comparisons,
    # not the boot tree. Access equality above IS a validation child. Do not
    # broadly exempt PartialEq, whole modules or names containing "validate".
    covered.update(n for n in functions if 'dashboard_config' in n
                   and '3cmp9PartialEq2eq' in n
                   and any(t in n for t in ('14DisplayOptionsNt', '15WarningSettingsNt', '12WidgetColorsNt')))
    # This one runtime snapshot contains dashboard_config in its generic pointee.
    # It is a required/countable publication boundary below, not a boot child.
    # Do not exempt other owner methods or unknown config helpers.
    covered.update(n for n in functions if all(token in n for token in (
        '14dashboard_core11acquisitionI', '16AcquisitionOwnerR',
        '16dashboard_config8validate15ValidatedConfigE8snapshot')))
    relevant = [n for n in functions if 'dashboard_config' in n]
    require(all(n in covered for n in relevant),
            'unreviewed config stack helpers: ' + str([n for n in relevant if n not in covered]))
    return weight(result)


def publication_frames(functions, sizes):
    # Nested path: firmware publication -> immutable owner -> private storage
    # -> sensor snapshot. Retain legacy capture support for baseline comparison.
    split = any('AcquisitionOwner' in n or 'AcquisitionState' in n for n in functions)
    boundaries = [('pico2', 'publish_snapshot')]
    boundaries += ([('AcquisitionOwner', '8snapshot'), ('AcquisitionState', '8snapshot')]
                   if split else [('AcquisitionModel', '8snapshot')])
    for tokens in boundaries:
        boundary = {address & ~1 for name, (address, _, _) in functions.items()
                    if all(token in name for token in tokens)}
        require(len(boundary) == 1, 'missing or ambiguous publication boundary: ' + str(tokens))
    matches = {address & ~1 for name, (address, _, _) in functions.items()
               if ('pico2' in name and 'publish_snapshot' in name)
               or (any(token in name for token in ('AcquisitionModel', 'AcquisitionOwner',
                                                   'AcquisitionState', 'Session', 'Vehicle', 'SensorState'))
                   and any(token in name for token in ('8snapshot', 'tracked_snapshot', 'snapshot_with_identity')))}
    require(all(address in sizes for address in matches), 'missing publication frame metadata')
    return sum(sizes[address] for address in matches)


def constructor_frames(functions, sizes):
    # Constructors may inline. Every emitted owner/storage/policy frame must be
    # counted, including the extra owner boundary; aliases count once by address.
    matches = {address & ~1 for name, (address, _, _) in functions.items()
               if 'configured_vehicle' in name
               or (any(token in name for token in ('AcquisitionModel', 'AcquisitionOwner',
                                                   'AcquisitionState', 'Session', 'Vehicle',
                                                   'RuleEngine', 'RuleEngineState'))
                   and any(token in name for token in ('new_inner', 'with_policy',
                                                       'with_configuration', 'configured', 'legacy', '3new')))}
    require(all(address in sizes for address in matches), 'missing constructor frame metadata')
    return sum(sizes[address] for address in matches)


def check(elf, headroom):
    require('.stack_sizes' in elf.named, 'build with -Z emit-stack-sizes')
    section = elf.named['.stack_sizes']
    require(section[1] == 1 and not section[2] & 2, 'stack metadata must not be allocated')
    for kind, offset, _, _, size, _, _, _ in elf.segments:
        if kind == 1 and size:
            require(section[4] + section[5] <= offset or offset + size <= section[4],
                    'stack metadata overlaps a LOAD segment')
    sizes = frames(elf.section_bytes('.stack_sizes'))

    def selected(predicate, required=False):
        matches = {elf.functions[name][0] & ~1 for name in elf.functions if predicate(name)}
        if required:
            require(len(matches) == 1, 'missing or ambiguous stack path: ' + str(matches))
        values = []
        for address in matches:
            require(address in sizes, 'missing frame metadata for ' + hex(address))
            values.append(sizes[address])
        return sum(values)

    def task(name):
        return task_frames(elf.functions, sizes, name)

    boot_poll = task('embassy_main_task')
    dashboard = task('dashboard_task_task')
    acquisition = task('acquisition_task_task')
    initializer = selected(lambda n: 'view_config' in n and 'initialize' in n, True)
    configuration = configuration_frames(elf.functions, sizes)
    constructor = constructor_frames(elf.functions, sizes)
    snapshots = publication_frames(elf.functions, sizes)
    # 1024 includes resident executor/entry frames, ordinary constructors'
    # memcpy/memclr, boot validation/contrast and snapshot memory helpers.
    # Vehicle step/reset/telemetry paths have an independently reviewed 8KiB
    # child allowance. Runtime and construction are different poll branches.
    boot_bound = boot_poll + initializer + configuration + 1024
    constructor_bound = acquisition + constructor + 1024
    runtime_bound = acquisition + max(snapshots + 1024, 8192)
    dashboard_bound = dashboard + 16384  # drawing/formatting, flush, startup children
    budgets(boot_bound, constructor_bound, runtime_bound, dashboard_bound, headroom)
    print(f'Frame admission: boot {boot_bound} B, core1 construct {constructor_bound} B, '
          f'core1 runtime {runtime_bound} B, dashboard {dashboard_bound} B; '
          'excludes unqualified hardware high-water use')
