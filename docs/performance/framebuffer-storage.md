# Fixed-size framebuffer storage

The renderer now accepts a complete `FrameBuffer`, aligned to four bytes by its
type. Clear and row fills use checked safe slices; no renderer write depends on
raw pointer alignment. Host tests import the production module directly, without
source slicing in a build script. The storage is intentionally not Copy or Clone.

Tests cover all RGB565 channel byte orders, odd row boundaries and neighbours,
200 deterministic clipped/truncated color streams against an independent pixel
reference, and unrepresentable rectangle rejection before embedded-graphics can
panic. Existing UI pixel contracts remain in the suite. These are host checks.

## Native geometry

`St7789Renderer::with_geometry` binds the existing storage to 320 × 240 or
240 × 320 logical pixels; `new` remains the landscape convenience constructor.
Rows use that geometry's 640- or 480-byte stride. Both framebuffers remain exactly
153600 bytes, with no third buffer or transpose. Rendering, custom artwork rows
and damage plans must use the same geometry. [Orientation](../configurator/orientation.md)
and the [hardware runbook](../hardware/display-orientation.md) describe the contract.

## Historical paired host measurements

Same machine, nightly and four existing Criterion fixtures, 30 samples each.
Raw JSON is saved in `host/baselines/stage04_storage_before` and
`host/baselines/stage04_storage_safe`. Before source: `0c63321`.

| Operation | Before median | Safe storage median | Observed change |
| --- | ---: | ---: | ---: |
| Clear full frame | 2.463 us | 2.366 us | -3.9% |
| Clipped contiguous fill | 8.400 us | 6.585 us | -21.6% |
| Boost cell | 7.857 us | 8.422 us | +7.2% |
| Complete graph | 3.038 us | 2.956 us | -2.7% |

These single paired runs include code generation and host scheduling variation.
The boost cost is tracked as a possible regression, not concealed by the clipped
fill gain. Neither result predicts RP2350 cycles or physical FPS. Stock release
compilation and strict host/firmware Clippy pass. Transfer ownership and boot
borrows are changed in the next reviewable slice; alignment alone does not close
that finding.
