# Artwork readiness benchmark

Run from `configurator/`:

```powershell
cargo +nightly-2026-10-18 bench --locked --bench artwork_readiness
```

This measures accepted-editor readiness and save-status queries, excluding admission,
conversion, serialization and rendering. It prints seven raw batches for no artwork,
a small GIF and a maximum valid converted asset. Preserve the compiler, input hashes
and options for comparisons. Host times do not establish RP2350 timing. Submicrosecond
batches are timer-resolution limited; report them as such rather than a precise ratio.

`fixtures/capacity-sprite.png` is an intentional generated 640x480 RGB source with a
2x2 grid of 320x240 frames, each lasting 100 ms. It contains no private data. Each pixel
starts as RGB565 `(x*67 + y*313 + frame*9001) mod 65536`, expanded to RGB8 using rounded
channel scaling. In frame 3, rows after 97 are constant across each row; row 97 uses
x=264 for all x >= 264. This gives three raw frames and one RLE frame totaling exactly
524288 converted EASA bytes, without padding. The shared converter and Artwork
constructor validate the result before measurement. Expected converted SHA256:
`6e8cc35a33e7000ec3ccacf8ecbe31a830d1dffd70353879c39ebd80b4420e0b`.

