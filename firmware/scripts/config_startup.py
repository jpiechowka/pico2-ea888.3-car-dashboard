"""Closed Reset-copy bridge with historical Rust and current assembly hooks.

Historical source-language qualification remains explicit. The current exact
stackless upstream assembly is separately admitted; neither path infers
hardware permission or application source/task/alias/loader closure.
"""

from dataclasses import replace
import re
import struct
from config_finite import _literal_body
from config_owner_entry import configuration_entry_boundary, _direct_target
from config_refill_children import _readonly
from config_stack import typed_function, _typed_function_extent, transfer_family
from config_startup_contracts import STARTUP, ASSEMBLY_PRE_INIT
from validation import require


def _symbol(elf, name, info, size, index):
    entries = [v for v in elf.symbol_entries if v[0] == name]
    require(len(entries) == 1, "missing or ambiguous startup symbol: " + name)
    raw, value, extent, kind, section = entries[0]
    require(
        all(type(x) is int for x in (value, extent, kind, section))
        and (kind, extent, section) == (info, size, index)
        and 0 <= value <= 0xFFFFFFFF,
        "startup symbol type/extent/section differs: " + name,
    )
    return value


def _function(objects, address, role):
    if role == "__pre_init" and objects.elf.functions.get("__pre_init", (0, 0))[1] == 42:
        extent, name = _typed_function_extent(objects, address, role)
        index = objects.elf.functions[name][2]
        require(
            name == "__pre_init"
            and _symbol(objects.elf, name, 18, 42, index) == address | 1
            and extent == 42
            and (
                address not in objects.sizes
                or (type(objects.sizes[address]) is int and objects.sizes[address] == 0)
            ),
            "assembly pre_init is not the exact zero-growth function",
        )
        return 42, 0, name
    if role != "Reset":
        return typed_function(objects, address, role)
    extent, name = _typed_function_extent(objects, address, role)
    index = objects.elf.functions[name][2]
    require(
        name == "Reset"
        and _symbol(objects.elf, "Reset", 18, 62, index) == address | 1
        and extent == 62
        and ("Reset" in objects.elf.functions)
        and (
            address not in objects.sizes
            or (type(objects.sizes[address]) is int and objects.sizes[address] == 0)
        ),
        "Reset is not the reviewed complete zero-growth function",
    )
    # This zero is usable only alongside the complete 62-byte contract below.
    # Missing metadata for any other function still fails typed_function.
    return 62, 0, name


def _no_policy(*args):
    raise AssertionError("unreviewed startup object or outline")


def _external_pool(objects, name, slot):
    """A separate Reset dependency, never an enlarged STT_FUNC extent."""
    elf = objects.elf
    start, extent, index = elf.functions[name]
    start &= ~1
    s = elf.sections[index]
    require(
        type(slot) is int
        and slot % 4 == 0
        and s[3] <= slot < slot + 4 <= s[3] + s[5]
        and not (start < slot + 4 and slot < start + extent),
        "Reset pool overlaps code or leaves its declared section",
    )
    points = {}
    for label, address, section in elf.mapping_symbols:
        if section != index:
            continue
        require(
            type(address) is int
            and s[3] <= address < s[3] + s[5]
            and address % 2 == 0
            and label[1] in ("t", "d"),
            "invalid Reset section mapping",
        )
        require(
            address not in points or points[address] == label[1], "conflicting Reset pool mapping"
        )
        points[address] = label[1]
    bounds = sorted(points)
    require(
        any(
            a <= slot < slot + 4 <= z and points[a] == "d"
            for a, z in zip(bounds, bounds[1:] + [s[3] + s[5]])
        ),
        "Reset external literal is not completely mapped data",
    )
    require(
        all(
            not (a & ~1 < slot + 4 and slot < (a & ~1) + n)
            for a, n, ix in elf.functions.values()
            if n and ix == index
        ),
        "Reset literal aliases another function extent",
    )
    return struct.unpack("<I", _readonly(objects, slot, 4, 4))[0]


