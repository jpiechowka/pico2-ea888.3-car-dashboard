"""Exercise real Pillow packaging without compiling Rust or touching hardware."""

from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from PIL import GifImagePlugin, Image

import export_startup_gallery as gallery


class StartupGallery(unittest.TestCase):
    def test_real_gif_preserves_pixels_geometry_and_merged_timeline(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "timeline.gif"
            red = Image.new("RGB", (12, 8), (255, 0, 0))
            blue = Image.new("RGB", (12, 8), (0, 0, 255))
            frames = [red] * 20 + [blue] * 20
            gallery.animation(frames, output)
            with Image.open(output) as decoded:
                self.assertIsInstance(decoded, GifImagePlugin.GifImageFile)
                if not isinstance(decoded, GifImagePlugin.GifImageFile):
                    self.fail("GIF decoder required")
                self.assertEqual(decoded.size, (12, 8))
                self.assertEqual(decoded.info["loop"], 0)
                self.assertLess(decoded.n_frames, len(frames))
                timeline = 0
                for index in range(decoded.n_frames):
                    decoded.seek(index)
                    duration: object = decoded.info["duration"]
                    if type(duration) is not int:
                        self.fail("integer GIF duration required")
                    self.assertEqual(duration % 50, 0)
                    self.assertEqual(
                        decoded.convert("RGB").tobytes(), frames[timeline // 50].tobytes()
                    )
                    timeline += duration
                self.assertEqual(timeline, 2000)

    def test_invalid_frame_collections_fail_before_writing(self) -> None:
        malformed: list[list[Image.Image]] = [
            [],
            [Image.new("RGB", (2, 2)), Image.new("RGB", (3, 2))],
            [Image.new("L", (2, 2))],
            [Image.new("RGBA", (2, 2))],
        ]
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "invalid.gif"
            for frames in malformed:
                with self.subTest(frames=frames), self.assertRaises(ValueError):
                    gallery.animation(frames, output)
                self.assertFalse(output.exists())

    def test_palette_overflow_is_rejected_without_quantization(self) -> None:
        frame = Image.new("RGB", (257, 1))
        frame.putdata([(value % 256, value // 256, 0) for value in range(257)])
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "overflow.gif"
            with self.assertRaisesRegex(ValueError, "257 colours"):
                gallery.animation([frame], output)
            self.assertFalse(output.exists())

    def test_complete_export_reopens_real_png_gif_and_records_hashes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, destination = root / "source", root / "published"
            (source / "animation").mkdir(parents=True)
            for geometry, size in (("Landscape", (320, 240)), ("Portrait", (240, 320))):
                with Image.new("RGB", size, (32, 64, 128)) as frame:
                    frame.save(source / f"fixture-{geometry}-1200.ppm")
                    for time in range(0, 2000, 50):
                        frame.save(source / "animation" / f"fixture-{geometry}-{time:04}.ppm")
            with (
                patch.object(sys, "argv", ["gallery", str(source), str(destination)]),
                redirect_stdout(io.StringIO()),
            ):
                gallery.main()
            manifest: object = json.loads((destination / "capture.json").read_text())
            self.assertIsInstance(manifest, dict)
            if not isinstance(manifest, dict):
                self.fail("manifest object required")
            self.assertEqual(manifest["sample_ms"], 50)
            self.assertEqual(manifest["cycle_ms"], 2000)
            self.assertIs(manifest["lossless_rgb_verified"], True)
            self.assertEqual(
                manifest["captures"],
                [
                    {
                        "theme": "fixture",
                        "geometry": geometry,
                        "png_sha256": gallery.digest(destination / f"fixture{suffix}.png"),
                        "gif_sha256": gallery.digest(destination / f"fixture{suffix}.gif"),
                    }
                    for geometry, suffix in (("Landscape", ""), ("Portrait", "-portrait"))
                ],
            )
            for suffix, size in (("", (320, 240)), ("-portrait", (240, 320))):
                for extension in ("png", "gif"):
                    with Image.open(destination / f"fixture{suffix}.{extension}") as decoded:
                        self.assertEqual(decoded.size, size)
                        self.assertEqual(decoded.convert("RGB").getpixel((0, 0)), (32, 64, 128))


if __name__ == "__main__":
    unittest.main()
