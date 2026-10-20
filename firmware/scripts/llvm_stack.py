"""Bounded selected-toolchain LLVM decoding; no target execution or PATH fallback."""

from collections.abc import Iterable, Sequence
from dataclasses import dataclass
import hashlib
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import tempfile
from typing import TypedDict

from elf_image import ElfImage, MappedImage
from validation import require

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "host/scripts"))
import owned_process

SCHEMA = 2
TARGET = "thumbv8m.main-none-eabihf"
DECODER_ARGS = ("--disassemble", "--disassemble-zeroes", "--triple=" + TARGET, "--mcpu=cortex-m33")


@dataclass(frozen=True)
class Instruction:
    address: int
    raw: bytes
    mnemonic: str
    operands: str


def capability_elf() -> bytes:
    """Literal reviewed Thumb fixture, including branch-looking mapped data.

    BL tail; BNE; IT/POPEQ; VPUSH/VPOP 64 B; VCMP/VMRS; BLX R3;
    BX LR; NOP; literal pool; zero-growth tail branch and BX LR.
    This synthetic ELF is decoded only and never executed.
    """
    code = bytes.fromhex(
        "00f010f8 00d1 08bf 10bd 2ded108b bdec108b b4ee408a f1ee10fa 9847 7047 00bf 00f000f8 ece7 7047"
    )
    names = b"\0probe\0tail\0$t\0$d\0"
    symbols = bytes(16)
    for name, value, size, info in [
        (1, 0x10000001, 32, 18),
        (7, 0x10000025, 4, 18),
        (12, 0x10000000, 0, 0),
        (15, 0x10000020, 0, 0),
        (12, 0x10000024, 0, 0),
    ]:
        symbols += struct.pack("<IIIBBH", name, value, size, info, 0, 1)
    section_names = b"\0.text\0.symtab\0.strtab\0.shstrtab\0"
    blob = bytearray(256) + code
    while len(blob) % 4:
        blob.append(0)
    symbol_offset = len(blob)
    blob.extend(symbols)
    names_offset = len(blob)
    blob.extend(names)
    section_names_offset = len(blob)
    blob.extend(section_names)
    while len(blob) % 4:
        blob.append(0)
    table_offset = len(blob)
    sections = [
        (0,) * 10,
        (1, 1, 6, 0x10000000, 256, len(code), 0, 0, 2, 0),
        (7, 2, 0, 0, symbol_offset, len(symbols), 3, 1, 4, 16),
        (15, 3, 0, 0, names_offset, len(names), 0, 0, 1, 0),
        (23, 3, 0, 0, section_names_offset, len(section_names), 0, 0, 1, 0),
    ]
    for section in sections:
        blob.extend(struct.pack("<10I", *section))
    blob[:52] = struct.pack(
        "<16sHHIIIIIHHHHHH",
        b"\x7fELF\x01\x01\x01" + bytes(9),
        2,
        40,
        1,
        0x10000001,
        52,
        table_offset,
        0x05000000,
        52,
        32,
        1,
        40,
        5,
        4,
    )
    blob[52:84] = struct.pack("<8I", 1, 256, 0x10000000, 0x10000000, len(code), len(code), 5, 2)
    return bytes(blob)


def mapping_ranges(elf: MappedImage, name: str) -> list[tuple[int, int, str]]:
    address, length, section_index = elf.functions[name]
    address &= ~1
    section = elf.sections[section_index]
    require(section[2] & 4, "function is outside executable section")
    entries: dict[int, str] = {}
    for label, value, index in elf.mapping_symbols:
        if index != section_index:
            continue
        kind = label[1]
        require(kind in ("t", "d"), "ARM mapping on Thumb target")
        require(
            section[3] <= value < section[3] + section[5] and value % 2 == 0,
            "invalid mapping address",
        )
        require(value not in entries or entries[value] == kind, "conflicting code/data mappings")
        entries[value] = kind
    require(entries and min(entries) <= address, "missing function code/data mapping")
    require(address + length <= section[3] + section[5], "function exceeds mapped section")
    points = sorted(entries)
    result = []
    for start, end in zip(points, points[1:] + [section[3] + section[5]]):
        lo, hi = max(start, address), min(end, address + length)
        if lo < hi:
            result.append((lo, hi, entries[start]))
    require(
        result and result[0][0] == address and result[-1][1] == address + length,
        "incomplete function mapping",
    )
    require(result[0][2] == "t", "function entry is mapped data")
    return result


