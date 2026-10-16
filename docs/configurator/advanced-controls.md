# Units, history and warning controls

These Studio controls expose the advanced fields in the current v5 configuration. Firmware and browser
share the advanced runtime; exports use the normal strict configuration codec.
See [historical v4 release evidence](advanced-v4-release.md) and
[current orientation controls](orientation.md).

Select a widget and open **Units & current value**. Temperatures offer Celsius
and Fahrenheit. Boost offers **Follow dashboard bar / PSI button** or fixed bar,
PSI and kPa. Only Follow dashboard responds to the physical or preview toggle.
An equal primary and secondary unit appears once. Lambda offers lambda,
estimated AFR, or both. AFR uses the declared Fuel & AFR stoichiometry, never a
measured blend or an ECU requested AFR. Numeric widgets support secondary lines
at every size; gauges need 2×1 or 2×2 for a companion reading.

Thresholds, scale endpoints, bands, extrema, history and references remain
canonical: Celsius, relative bar, volts or lambda. Conversion occurs at display
formatting and geometry. Changing units preserves thresholds. **Current-value
smoothing** offers Off, Gentle (250 ms), Strong (750 ms), or 50–5000 ms. The
current number and gauge share the prepared reading. Recorded history,
statistics, peaks and warning rules remain unsmoothed.

**Channel-shared history** offers 10, 30, 120 and 300 second presets or an integer
10–900 seconds. Every associated widget uses the same duration. Changing it
clears that channel's history while preserving independent extrema. Sixty timed
buckets take the last qualified sample in each. Empty buckets remain gaps;
values are never backfilled. A fault can remove the prior point to make a brief
outage visible. This is a trend view, not a lossless waveform or peak recorder.
Duration remains visible while filling. Gauges replace history but retain its
settings for numeric widgets.

Gauges offer Off, Maximum or Minimum **Peak marker**, with a hold of 100–60000 ms
(750 ms default). After expiry, the next accepted fresh sample starts a new hold.
Stale or faulted data clears the marker. **Reference marker** offers Off, Zero
and Manual. Manual is your own finite value in the channel domain, not an ECU
target. Off and Zero retain a finite manual setting; Zero renders actual zero
only inside the scale. Markers clip to the interior and never expand the scale.
Numeric style retains these settings without rendering markers or bands.

**Gauge bands** stores at most three finite, sorted intervals in the channel
domain. Minimum must be less than maximum. Intervals cannot overlap but may
touch. Band colour needs 3:1 contrast with the track after RGB565 quantization.
Labels are optional, at most five printable ASCII characters, and appear in
three fixed rows at 2×2. Hidden settings keep their validation bounds.

Warnings retain source, quality, freshness and operating-context gates. High,
Low, Outside window and Inside window apply to direct rules; lambda retains
qualified deviation from its verified target and required warm/load/fuel-cut
context. Choosing a window initializes illustrative channel-domain endpoints;
configure your own intended thresholds.

Returning from a window to a direct rule restores that channel's example
thresholds in the chosen High/Low order. Battery's Low examples remain
trigger 11.5 V and clear 12.0 V; High uses the reversed pair. These are synthetic
examples, not recommended vehicle limits.

| Mode | Valid ordering |
| --- | --- |
| High | clear < trigger |
| Low | trigger < clear |
| Outside window | trigger low < clear low ≤ clear high < trigger high |
| Inside window | clear low < trigger low ≤ trigger high < clear high |

Dwell advances only with newly admitted measurements. Repainting or time with
no data cannot confirm activation or clear. Priority sorts Critical, High,
Normal, Low, then stable configured view/widget order. Every eligible warning
cycles; a new episode restarts paging. Latch remembers an episode until
acknowledgment and a qualified clear. Stale remembered episodes show
**LATCHED / DATA STALE**. Acknowledging an active episode keeps its indication;
statistics reset does not acknowledge it. Snooze is 5–900 seconds, or 0 for Off.
It suppresses attention while retaining active/latch indication. A new episode
interrupts snooze.

Enabled latch or snooze requires an onboard Dashboard button assigned to
**Acknowledge warnings** in **Device controls**. Acknowledgment identifies the
warning page actually completed, never a newer prepared page. Pending and Busy
remain visible until the producer result arrives. Protected Y recovery remains
available from dark screens.

The appearance preview reconstructs synthetic history and its Warnings button
compresses dwell. **Diagnostics** has a separate persistent timing
simulation: sample normal/elevated values, inject faults, advance without data
to expose gaps/staleness, and process acknowledgment results. Its Dashboard
canvas has separate submit and complete steps. LED and acknowledgment identity
change only on completion of pixels accepted by the canvas. Failed writes retain
the exact candidate for retry. Dashboard, Debug and Logs use their shared pixel
renderers. Startup replay adds status and splash phases without resetting acquisition.
Synthetic controls do not qualify an ECU source or physical device timing.

Rejected edits show **INVALID PREVIEW** and block save/export. Repair the actual
field, change reference mode/value, remove a band, or add the required control
route. Other raw text stays visible. Settings that would hide a retained
graph/fuel input remain on the current
option until that edit and other invalid fields are repaired. The message names
the field that needs attention.
Diagnostic rendering bounds numbers and geometry while retaining raw RGB565
colours, including unreadable choices.
Outside-grid placement projects to the nearest valid position, retaining
size/style; exact attempts and fallbacks are listed. A complete valid repair
creates one undo entry.

See [RGB indicator](led-policy-design.md), [gauges and colours](gauges-and-colours.md)
and [transactional editor state](editor-state.md).
