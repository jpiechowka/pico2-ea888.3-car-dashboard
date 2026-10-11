# Cooperative input and rendering

Rendering yields after the framebuffer clear and between dashboard cells.
Profiling and log pages also yield between rows. The log page copies at most
13 entries under its mutex and releases it before formatting, drawing or
awaiting. Core 1 never has to wait for a screen raster operation while logging.

`embassy-futures` 0.1.2 `yield_now` wakes its task and returns Pending on its
first poll, then Ready on its second. The exact locked source was checked;
this is an executor checkpoint, with no minimum sleep or real-time deadline.
Device traces are needed to establish the longest uninterrupted drawing slice
and input-to-visible latency. The integrated no-damage path waits on a 16 ms
timer so an unchanged scene yields execution without a busy loop.

`CycleSegments` pauses DWT accounting before yielding and resumes afterward.
This excludes other tasks' work while suspended, but includes interrupt work
inside active segments. Both cores report work estimates over actual elapsed
time, labelled UI work and acquisition work rather than total CPU utilization.
Malformed transitions, counter wrap, segment length and accumulation bounds
are checked. Host tests verify those arithmetic contracts independently of
the target DWT peripheral.

The input producer has its own watchdog progress counter. Bounded event
accumulation, reconnects and debounce are covered in
[encoder recovery](../hardware/encoder-recovery.md). Sampled SP telemetry and
the new linked-image RAM acceptance gate are described in
[memory layout](memory-layout.md).

Historical verification for this slice: 86 runtime host tests plus two ownership
compile-fail tests pass in default and full-outline modes, with strict host
and firmware Clippy, seven benchmark smoke fixtures, and stock/trace/300 MHz
builds. These checks establish source and host contracts only. They do not
establish physical input timing, DMA overlap, FPS, stack high-water marks or
vehicle compatibility.
