# Gauge raster fixture

`gauge-markers.rle` is the lossless RGB565 output of
`advanced_gauge_markers_use_prepared_raw_peaks_and_static_bands_in_every_span`.
It concatenates each cropped widget in `GaugeKind::ALL`, then 1x1/2x1/2x2,
then valid/unavailable order. Each run is a nonzero little-endian u16 pixel
count followed by one big-endian RGB565 pixel. Runs restart at each widget.

The test checks independent band/peak visibility, clipping and retained/full
parity before comparing the fixture. Intentional raster changes can be
regenerated with `BLESS_GAUGE_GOLDENS=1`. Set `GAUGE_EVIDENCE_DIR` to export
the same cropped cells as PPMs for visual review; review these before accepting
a regenerated fixture. Normal runs never modify this file.
