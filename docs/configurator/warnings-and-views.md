# Warning instances and named views

Each configured widget owns its warning rule. The eight channels still share
one acquisition history and poll schedule. Rule indices and channel indices
are separate: rule 63 may observe channel 0, with a different threshold from
another widget using that channel. Disabled and DemoOnly rules cannot enable
vehicle alarms. Source quality, freshness, operating context, hysteresis,
accepted-sample dwell and sequence guards remain enforced before publication.

Dashboard rotation selects an ordered view using signed modulo. The selected
view survives Debug/Logs and resets to the configured boot view at reboot.
Rotation does not change acquisition or write flash. Other pages keep their
existing encoder controls. The view name appears briefly as feedback and
remains in the header.

Motion interrupts an older pending screen-off transition before the same
batch's clicks run. Click feedback wins; an Off created by that new click is
preserved. Motion while already black changes selection without waking the
display or being counted as visible input. Full, Dim at 50%, and the existing
two-second, completion-gated Off cycle are unchanged.

Warnings follow configured view and widget order. CycleOne shows one message;
ListThree shows up to three 20-character lines, with further pages rotating
by elapsed time. AllViews and CurrentView scopes are configurable. Active-set
changes preserve a still-active cursor and its deadline, preventing new alarms
from continually restarting the page. User feedback takes priority. Debug and
Logs remain clear. The next visible warning sequence restarts after feedback.

One resolved overlay drives drawing, retained keys, input exclusion and the
presentation-gated LED. Red/off follows a completed warning frame; other
states remain green. Old frame completions cannot apply a newer view's LED or
backlight transition.

## Host evidence

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

## Target scope

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
