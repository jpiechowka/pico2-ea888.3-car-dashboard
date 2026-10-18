"""Package shared-renderer PPM frames as lossless PNGs and looping GIFs.

First run the startup_gallery Rust example with --animation. Pillow is a host
documentation dependency only; Pico firmware draws these scenes directly.
"""

import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image, __version__ as pillow_version


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def animation(frames, output):
    # Preserve every renderer colour. Refuse lossy quantization if art grows
    # beyond GIF's palette, rather than silently degrading the published image.
    colours = sorted({pixel for frame in frames for pixel in frame.get_flattened_data()})
    if len(colours) > 256:
        raise ValueError(f"{output.name}: {len(colours)} colours exceed lossless GIF palette")
    lookup = {colour: index for index, colour in enumerate(colours)}
    palette = [channel for colour in colours for channel in colour]
    palette += [0] * (768 - len(palette))
    indexed = []
    for frame in frames:
        item = Image.frombytes(
            "P", frame.size, bytes(lookup[pixel] for pixel in frame.get_flattened_data())
        )
        item.putpalette(palette)
        indexed.append(item)
    indexed[0].save(
        output,
        save_all=True,
        append_images=indexed[1:],
        duration=50,
        loop=0,
        optimize=False,
        disposal=2,
    )
    # Encoders can coalesce identical frames. Verify decoded pixels and the
    # complete two-second timeline, including those merged intervals.
    with Image.open(output) as decoded:
        if decoded.info.get("loop") != 0:
            raise ValueError(f"{output.name}: loop flag lost")
        sample = 0
        for index in range(decoded.n_frames):
            decoded.seek(index)
            duration = decoded.info["duration"]
            if duration <= 0 or duration % 50:
                raise ValueError(f"{output.name}: unexpected frame duration {duration}")
            pixels = decoded.convert("RGB").tobytes()
            for _ in range(duration // 50):
                if sample >= len(frames) or pixels != frames[sample].tobytes():
                    raise ValueError(f"{output.name}: decoded frame {sample} differs")
                sample += 1
        if sample != len(frames):
            raise ValueError(f"{output.name}: incomplete timeline {sample}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="startup_gallery --animation output")
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    source, destination = args.source.resolve(), args.destination.resolve()
    destination.mkdir(parents=True, exist_ok=True)
    names = sorted(
        path.name.removesuffix("-Landscape-1200.ppm")
        for path in source.glob("*-Landscape-1200.ppm")
    )
    if not names:
        raise ValueError("no landscape frame exports found")
    records = []
    for name in names:
        for geometry, size, suffix in [
            ("Landscape", (320, 240), ""),
            ("Portrait", (240, 320), "-portrait"),
        ]:
            stem = f"{name}{suffix}"
            still_source = source / f"{name}-{geometry}-1200.ppm"
            with Image.open(still_source) as image:
                still = image.convert("RGB")
            if still.size != size:
                raise ValueError(f"{still_source}: wrong geometry")
            png = destination / f"{stem}.png"
            still.save(png, optimize=True, compress_level=9)
            with Image.open(png) as decoded:
                if decoded.convert("RGB").tobytes() != still.tobytes():
                    raise ValueError(f"{png.name}: PNG pixels differ")
            frames = []
            for time in range(0, 2000, 50):
                path = source / "animation" / f"{name}-{geometry}-{time:04}.ppm"
                with Image.open(path) as image:
                    frame = image.convert("RGB")
                if frame.size != size:
                    raise ValueError(f"{path}: wrong geometry")
                frames.append(frame)
            gif = destination / f"{stem}.gif"
            animation(frames, gif)
            records.append(
                {
                    "theme": name,
                    "geometry": geometry,
                    "png_sha256": digest(png),
                    "gif_sha256": digest(gif),
                }
            )
            print(f"Verified {stem}: 40 frames, 2000 ms, {gif.stat().st_size} bytes")
    repo = Path(__file__).resolve().parents[2]
    manifest = {
        "pillow": pillow_version,
        "sample_ms": 50,
        "cycle_ms": 2000,
        "still_ms": 1200,
        "lossless_rgb_verified": True,
        "source_hash_line_endings": "LF",
        "source_sha256": {
            path: hashlib.sha256((repo / path).read_text(encoding="utf-8").encode()).hexdigest()
            for path in [
                "crates/dashboard-core/src/startup/scene.rs",
                "crates/dashboard-config/src/startup.rs",
                "host/examples/startup_gallery.rs",
            ]
        },
        "captures": records,
    }
    (destination / "capture.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
