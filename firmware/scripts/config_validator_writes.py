"""Current SizeZ direct local-write envelope, not visitor/source admission.

Arithmetic values remain opaque. This closed source family classifies direct
SP/R7 local stores and forbids new local frame-base writes. Pointer-directed
effects, child preservation and all control/typed-selector interfaces stay
unresolved until the enclosing producer composes them. No ISA execution or
policy is performed here.
"""

import re
from config_validator_result import validator_result_boundary
from config_regions import borrow, require_writes
from llvm_stack import mapping_ranges
from validation import require


# Closed current instruction families for this source body. They are not
# general arithmetic/ISA semantics. Unknown families cannot hide a new store,
# exception or system-register effect behind an opaque scalar operation.
_OPERATIONS = frozenset(
    """add add.w addeq adds and ands b b.w beq beq.w bfi bge.w bgt.w bhi bhi.w bhs bhs.w
    bic bl ble.w blo bls bls.w blt blt.w bmi bne bne.w bpl cbnz cbz clz cmp cmp.w eors it ldm ldr ldr.w
    ldrb ldrb.w ldrd ldrh ldrh.w lsl.w lsls lsr.w lsrs mla mov mov.w moveq movgt movhi movlo movlt movmi
    movne movs movw mvns nop orr.w orrs push push.w smlabb str str.w strb strb.w strd strh strh.w sub
    sub.w subs tbb tst ubfx uxtb uxtb.w uxth vcmp.f32 vdiv.f32 vldr vmla.f32 vmov vmov.f32 vmrs vmul.f32
    vpop vpush vstr vsub.f32""".split()
)
_REGISTER = r"(?:r(?:[0-9]|1[0-2])|lr)"
_IMMEDIATE = r"-?(?:0x[0-9a-f]+|[0-9]+)"
_WIDTHS = {"str": 4, "strh": 2, "strb": 1, "strd": 8}


def _frame_base(offset, row):
    """Only the reviewed prologue/restore may change this local SP/R7 map."""
    operation = row.mnemonic
    operand = row.operands.split("@", 1)[0].split("<", 1)[0].strip()
    require(operation in _OPERATIONS, "unclassified validator instruction/effect family")
    require(
        not re.search(r"\br(?:1[3-9]|[2-9][0-9]+)\b", operand),
        "noncanonical validator register alias",
    )
    if operation in ("push", "push.w", "vpush", "vpop"):
        require(offset in (0, 4, 8, 350), "new validator stack-control site")
        return
    if offset in (2, 12, 348):
        return  # Exact operands rechecked by Result boundary.
    if "[" in operand and operation.removesuffix(".w") not in _WIDTHS:
        outputs = operand.split("[", 1)[0]
        require(
            not re.search(r"\b(?:sp|r7)\b", outputs),
            "validator load output aliases its current SP/R7",
        )
    if re.search(r"\bsp\b", operand):
        memory = re.fullmatch(r"[^\[\]]+, \[sp(?:, #" + _IMMEDIATE + r")?\]", operand)
        address = operation in ("add", "add.w") and re.fullmatch(
            _REGISTER + r", sp, #" + _IMMEDIATE, operand
        )
        require(
            memory is not None or address is not None,
            "validator changes or writes back its current SP",
        )
    if re.search(r"\br7\b", operand):
        memory = re.fullmatch(r"[^\[\]]+, \[r7, #" + _IMMEDIATE + r"\]", operand)
        address = operation == "sub.w" and re.fullmatch(
            r"r(?:[0-6]|[89]|1[0-2]), r7, #" + _IMMEDIATE, operand
        )
        # Storing R7 reads it; loading into it or using it as another output
        # would invalidate every frame-relative footprint below.
        source = operation.removesuffix(".w") in _WIDTHS and re.match(
            r"(?:r7,|" + _REGISTER + r", r7,)", operand
        )
        require(
            (memory is not None and not re.match(r"r7,", operand))
            or address is not None
            or source is not None,
            "validator changes or writes back its current R7",
        )


def validator_local_writes(objects, name, rows, context, call):
    boundary = validator_result_boundary(objects, name, rows, context, call)
    parent = objects.elf.functions[name][0] & ~1
    extent = objects.elf.functions[name][1]
    q = context.frame
    mapping = mapping_ranges(objects.elf, name)
    code = {a for lo, hi, kind in mapping if kind == "t" for a in range(lo, hi)}
    covered = set()
    local = []
    pending = []
    for at, row in sorted(rows.items()):
        if not parent <= at < parent + extent:
            continue
        points = set(range(at, at + len(row.raw)))
        require(
            type(at) is int
            and row.address == at
            and len(row.raw) in (2, 4)
            and points <= code
            and not points & covered
            and objects.read_only(at, len(row.raw)) == row.raw,
            "validator local-effect current coverage/bytes differ",
        )
        covered |= points
        _frame_base(at - parent, row)
        operation = row.mnemonic.removesuffix(".w")
        operand = row.operands.split("@", 1)[0].split("<", 1)[0].strip()
        if operation not in _WIDTHS and operation != "vstr":
            continue
        if operation == "vstr":
            pending.append(row)
            continue
        values = _REGISTER + (r", " + _REGISTER if operation == "strd" else "")
        match = re.fullmatch(values + r", \[(sp|r7)(?:, #(" + _IMMEDIATE + r"))?\]", operand)
        if match is None:
            require(
                not re.search(r"\[(?:sp|r7)(?:,|\])", operand),
                "unclassified local store/base writeback",
            )
            pending.append(row)
            continue
        offset = (312 if match[1] == "r7" else 0) + (
            int(match[2], 0) if match[2] is not None else 0
        )
        width = _WIDTHS[operation]
        require(
            0 <= offset < offset + width <= 216 and offset % min(width, 4) == 0,
            "validator local store escapes locals or loses its natural alignment",
        )
        view = borrow(q, offset, width, q.phase, writable=True)
        require_writes((view,), ((view, 0, width),), context.protected)
        local.append((at, offset, offset + width))
    require(covered == code, "validator local-effects omit mapped current instructions")
    require(
        len(local) == 128 and len(pending) == 56,
        "validator fixed/pointer store family inventory differs",
    )

    def writers(start, end):
        return tuple((at, lo, hi) for at, lo, hi in local if lo < end and start < hi)

    spill = writers(60, 64)
    capture = writers(88, 92)
    valid = writers(95, 96)
    require(
        spill == ((parent + 86, 60, 64),) and capture == ((parent + 30, 88, 92),),
        "validator overwrites its exclusive Result/captured-first pointer cells",
    )
    require(
        valid == ((parent + 32, 95, 96), (parent + 4876, 95, 96)),
        "validator direct valid-byte producers differ",
    )
    return dict(
        boundary=boundary,
        local_ranges=tuple(local),
        spill_writers=spill,
        capture_writers=capture,
        valid_writers=valid,
        local_writes=tuple(
            (borrow(q, lo, hi - lo, q.phase, writable=True), 0, hi - lo) for at, lo, hi in local
        ),
        unresolved_pointer_writes=tuple(pending),
        code_bytes=len(code),
        mapped_data_bytes=extent - len(code),
        natural_local_alignment=True,
        current_body_has_no_other_SP_R7_write=True,
        requires_all_child_SP_R7_and_pointer_effects=True,
        requires_whole_parent_control_entry_and_source_domains=True,
        requires_first_typed_constructors_and_valid_boolean_producers=True,
        platform_requirements_unresolved=True,
    )
