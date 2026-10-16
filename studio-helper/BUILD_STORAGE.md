# Native build storage

The helper owns only generated attempts recorded in its private `target/studio-helper/attempt-index` index. Existing directories without matching ownership records are quarantined, never adopted or recursively erased. A restarted helper does not turn historical storage into artifact or Flash authority.

## Limits and lifecycle

- At most 16 successful attempt records and 8 GiB retained successful storage. Admission reserves the worst-case 4 GiB new attempt plus 64 MiB metadata margin against the aggregate cap; it can therefore refuse before 8 GiB is physically occupied.
- At most four failed diagnostic records within the 1 GiB aggregate budget. Each closed failure is reduced to accepted input files, source manifest and a durable bounded error summary, at most 16 MiB plus bounded ownership metadata. Generated source and compiler scratch are disposed only after owned process closure and released frozen-input handles.
- One active 4 GiB scratch reservation, requiring another 256 MiB available free-space headroom. The existing process supervisor checks scratch and headroom at most once per second between output-drain iterations. These are bounded polling checks, not an instantaneous filesystem quota. Any failure uses the existing terminate-and-join path. A failed process closure leaves an incomplete reservation and stops further work.
- Inventory bounds: 65,536 members, 64 path components, 1,024 path units, 32 MiB manifest. List operations read at most 21 cached records and immediate fixed-directory children; they do not scan retained trees. Charged closed bytes include regular tree files and both private metadata files.

## Explicit API

Authenticated `GET /api/v1/build-attempts` returns `BuildStorage`: `attempts` (at most 21 `BuildAttemptSummary`) and `reason`. Each summary contains `attempt_id`, `state` (`artifact`, `failed`, `incomplete`), charged `bytes`, and `protected`. While an operation owns the worker, listing returns `409 BuildStorageBusy`; refresh after closure. An invalid, unknown or incomplete index publication produces `409 BuildStorageQuarantined`.

Authenticated `POST /api/v1/build-attempts/cleanup` accepts existing `CleanupRequest` with 1 to 16 unique `attempts` IDs. It starts the same exclusive idle Cleanup job as flash-history cleanup. There is no path input, automatic retry, or device operation.

All selected records are preflighted before the first deletion: closed state, no retained artifact backing, original directory and member volume/file identities, exact inventory and no active readers. Combined cleanup budget is 65,536 members and 32 MiB metadata. Retained DELETE handles bind removal to those identities. Cancellation stops before the next attempt; prior deletions remain. An OS deletion failure is reported truthfully and remaining/inconsistent records stay charged or quarantined for manual review. Unknown children and reparse points are never followed or removed.

A retained artifact backing an active session cannot be cleaned. Older artifacts become eligible when their capability/lease is evicted; helper restart drops session capabilities. Incomplete records require manual review because process closure was not durably proven. No broad startup walk or automatic recovery deletion occurs.