def decode_listing(
    text: str, elf: MappedImage, names: Iterable[str] | None = None
) -> dict[int, Instruction]:
    """Check raw bytes and complete mapped code coverage before using LLVM text."""
    selected = list(elf.functions if names is None else names)
    ranges = []
    raw: dict[int, int] = {}
    extents: dict[int, tuple[int, int]] = {}
    for name in selected:
        start = elf.functions[name][0] & ~1
        extent = (elf.functions[name][1], elf.functions[name][2])
        require(start not in extents or extents[start] == extent, "conflicting function aliases")
        if start not in extents:
            require(
                all(start + extent[0] <= a or a + v[0] <= start for a, v in extents.items()),
                "ambiguous overlapping function ownership",
            )
            extents[start] = extent
        data = elf.function_bytes(name)
        for i, byte in enumerate(data):
            require(start + i not in raw or raw[start + i] == byte, "conflicting function extents")
            raw[start + i] = byte
        ranges.extend(mapping_ranges(elf, name))
    code = {address for start, end, kind in ranges if kind == "t" for address in range(start, end)}
    records = {}
    covered: set[int] = set()
    for line in text.splitlines():
        prefix = re.match(r"^\s*([0-9a-fA-F]+):\s*(.*)$", line)
        if not prefix:
            continue
        address = int(prefix[1], 16)
        if address not in code:
            continue
        match = re.fullmatch(
            r"((?:(?:[0-9a-fA-F]{4}|[0-9a-fA-F]{2})\s+)+)(\S+)(?:\s+(.*))?", prefix[2]
        )
        if match is None:
            raise AssertionError("invalid LLVM instruction record")
        groups = match[1].split()
        require(all(len(g) == 4 for g in groups), "unexpected instruction byte grammar")
        data = b"".join(int(g, 16).to_bytes(2, "little") for g in groups)
        require(len(data) in (2, 4) and address % 2 == 0, "invalid Thumb instruction width")
        addresses = set(range(address, address + len(data)))
        require(
            addresses <= code and not addresses & covered,
            "overlapping or crossed instruction mapping",
        )
        require(
            data == bytes(raw[a] for a in range(address, address + len(data))),
            "LLVM bytes differ from ELF",
        )
        mnemonic = match[2].lower()
        require(
            mnemonic not in ("<unknown>", ".inst", ".word", ".short", ".byte"),
            "undecoded mapped instruction",
        )
        records[address] = Instruction(
            address, data, mnemonic, (match[3] or "").split("@", 1)[0].strip()
        )
        covered.update(addresses)
    require(covered == code, "incomplete LLVM instruction coverage")
    return records


def _digest(path: str | Path) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def _supervisor_digest() -> str:
    return _digest(owned_process.__file__)


def _run(argv: Sequence[str], limit: int, timeout: float = 60) -> str:
    """Bound both output streams and own every descendant of the selected tool.

    The shared supervisor limits execution/draining to timeout, then allows up
    to ten seconds on Windows or 5.2 seconds on POSIX for owned-tree cleanup. The
    child receives only the fixed minimal environment. No shell or PATH fallback
    is added by this adapter.
    """
    env = {
        k: v for k, v in os.environ.items() if k.upper() in ("SYSTEMROOT", "WINDIR", "TEMP", "TMP")
    }
    env.update(LC_ALL="C", LANG="C")
    try:
        result = owned_process.run_bounded(
            argv,
            env=env,
            timeout=timeout,
            stdout_limit=limit,
            stderr_limit=limit,
            on_stderr=lambda _: None,
        )
    except subprocess.TimeoutExpired as error:
        raise AssertionError("LLVM child/drain deadline exceeded") from error
    except owned_process.OutputLimitError as error:
        raise AssertionError("LLVM output bound exceeded") from error
    require(
        result.returncode == 0,
        "LLVM child failed: " + result.stderr.decode("utf-8", errors="replace")[:512],
    )
    require(not result.stderr, "LLVM diagnostic output is not permitted")
    return result.stdout.decode("utf-8", errors="strict").replace("\r\n", "\n")


class LlvmContextRecord(TypedDict):
    schema: int
    compiler_path: str
    compiler_sha256: str
    compiler_version: str
    sysroot: str
    host: str
    executable: str
    executable_sha256: str
    version: str
    target: str
    arguments: tuple[str, ...]
    capability_sha256: str
    capability_output_sha256: str
    adapter_sha256: str
    supervisor_sha256: str


class DisassemblyEvidence(TypedDict):
    context: LlvmContextRecord
    command: list[str]
    elf_sha256: str
    output_sha256: str
    covered_instructions: int


@dataclass(frozen=True)
class LlvmContext:
    schema: int
    compiler_path: str
    compiler_sha256: str
    compiler_version: str
    sysroot: str
    host: str
    executable: str
    executable_sha256: str
    version: str
    target: str
    arguments: tuple[str, ...]
    capability_sha256: str
    capability_output_sha256: str
    adapter_sha256: str
    supervisor_sha256: str

    def to_record(self) -> LlvmContextRecord:
        return {
            "schema": self.schema,
            "compiler_path": self.compiler_path,
            "compiler_sha256": self.compiler_sha256,
            "compiler_version": self.compiler_version,
            "sysroot": self.sysroot,
            "host": self.host,
            "executable": self.executable,
            "executable_sha256": self.executable_sha256,
            "version": self.version,
            "target": self.target,
            "arguments": self.arguments,
            "capability_sha256": self.capability_sha256,
            "capability_output_sha256": self.capability_output_sha256,
            "adapter_sha256": self.adapter_sha256,
            "supervisor_sha256": self.supervisor_sha256,
        }


