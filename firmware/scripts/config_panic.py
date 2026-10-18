"""Complete finite panic objects/bodies; root/logger/default closure is separate."""

import re
import struct
from config_panic_contracts import PANIC_GRAMMARS
from config_stack import canonical_symbol, typed_function
from llvm_stack import mapping_ranges
from validation import require

PANIC_CALLBACKS = {
    "panicinfo": (
        "_RNvXs1i_NtCs_4core3fmtRNtNtNtB8_5panic10panic_info9PanicInfoNtB6_7Display3fmtCs_11panic_probe",
        ((148, 56), (136, 56)),
    ),
    "dyn_display_callback": (
        "_RNvXs1i_NtCs_4core3fmtRDNtB6_7DisplayEL_Bx_3fmtCs_5defmt",
        ((16, 8),),
    ),
    "str_callback": ("_RNvXs1i_NtCs_4core3fmtReNtB6_7Display3fmtB8_", ((20, 8),)),
    "u32_callback": ("_RNvXs8_NtNtNtCs_4core3fmt3num3impmNtB9_7Display3fmt", ((230, 64),)),
    "sink_str": (
        "_RNvXs3_NtCs_5defmt6exportNtB5_8FmtWriteNtNtCs_4core3fmt5Write9write_str",
        ((16, 8),),
    ),
    "sink_char": (
        "_RNvYNtNtCs_5defmt6export8FmtWriteNtNtCs_4core3fmt5Write10write_charB6_",
        ((140, 16), (128, 24)),
    ),
    "sink_fmt": (
        "_RNvYNtNtCs_5defmt6export8FmtWriteNtNtCs_4core3fmt5Write9write_fmtB6_",
        ((24, 8),),
    ),
}
PANIC_LITERALS = dict(
    outer_template=b"\xc0\0",
    location_template=b"\xc0\x01:\xc0\x01:\xc0\0",
    prefix=b"panicked at ",
    separator=b":\n",
    trailer=b"\xff",
)


def _callback(objects, pointer, kind):
    require(pointer & 1, "panic callback is not Thumb")
    role, variants = PANIC_CALLBACKS[kind]
    extent, frame, _ = typed_function(objects, pointer & ~1, role)
    require((extent, frame) in variants, "panic callback extent/frame differs")
    return (pointer & ~1, role, extent, frame)


def panic_object(objects, address, kind):
    """One originating interface, never a union of all plausible formatter slots."""
    if kind == "panicked":
        return objects.panic_guard(address)
    if kind in ("header_id", "display_encoding_id"):
        return objects.defmt_id(address, kind)
    if kind == "shcsr":
        require(address == 0xE000ED24, "panic terminal MMIO address differs")
        return address
    if kind in PANIC_LITERALS:
        value = PANIC_LITERALS[kind]
        require(objects.read_only(address, len(value)) == value, "panic literal/template differs")
        return address
    if kind in PANIC_CALLBACKS:
        return _callback(objects, address, kind)
    require(
        kind in ("panicinfo_vtable", "sink_vtable") and address % 4 == 0,
        "unreviewed or unaligned panic table",
    )
    count = 4 if kind == "panicinfo_vtable" else 6
    words = struct.unpack("<" + "I" * count, objects.read_only(address, count * 4))
    require(
        words[:3] == ((0, 4, 4) if count == 4 else (0, 0, 1)), "panic table type layout differs"
    )
    targets = ("panicinfo",) if count == 4 else ("sink_str", "sink_char", "sink_fmt")
    return tuple(_callback(objects, pointer, role) for pointer, role in zip(words[3:], targets))