def assembly_pre_init(objects, rows):
    """Match the complete stackless hook and its three external MMIO words."""
    elf = objects.elf
    require("__pre_init" in elf.functions, "missing assembly startup hook")
    start = elf.functions["__pre_init"][0] & ~1
    # The decoder's text must agree with the exact reviewed nonliteral opcodes.
    # Only the three LDR displacements may vary with external pool placement.
    fixed = bytes.fromhex(
        "0a48012101600a480906016002688a42fcd10021016002688a42fcd10548016801225207114301607047"
    )
    raw = objects.read_only(start, 42)
    require(
        all(raw[i : i + 2] == fixed[i : i + 2] for i in range(0, 42, 2) if i not in (0, 6, 28)),
        "assembly startup fixed opcode differs",
    )
    contract = dict(ASSEMBLY_PRE_INIT)
    operations = []
    pools = []
    expected = (0xD000017C, 0x40018004, 0xE000E008)
    for offset, width, text in contract["instructions"]:
        match = re.search(r"@hook_pool([0-9]+)", text)
        if match:
            row = rows.get(start + offset)
            require(
                row is not None and row.mnemonic == "ldr", "missing assembly MMIO literal consumer"
            )
            actual = re.fullmatch(r"r0, \[pc, #(0x[0-9a-f]+|[0-9]+)\]", row.operands)
            require(actual is not None, "assembly MMIO literal receiver differs")
            immediate = int(actual[1], 0)
            slot = ((start + offset + 4) & ~3) + immediate
            require(
                immediate % 4 == 0
                and 0 <= immediate <= 1020
                and row.raw == struct.pack("<H", 0x4800 | immediate // 4),
                "assembly literal opcode or displacement differs",
            )
            value = _external_pool(objects, "__pre_init", slot)
            require(value == expected[int(match[1])], "assembly startup MMIO address differs")
            pools.append((start + offset, slot, value))
            text = "ldr " + row.operands
        operations.append((offset, width, text))
    require(
        len(pools) == 3 and len({p[1] for p in pools}) == 3,
        "assembly startup literal ownership differs",
    )
    contract["instructions"] = operations
    proof = _literal_body(
        objects,
        "__pre_init",
        rows,
        "__pre_init",
        "assembly_rp235x",
        contract,
        _no_policy,
        _no_policy,
        window=start,
        function_consumer=_function,
    )
    require(
        not proof["ordinary_edges"] and not proof["indirect_sites"], "assembly startup gains a call"
    )
    proof.update(
        external_literals=tuple(pools),
        zero_growth_requires_complete_assembly_contract=True,
        reviewed_upstream_commit="c003af1767be1b7602ecf2a3b155eb6ce543e06a",
    )
    return proof


def _bodies(objects, rows, mode, expected):
    elf = objects.elf
    require("Reset" in elf.functions and "__pre_init" in elf.functions, "missing startup function")
    reset = elf.functions["Reset"][0] & ~1
    preinit = elf.functions["__pre_init"][0] & ~1
    hook_shape = "assembly_rp235x" if elf.functions["__pre_init"][1] == 42 else mode
    hook_contract = STARTUP[hook_shape]
    require(
        _symbol(elf, "__pre_init", 18, hook_contract["extent"], elf.functions["__pre_init"][2])
        == preinit | 1,
        "pre_init raw definition differs",
    )
    clean = {
        a: replace(r, operands=r.operands.split("@", 1)[0].split("<", 1)[0].strip())
        for a, r in rows.items()
    }
    contract = dict(STARTUP["Reset"])
    operations = []
    pools = []
    for offset, width, text in contract["instructions"]:
        match = re.search(r"@reset_pool([0-9]+)", text)
        if match:
            at = reset + offset
            row = clean.get(at)
            register = text.partition(" ")[2].partition(",")[0]
            require(row is not None and row.mnemonic == "ldr", "missing Reset literal consumer")
            actual = re.fullmatch(
                re.escape(register) + r", \[pc, #(0x[0-9a-f]+|[0-9]+)\]", row.operands
            )
            require(actual is not None, "Reset literal receiver/immediate differs")
            immediate = int(actual[1], 0)
            rd = int(register[1:])
            slot = ((at + 4) & ~3) + immediate
            require(
                immediate % 4 == 0
                and 0 <= immediate <= 1020
                and row.raw == struct.pack("<H", 0x4800 | (rd << 8) | (immediate // 4)),
                "Reset literal opcode/register/immediate differs",
            )
            ordinal = int(match[1])
            value = _external_pool(objects, "Reset", slot)
            require(value == expected[ordinal], "Reset source/destination/system literal differs")
            pools.append((at, slot, value))
            text = "ldr " + row.operands
        operations.append((offset, width, text))
    require(
        len(pools) == 6 and len({v[1] for v in pools}) == 6,
        "Reset loses a distinct mandatory literal",
    )
    contract["instructions"] = operations
    # The window is exactly the independently checked whole 62-byte function.
    # Its six *external* PC dependencies were all bound above; the complete-body
    # matcher's internal-pool policy is not widened for other consumers.
    proof = _literal_body(
        objects,
        "Reset",
        clean,
        "Reset",
        "Reset",
        contract,
        _no_policy,
        _no_policy,
        window=reset,
        function_consumer=_function,
    )
    child = (
        assembly_pre_init(objects, clean)
        if hook_shape == "assembly_rp235x"
        else _literal_body(
            objects, "__pre_init", clean, "__pre_init", mode, hook_contract, _no_policy, _no_policy
        )
    )
    require(
        not child["ordinary_edges"] and not child["indirect_sites"] and not proof["indirect_sites"],
        "startup gains an unexamined hook/helper/indirect call",
    )
    require(
        tuple((o, op, role, frame) for o, op, target, role, frame in proof["ordinary_edges"])
        == ((0, "bl", "__pre_init", hook_contract["frame"]), (56, "bl", "main", 8)),
        "Reset ordinary/main seam differs",
    )
    proof.update(external_literals=tuple(pools), zero_growth_requires_complete_Reset_contract=True)
    return proof, child, clean


def _ranges(objects, initial, hook_frame=8):
    elf = objects.elf
    data = elf.named.get(".data")
    bss = elf.named.get(".bss")
    uninit = elf.named.get(".uninit")
    vector = elf.named.get(".vector_table")
    require(
        all(v is not None for v in (data, bss, uninit, vector)), "missing exact startup allocation"
    )

    def index(section):
        indices = [i for i, v in enumerate(elf.sections) if v == section]
        require(len(indices) == 1, "ambiguous startup section")
        return indices[0]

    for section, kind, flags in ((data, 1, 7), (bss, 8, 3), (uninit, 8, 3), (vector, 1, 2)):
        require(
            section[1] == kind
            and section[2] & 7 == flags
            and all(type(x) is int for x in section)
            and 0 <= section[3] < section[3] + section[5] <= 0x100000000,
            "startup section type/extent differs",
        )
    d = _symbol(elf, "__sdata", 16, 0, index(data))
    de = _symbol(elf, "__edata", 16, 0, index(data))
    z = _symbol(elf, "__sbss", 16, 0, index(bss))
    ze = _symbol(elf, "__ebss", 16, 0, index(bss))
    load = _symbol(elf, "__sidata", 16, 0, 0xFFF1)
    sp = _symbol(elf, "_stack_start", 16, 0, 0xFFF1)
    bottom = _symbol(elf, "_stack_end", 16, 0, index(uninit))
    ue = _symbol(elf, "__euninit", 16, 0, index(uninit))
    require(
        (d, de) == (data[3], data[3] + data[5])
        and (z, ze) == (bss[3], bss[3] + bss[5])
        and all(v % 4 == 0 for v in (d, de, z, ze, load))
        and 0x20000000 <= d < de <= z < ze <= ue == uninit[3] + uninit[5]
        and bottom == ((ue + 7) & ~7)
        and bottom < sp <= 0x20080000
        and sp % 8 == 0
        and 0x10000000 <= load < load + (de - d) <= 0x101FE000,
        "startup zero/copy/stack ranges or word alignment differ",
    )
    require(
        d <= initial["address"] < initial["address"] + 14604 <= de,
        "Config is outside complete initialized data",
    )
    target = [v for v in elf.segments if v[0] == 1 and v[2] == d and v[3] == load]
    require(len(target) == 1, "startup lacks unique complete physical data load")
    segment = target[0]
    require(
        all(type(v) is int for v in segment)
        and segment[1] == data[4]
        and segment[4] == segment[5] == de - d
        and segment[6] & 7 == 7
        and 0 <= segment[1] < segment[1] + segment[4] <= len(elf.data),
        "startup section/physical/file mapping differs",
    )
    for other in elf.segments:
        if other is segment or other[0] != 1:
            continue
        require(
            all(type(v) is int for v in other)
            and 0 <= other[2] <= other[2] + other[5] <= 0x100000000,
            "startup virtual load wraps or is untyped",
        )
        if other[5]:
            require(
                not (other[2] < de and d < other[2] + other[5]),
                "competing virtual load overlaps initialized data destination",
            )
        if not other[4]:
            continue
        a, b = other[3], other[3] + other[4]
        require(0 <= a < b <= 0x100000000, "startup physical load wraps")
        require(
            not (a < load + (de - d) and load < b),
            "competing physical load overlaps initialized data source",
        )
    require(
        segment[1] + initial["address"] - d == initial["file_offset"]
        and load + initial["address"] - d == initial["load_physical"],
        "Config source slice loses the exact initial-image mapping",
    )
    # No overlap waiver for a small partial allocation or another live static.
    for section in elf.sections:
        if not section[2] & 2 or not section[5]:
            continue
        lo, hi = section[3], section[3] + section[5]
        require(0 <= lo < hi <= 0x100000000, "startup allocated section wraps")
        require(
            not (lo < sp and sp - 8 < hi),
            "initial stack reserve overlaps an allocated object/section",
        )
        if section != data:
            require(
                not (lo < de and d < hi),
                "initialized data destination has an overlapping allocation",
            )
        if section != bss:
            require(not (lo < ze and z < hi), "zero destination has an overlapping allocation")
    v = _symbol(elf, "__vector_table", 16, 0, index(vector))
    rv = _symbol(elf, "__RESET_VECTOR", 17, 4, index(vector))
    require(
        v == vector[3]
        and rv == v + 4
        and 0x10000000 <= v < v + 8 <= vector[3] + vector[5]
        and v % 4 == 0,
        "startup vector allocation/field differs",
    )
    words = struct.unpack("<II", _readonly(objects, v, 8, 4))
    require(
        words[0] == sp and elf.data[:7] == b"\x7fELF\x01\x01\x01" and len(elf.data) >= 52,
        "startup vector SP or ELF entry format differs",
    )
    return dict(
        data=(d, de),
        bss=(z, ze),
        source=(load, load + de - d),
        source_file=(data[4], data[4] + data[5]),
        Config_address=initial["address"],
        Config_source=load + initial["address"] - d,
        step=4,
        words=(de - d) // 4,
        zero_words=(ze - z) // 4,
        initial_sp=sp,
        stack_bottom=bottom,
        pre_init_stack=(sp - hook_frame, sp),
        vector=(v, rv, words[1]),
        header_entry=struct.unpack_from("<I", elf.data, 24)[0],
    )


def startup_configuration_copy(objects, instructions):
    """Conditional copy-at-main lineage, not an unqualified live owner."""
    entry = configuration_entry_boundary(objects, instructions)
    initial = entry["initial"]
    hook_frame = 0 if objects.elf.functions.get("__pre_init", (0, 0))[1] == 42 else 8
    ranges = _ranges(objects, initial, hook_frame)
    expected = (
        ranges["bss"][0],
        ranges["bss"][1],
        ranges["data"][0],
        ranges["data"][1],
        ranges["source"][0],
        0xE000ED88,
    )
    reset, preinit, rows = _bodies(objects, instructions, entry["mode"], expected)
    require(
        ranges["vector"][2] == ranges["header_entry"] == reset["function"] | 1,
        "Reset vector/ELF/typed function entry differ",
    )
    main = entry["entry"]["phases"]["main"]
    edges = reset["ordinary_edges"]
    require(
        edges[0][2] == preinit["function"] and edges[1][2] == main["function"],
        "startup pre_init or main seam targets a different complete body",
    )
    spans = (
        (reset["function"], reset["extent"], None),
        (preinit["function"], preinit["extent"], reset["function"]),
        (main["function"], main["extent"], reset["function"] + 56),
    )
    for at, row in rows.items():
        family = transfer_family(row.mnemonic.removesuffix(".w"))
        if family not in ("b", "bl", "cbz", "cbnz"):
            continue
        target = _direct_target(row, family)
        for start, size, caller in spans:
            if start <= target < start + size and not start <= at < start + size:
                require(
                    caller is not None
                    and at == caller
                    and row.mnemonic == "bl"
                    and target == start,
                    "foreign direct ingress bypasses startup copy or hook",
                )
    return dict(
        mode=entry["mode"],
        phase="machine_copy_at_main_pending_application_owner",
        entry=entry,
        reset=reset,
        pre_init=preinit,
        copy=ranges,
        reaching_phase_requires=(
            "pre_init_returned",
            "complete_bss_zero_exit",
            "complete_data_copy_exit",
        ),
        pre_init_outcomes=(
            "nonterminating_poll_has_no_successor",
            "normal_restored_before_zero_and_copy",
        ),
        source_qualification=(
            "reviewed_stackless_assembly_pre_init"
            if hook_frame == 0
            else "deprecated_Rust_pre_init_emitted_machine_only"
        ),
        requires_M2_pre_init_source_reconciliation=hook_frame != 0,
        hardware_qualified=False,
        startup_stack_alternatives=(
            (0, hook_frame),
            (0,) + tuple(frame for at, frame in entry["entry"]["entry_chain"]),
        ),
        post_copy_direct_store=(0xE000ED88, 4, "CPACR read OR 0x00f00000"),
        requires_expected_vector_entry_and_successful_ordinary_memory_execution=True,
        requires_first_batch_source_alias_and_specialized_loader_closure=True,
        requires_all_remaining_implicit_computed_diagnostic_and_graph_closure=True,
        must_not_reuse_initial_phase_after_decode_refill_or_publication=True,
    )