def resolve(rustc: str | Path, version: str) -> LlvmContext:
    adapter_hash = _digest(__file__)
    supervisor_hash = _supervisor_digest()
    compiler = Path(rustc)
    require(compiler.is_absolute() and compiler.is_file(), "missing absolute selected compiler")
    compiler = compiler.resolve()
    compiler_hash = _digest(compiler)
    require(_run([str(compiler), "-Vv"], 16384) == version, "selected compiler identity changed")
    host = re.search(r"^host: ([a-z0-9_-]+)$", version, re.M)
    if host is None or "LLVM version: 23.1.3" not in version:
        raise AssertionError("unreviewed compiler/LLVM identity")
    sysroot = Path(_run([str(compiler), "--print", "sysroot"], 16384).strip()).resolve()
    require(compiler.parent == sysroot / "bin", "compiler is outside selected sysroot")
    tool = (
        sysroot
        / "lib/rustlib"
        / host[1]
        / "bin"
        / ("llvm-objdump.exe" if os.name == "nt" else "llvm-objdump")
    ).resolve()
    require(
        tool.is_file() and tool.is_relative_to(sysroot), "selected compiler LLVM tool unavailable"
    )
    tool_hash = _digest(tool)
    tool_version = _run([str(tool), "--version"], 32768)
    require("LLVM version 23.1.3" in tool_version, "unreviewed LLVM decoder version")
    probe = capability_elf()
    with tempfile.TemporaryDirectory(prefix="dashboard-llvm-probe-") as folder:
        path = Path(folder) / "probe.elf"
        path.write_bytes(probe)
        output = _run([str(tool), *DECODER_ARGS, str(path)], 65536)
        decoded = decode_listing(output, ElfImage(path))
    expected = {
        0: "bl",
        4: "bne",
        6: "it",
        8: "popeq",
        10: "vpush",
        14: "vpop",
        18: "vcmp.f32",
        22: "vmrs",
        26: "blx",
        28: "bx",
        30: "nop",
        36: "b",
        38: "bx",
    }
    require(
        {a - 0x10000000: i.mnemonic for a, i in decoded.items()} == expected,
        "LLVM capability semantics differ",
    )
    require(
        decoded[0x10000000].operands.startswith("0x10000024")
        and decoded[0x10000024].operands.startswith("0x10000000"),
        "LLVM capability targets differ",
    )
    require(
        _digest(tool) == tool_hash
        and _digest(compiler) == compiler_hash
        and _digest(__file__) == adapter_hash
        and _supervisor_digest() == supervisor_hash,
        "decoder/compiler changed during probe",
    )
    return LlvmContext(
        SCHEMA,
        str(compiler),
        compiler_hash,
        version,
        str(sysroot),
        host[1],
        str(tool),
        tool_hash,
        tool_version,
        TARGET,
        DECODER_ARGS,
        hashlib.sha256(probe).hexdigest(),
        hashlib.sha256(output.encode()).hexdigest(),
        adapter_hash,
        supervisor_hash,
    )


def disassemble(
    path: str | Path, elf: ElfImage, context: LlvmContext
) -> tuple[dict[int, Instruction], DisassemblyEvidence]:
    require(
        isinstance(context, LlvmContext)
        and context.schema == SCHEMA
        and context.arguments == DECODER_ARGS,
        "missing or unsupported typed LLVM context",
    )
    require(
        _digest(context.executable) == context.executable_sha256
        and _digest(context.compiler_path) == context.compiler_sha256
        and _digest(__file__) == context.adapter_sha256
        and _supervisor_digest() == context.supervisor_sha256,
        "LLVM context identity changed",
    )
    source = Path(path).resolve()
    require(source.read_bytes() == elf.data, "decoder input differs from immutable ELF")
    command = [context.executable, *context.arguments, str(source)]
    output = _run(command, min(64 * 1024 * 1024, max(1024 * 1024, len(elf.data) * 32)))
    instructions = decode_listing(output, elf)
    require(
        source.read_bytes() == elf.data
        and _digest(context.executable) == context.executable_sha256
        and _digest(context.compiler_path) == context.compiler_sha256
        and _supervisor_digest() == context.supervisor_sha256,
        "ELF/decoder/compiler changed during admission",
    )
    evidence: DisassemblyEvidence = dict(
        context=context.to_record(),
        command=command,
        elf_sha256=hashlib.sha256(elf.data).hexdigest(),
        output_sha256=hashlib.sha256(output.encode()).hexdigest(),
        covered_instructions=len(instructions),
    )
    return instructions, evidence
