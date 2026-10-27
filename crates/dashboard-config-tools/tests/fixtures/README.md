# Frozen project fixtures

`v1-landscape.eavp` was exported by commit
`91805422ae1ba3ee4b2f34dccd027cd0cd039b59`, using the default v4 dashboard and
default v1 startup profile without artwork. It tests migration of an actual
historical container rather than a file regenerated using the current schema.

SHA-256: `3a0f33a2f08834b149aa3a5b42770de7acd14aeff6856cc16e2fda3d46dbfa63`.

`v2-schema6-artwork.eavp` was frozen on 27 October 2026 using unchanged
production encoders at `f0ad9035e12d6c1b1011004c5927434577475460`, with
`nightly-2026-10-18`. A temporary typed test called `Project::new` and
`encode(ExportMode::Complete)`, immediately decoded and compared every component,
then was removed. Keep these bytes unchanged through schema updates.

Its dashboard is exactly [v6-mixed.eav](../../../dashboard-config/tests/fixtures/README.md),
with eight views, 64 widgets, every historical scalar channel/readout and disabled
synthetic BLE settings. Startup selects Custom, 7134 ms duration, 913 ms status,
no loop, attention interruption, no warning LED, PortraitInverted status and
Landscape splash. Remaining startup values are the frozen defaults.

Artwork reuses `project.rs::artwork()`: one 100 ms landscape red RGB565 frame,
its conversion recipe and the hash-bound synthetic PNG-header source bytes.
The source tests identity preservation, **not PNG decoding**. The complete
fixture is 9985 bytes; its project framing is v2 and dashboard schema is 6.

SHA-256: `5e79ce126ff0a9cd4b9b1cb00ec6b1056ee3a6feb3034ea7d5468c02c9e1fb2e`.

`frozen_schema_six_project_preserves_complete_content` checks that digest,
dashboard content, profile, converted bytes, recipe, original bytes, all four
non-dashboard sections and canonical complete re-encoding after import. The
configuration crate separately compares every frozen dashboard field.