def panic_body(objects, name, instructions):
    """Exact complete body plus typed relocations and every outgoing interface.

    Input is the current byte-checked decoder map. The return preserves child
    obligations; it cannot establish unrelated root Arguments or logger bodies.
    """
    require(name in objects.elf.functions, "missing finite panic body")
    start = objects.elf.functions[name][0] & ~1
    role = canonical_symbol(name)
    extent, frame, _ = typed_function(objects, start, role)
    candidates = [
        (key, c)
        for key, c in PANIC_GRAMMARS.items()
        if (c["extent"], c["frame"]) == (extent, frame)
        and (
            c["raw_role"] == role
            or c["raw_role"] == "@panicinfo_epilogue"
            and re.fullmatch(r"OUTLINED_FUNCTION_[0-9]+", name)
        )
    ]
    require(len(candidates) == 1, "unreviewed finite panic shape")
    shape, contract = candidates[0]
    mapping = [(start + a, start + b, k[1]) for a, b, k in contract["mapping"]]
    require(mapping_ranges(objects.elf, name) == mapping, "panic code/data mapping differs")
    raw = objects.read_only(start, extent)
    rows = sorted(
        (r for a, r in instructions.items() if start <= a < start + extent), key=lambda r: r.address
    )
    require(
        len(rows) == len(contract["grammar"]) == len(contract["instruction_widths"]),
        "panic instruction inventory differs",
    )
    code = {a for lo, hi, k in mapping if k == "t" for a in range(lo, hi)}
    covered = set()
    pairs = {}
    edges = []
    indirect = []
    for record, line, width in zip(rows, contract["grammar"], contract["instruction_widths"]):
        match = re.fullmatch(r"([0-9a-f]+) (\S+)(?:[\t ]+(.*))?", line)
        offset = int(match[1], 16)
        mnemonic = match[2]
        expected = match[3] or ""
        points = set(range(record.address, record.address + len(record.raw)))
        require(
            record.address == start + offset
            and len(record.raw) == width
            and points <= code
            and not points & covered
            and raw[offset : offset + width] == record.raw,
            "panic current bytes/coverage differ",
        )
        covered |= points
        require(record.mnemonic == mnemonic, "panic instruction/guard/return differs")
        actual = record.operands.split("@", 1)[0].strip()
        producer = re.search(r"@object:([a-z0-9_]+)\.(low|high)", expected)
        local = re.search(r"@self\+([0-9a-f]+)", expected)
        external = re.search(r"@role:(\S+)", expected)
        if producer:
            material = re.fullmatch(r"(r[0-9]+), #(0x[0-9a-f]+|[0-9]+)", actual)
            require(
                material is not None and int(material[2], 0) <= 0xFFFF,
                "invalid panic object producer",
            )
            key, part = producer[1], producer[2]
            require(part not in pairs.setdefault(key, {}), "ambiguous panic object producer")
            pairs[key][part] = int(material[2], 0)
            actual = material[1] + ", " + producer[0]
        elif local:
            target = re.search(r"0x([0-9a-f]+)(?:\s|$)", actual)
            require(
                target is not None and int(target[1], 16) == start + int(local[1], 16),
                "panic local guard/branch differs",
            )
            actual = actual[: target.start()] + local[0]
        elif external or expected == "@panicinfo_epilogue":
            target = re.match(r"0x([0-9a-f]+)(?:\s|$)", actual)
            require(target is not None, "unresolved panic external transfer")
            address = int(target[1], 16)
            if external:
                _, child_frame, _ = typed_function(objects, address, external[1])
                target_role = external[1]
            else:
                names = [n for n, e in objects.elf.functions.items() if e[0] & ~1 == address]
                require(
                    names and all(re.fullmatch(r"OUTLINED_FUNCTION_[0-9]+", n) for n in names),
                    "panic epilogue target origin differs",
                )
                child = panic_body(objects, names[0], instructions)
                require(
                    child["shape"] == "panicinfo_epilogue",
                    "panic parent tail reaches wrong outline",
                )
                child_frame = child["frame"]
                target_role = "@panicinfo_epilogue"
            edges.append((offset, mnemonic, address, target_role, child_frame))
            actual = external[0] if external else expected
        elif mnemonic in ("bx", "blx") and actual != "lr":
            indirect.append((offset, mnemonic, actual))
        require(actual == expected, "panic operands/object/cell/guard differ")
    require(covered == code, "incomplete panic mapped code")
    data = {a for lo, hi, k in mapping if k == "d" for a in range(lo, hi)}
    pool_bytes = set()
    values = {}
    bindings = {}
    row_map = {r.address - start: r for r in rows}
    for descriptor in contract["relocations"]:
        key = descriptor["object"]
        if descriptor["kind"] == "movwide_pair":
            require(
                key in pairs and set(pairs[key]) == {"low", "high"}, "missing panic object pair"
            )
            value = pairs[key]["low"] | (pairs[key]["high"] << 16)
        else:
            offset = int(descriptor["instruction_offset"], 16)
            pool = start + int(descriptor["pool_offset"], 16)
            record = row_map[offset]
            match = re.fullmatch(
                re.escape(descriptor["register"]) + r", \[pc, #(0x[0-9a-f]+|[0-9]+)\]",
                record.operands,
            )
            require(
                match is not None and ((record.address + 4) & ~3) + int(match[1], 0) == pool,
                "panic PC-relative object pool differs",
            )
            points = set(range(pool, pool + 4))
            require(
                points <= data and not points & pool_bytes, "panic object pool ownership differs"
            )
            pool_bytes |= points
            value = struct.unpack("<I", objects.read_only(pool, 4))[0]
        require(key not in values, "ambiguous panic object binding")
        values[key] = value
        bindings[key] = panic_object(objects, value, key)
    require(pool_bytes == data and set(pairs) <= set(values), "unbound panic pool/producer bytes")
    reentry = ()
    if shape.startswith("handler"):
        names = [
            n
            for n in objects.elf.functions
            if canonical_symbol(n) == PANIC_GRAMMARS["panic_fmt"]["raw_role"]
        ]
        require(len(names) == 1, "missing/ambiguous panic reentry function")
        target = objects.elf.functions[names[0]][0] & ~1
        child_extent, child_frame, _ = typed_function(objects, target, canonical_symbol(names[0]))
        require((child_extent, child_frame) == (28, 32), "panic reentry frame/extent differs")
        reentry = (child_frame, frame)
    return dict(
        shape=shape,
        function=start,
        extent=extent,
        frame=frame,
        objects=values,
        bindings=bindings,
        ordinary_edges=tuple(edges),
        indirect_sites=tuple(indirect),
        guarded_reentry_frames=reentry,
        requires_root_arguments_default_options_and_logger_closure=True,
        requires_child_sink_table=shape == "handler_literal",
        requires_parent_local_restore=32 if shape == "panicinfo_epilogue" else None,
    )
