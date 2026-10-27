# Warning instances and named views

Each configured widget owns its warning rule. Scalar widgets share the eight
per-channel acquisition histories and poll schedules. Rule indices and channel indices
are separate: rule 63 may observe channel 0, with a different threshold from
another widget using that channel. Disabled and DemoOnly rules cannot enable
vehicle alarms. Source quality, freshness, operating context, hysteresis,
accepted-sample dwell and sequence guards remain enforced before publication.

An ignition widget instead observes four independent cylinder samples through
one shared DemoOnly rule, with no scalar history or poll route. A fresh affected
cylinder can support activation; clearing needs all four fresh and clear.
Unavailable cylinders lose hysteresis support. Shared activation dwell resets
when no supporter remains; clear dwell resets when all-four freshness is lost.
Remembered latch membership stays distinct from current fresh membership.
A new member joining an active episode advances its acknowledgment identity.
See [sample quality](../vehicle/sample-quality.md) for the acquisition boundary.

Dashboard rotation selects an ordered view using signed modulo. The selected
view survives Debug/Logs and resets to the configured boot view at reboot.
Rotation does not change acquisition or write flash. The defaults use view
rotation on Dashboard and brightness/log rotation on Debug/Logs; accepted remaps
follow the shared [controls contract](control-mapping-design.md). The view name appears briefly as feedback and
remains in the header.

Motion interrupts an older pending screen-off transition before the same
batch's clicks run. Click feedback wins; an Off created by that new click is
preserved. Motion while already black changes selection without waking the
display or being counted as visible input. Full, Dim at 50%, and the existing
two-second, completion-gated Off cycle are unchanged.

Warnings sort Critical, High, Normal, Low priority, then configured view and
widget order. CycleOne shows one message;
ListThree shows up to three ordinary 20-character messages. An ignition rule
using CylinderLines owns a dedicated page of up to four prefixed cylinder lines
in either mode; its configured message remains one bounded 20-byte value.
Further pages rotate by elapsed time. AllViews and CurrentView scopes are configurable. Active-set
changes preserve a still-eligible cursor unless a new episode or higher priority
requires interruption. User feedback takes priority. Debug and
Logs remain clear. The next visible warning sequence restarts after feedback.

One resolved overlay drives drawing, retained keys, input exclusion and the
presentation-gated LED. Default red/off follows a completed warning frame;
normal indication is green. Configured patterns and background switches follow
[the RGB policy](led-policy-design.md). Old frame completions cannot apply a newer view's LED or
backlight transition.

Latch retains an episode until acknowledgment and qualified clear; missing/stale
data never supplies that clear or a live number. Acknowledged active/latching
widgets retain steady badges. Snooze suppresses attention, with indication
retained; expiry and a new episode resume attention. AcknowledgeWarnings targets
the completed warning page and reports a matching producer result. Enabled latch
or snooze requires an onboard Dashboard route. See [advanced controls](advanced-controls.md).

## Historical configured-rule host evidence

The two outline suites contain 319 tests each, including the existing alarm
guard tests and eleven configured-rule/navigation tests. New coverage includes
all 64 rules with permuted channels, all warning pages, stale/fault/recovery
paths, cursor churn, maximum text, both retained buffers versus full redraw,
view/feedback precedence, large signed deltas and old/new Off token separation.

`host/baselines/configurator_rules_a` preserves seven workloads, 30 samples
each, 206 hashed source inputs and an observed Criterion affinity mask of 1.
These are host measurements, not MCU cycles or physical FPS:

| Workload | Median |
| --- | ---: |
| Legacy batch and snapshot | 549.439 ns |
| Legacy typed sample and snapshot | 264.850 ns |
| Watch copy roundtrip | 82.477 ns |
| Eight-channel alarm samples | 249.664 ns |
| 64-rule ingest fixture | 497.244 ns |
| 64-rule snapshot copy | 280.407 ns |
| 64-rule warning page | 412.292 ns |

The acquisition enum and snapshot have changed. These establish the new
configured workloads; they do not claim a speedup against an old fixture.
Raw samples, confidence intervals and exact sources are retained for review.

## Historical configured-rule target scope

The traced Demo 280/70 default and maximum documents pass the 64 KiB minimum
core 0 headroom gate, flash helper/startup/vector checks and 18 corrupted-artifact
rejections each. Both use 441936 bytes of static RAM, leaving 82352 bytes for
core 0. The normal inspected boot chains are 51144/51328 bytes; the inspected
core 1 snapshot chain is 18992 of its 32768-byte stack. These linked-code sums
exclude interrupt/panic overhead and are not hardware high-water measurements.
Final ELF SHA-256 values are
`7cd2e0b41dd6ae930a45af18101b55bfab98d57aea6d9bfa84bba17af0918b05`
and `3f98db5abff479d03ebd92693d78faba5be5364062dabe97a270b960749ff661`;
213 source/archive hashes match each frozen artifact. Snapshot copies remain
bounded under the existing cross-core critical-section Watch; evaluation is
outside that lock. A host copy benchmark does not establish interrupt latency.

No pin, clock, DMA ownership, journal layout or hardware qualification is
changed by this stage. ECU support, optical FPS, physical warning/LED timing
and sustained device stability still require hardware validation.
