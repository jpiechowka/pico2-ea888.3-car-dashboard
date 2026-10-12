"""Reject reviewed ARM stack-frame regressions using LLVM ELF metadata.

This is a conservative admission gate for named, source-reviewed call paths,
not a whole-program stack proof or a device high-water measurement. Changes to
those paths/dependencies require disassembly review of the child allowances.
See docs/configurator/gauges-and-colours.md. Never increase budgets to pass.
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
        matches = [name for name in elf.functions if predicate(name)]
        if required:
            require(len(matches) == 1, 'missing or ambiguous stack path: ' + str(matches))
        values = []
        for name in matches:
            address = elf.functions[name][0] & ~1
            require(address in sizes, 'missing frame metadata for ' + name)
            values.append(sizes[address])
        return sum(values)

    def task(name):
        return task_frames(elf.functions, sizes, name)

    boot_poll = task('embassy_main_task')
    dashboard = task('dashboard_task_task')
    acquisition = task('acquisition_task_task')
    initializer = selected(lambda n: 'view_config' in n and 'initialize' in n, True)
    loader = selected(lambda n: 'dashboard_config' in n and 'load_at_boot' in n, True)
    decoder = selected(lambda n: 'dashboard_config' in n and
                       any(t in n for t in ('5codec6decode', 'default_config')))
    constructor = selected(lambda n: 'configured_vehicle' in n)
    # Usually inlined. Count separately if compiler changes introduce frames.
    constructor += selected(lambda n: (
        any(t in n for t in ('AcquisitionModel', 'Session', 'Vehicle', 'RuleEngine'))
        and any(t in n for t in ('new_inner', 'with_policy', 'configured', '3new'))
    ))
    snapshots = selected(lambda n: (
        any(t in n for t in ('AcquisitionModel', 'Session', 'SensorState')) and '8snapshot' in n
    ))
    # 1024 includes resident executor/entry frames, ordinary constructors'
    # memcpy/memclr, boot validation/contrast and snapshot memory helpers.
    # Vehicle step/reset/telemetry paths have an independently reviewed 8KiB
    # child allowance. Runtime and construction are different poll branches.
    boot_bound = boot_poll + initializer + loader + decoder + 1024
    constructor_bound = acquisition + constructor + 1024
    runtime_bound = acquisition + max(snapshots + 1024, 8192)
    dashboard_bound = dashboard + 16384  # drawing/formatting, flush, startup children
    budgets(boot_bound, constructor_bound, runtime_bound, dashboard_bound, headroom)
    print(f'Frame admission: boot {boot_bound} B, core1 construct {constructor_bound} B, '
          f'core1 runtime {runtime_bound} B, dashboard {dashboard_bound} B; '
          'excludes unqualified hardware high-water use')
