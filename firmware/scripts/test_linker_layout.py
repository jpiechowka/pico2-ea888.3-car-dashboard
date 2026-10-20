"""Link synthetic ARM objects through both actual memory maps, without a device."""

from pathlib import Path
import shutil
import sys
import tempfile
import unittest

from elf_image import ElfImage

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "host/scripts"))
from owned_process import run_bounded

REPO = Path(__file__).resolve().parents[2]


class LinkerLayout(unittest.TestCase):
    rustc: str
    linker: Path

    @classmethod
    def setUpClass(cls) -> None:
        rustc = shutil.which("rustc")
        if rustc is None:
            raise RuntimeError("the pinned Rust toolchain and ARM target are required")
        cls.rustc = rustc
        root = cls.command([rustc, "--print", "sysroot"]).strip()
        version = cls.command([rustc, "-vV"])
        host = next(
            line.removeprefix("host: ")
            for line in version.splitlines()
            if line.startswith("host: ")
        )
        cls.linker = (
            Path(root)
            / "lib/rustlib"
            / host
            / "bin"
            / ("rust-lld.exe" if sys.platform == "win32" else "rust-lld")
        )

    @staticmethod
    def command(args: list[str], cwd: Path | None = None) -> str:
        result = run_bounded(
            args, cwd=cwd or REPO, timeout=60, stdout_limit=65536, stderr_limit=65536
        )
        if result.returncode:
            raise AssertionError(result.stderr.decode(errors="replace"))
        return result.stdout.decode("utf-8")

    def link(
        self,
        package: str,
        power: int,
        folder: Path,
        scratch_sizes: tuple[int, int] = (4096, 4096),
    ) -> ElfImage:
        assembly = rf"""
#![no_std]
core::arch::global_asm!(r#"
.syntax unified
.thumb
.section .vector_table,"a",%progbits
.word 0
.section .start_block,"a",%progbits
.word 0
.section .text.fixture,"ax",%progbits
.p2align {power}
.global fixture
.type fixture,%function
.thumb_func
fixture:
bx lr
.section .dashboard_config,"a",%progbits
.word 1
.section .startup_profile,"a",%progbits
.space 16
.section .startup_asset,"a",%progbits
.byte 0
.section .uninit,"aw",%nobits
.space 8
.section .scratch8,"aw",%nobits
.space {scratch_sizes[0]}
.section .scratch9,"aw",%nobits
.space {scratch_sizes[1]}
"#);
"""
        if package == "ble-probe":
            start = assembly.index(".section .dashboard_config")
            end = assembly.index(".section .uninit")
            assembly = assembly[:start] + assembly[end:]
        (folder / "fixture.rs").write_text(assembly, encoding="utf-8")
        shutil.copyfile(REPO / f"firmware/{package}/memory.x", folder / "memory.x")
        (folder / "base.x").write_text(
            """
INCLUDE memory.x
ENTRY(fixture)
SECTIONS {
  .vector_table : { KEEP(*(.vector_table)); } > FLASH
  .text _stext : { *(.text*); } > FLASH
  .rodata : { *(.rodata*); } > FLASH
  .data : { *(.data*); } > RAM AT> FLASH
  .bss (NOLOAD) : { *(.bss*); } > RAM
  .uninit (NOLOAD) : { *(.uninit*); __euninit = .; } > RAM
  .scratch8 (NOLOAD) : { *(.scratch8); } > SRAM8
  .scratch9 (NOLOAD) : { *(.scratch9); } > SRAM9
}
_stack_start = ORIGIN(RAM) + LENGTH(RAM);
ASSERT(ADDR(.text) % ALIGNOF(.text) == 0, "text alignment regression");
ASSERT(ADDR(.text) == _stext, "text symbol regression");
""",
            encoding="utf-8",
        )
        self.command(
            [
                self.rustc,
                "--crate-type=lib",
                "--emit=obj",
                "--target=thumbv8m.main-none-eabihf",
                "-Ctarget-cpu=cortex-m33",
                "-o",
                "fixture.o",
                "fixture.rs",
            ],
            folder,
        )
        self.command(
            [str(self.linker), "-flavor", "gnu", "-T", "base.x", "-o", "fixture.elf", "fixture.o"],
            folder,
        )
        return ElfImage(folder / "fixture.elf")

    def test_text_alignment_and_protected_flash_are_preserved(self) -> None:
        for package, end in (("pico2", 0x101FE000), ("ble-probe", 0x101E6000)):
            for power in (4, 5, 8):
                with (
                    self.subTest(package=package, alignment=1 << power),
                    tempfile.TemporaryDirectory() as tmp,
                ):
                    image = self.link(package, power, Path(tmp))
                    text = image.named[".text"]
                    self.assertEqual(text[3] % (1 << power), 0)
                    boot = image.named[".start_block"]
                    self.assertGreaterEqual(text[3], boot[3] + boot[5])
                    self.assertLess(boot[3] + boot[5], 0x10001000)
                    self.assertEqual(image.symbols["__settings_start"], 0x101FE000)
                    self.assertEqual(image.symbols["__settings_end"], 0x10200000)
                    self.assertGreaterEqual(image.symbols["__sheap"], 0x20000000)
                    self.assertLessEqual(image.symbols["__sheap"], 0x20080000 - 32768)
                    self.assertEqual(image.symbols["_stack_start"], 0x20080000)
                    for name, origin in ((".scratch8", 0x20080000), (".scratch9", 0x20081000)):
                        section = image.named[name]
                        self.assertEqual(section[3], origin)
                        self.assertEqual(section[5], 4096)
                    if package == "ble-probe":
                        self.assertEqual(image.symbols["__captures_start"], end)
                        self.assertEqual(image.symbols["__captures_end"], 0x101FE000)
                    for segment in image.segments:
                        kind, _, _, physical, size, _, _, _ = segment
                        if kind == 1 and size and physical >= 0x10000000 and physical < 0x20000000:
                            self.assertLessEqual(physical + size, end)

    def test_each_scratch_bank_rejects_overflow(self) -> None:
        for package in ("pico2", "ble-probe"):
            for bank, sizes in (("SRAM8", (4097, 4096)), ("SRAM9", (4096, 4097))):
                with self.subTest(package=package, bank=bank), tempfile.TemporaryDirectory() as tmp:
                    with self.assertRaisesRegex(AssertionError, "overflowed by 1 bytes") as error:
                        self.link(package, 4, Path(tmp), sizes)
                    self.assertIn(f"region '{bank}'", str(error.exception))


if __name__ == "__main__":
    unittest.main()
