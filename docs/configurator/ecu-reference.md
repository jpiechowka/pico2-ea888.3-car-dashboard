# Using ECU Reference in Studio

Open **ECU Reference** in the workspace tab bar or command palette. The reference is embedded and works offline; public source links open only when clicked. It provides documentary context, not a live vehicle connection or capture tool.

Search by a signal name, DID, unit or topic: `39C0`, `boost`, `DQ381`, `GPF`, `E5`, `gear`, `65535`. Search is case-insensitive and matches all whitespace-separated terms. Section and status filters combine with the query. A no-results state offers a clear-filter action and retains the selected detail. Queries are capped at 256 UTF-8 bytes, with an explicit truncation notice.

The list shows section and textual status. Select a card to read applicability, semantics, units, documentary scaling, qualification limits and public sources. Generic decoder implementation is not exact-car support. Conflicts show competing definitions rather than silently selecting one. Nothing on a card enables polling, changes a widget or supplies a warning threshold.

In **Data & warnings → Channel sources**, choose **Why this source? Open ECU Reference** to inspect that definition. The link clears search filters so the selected entry is visible. **Open channel source settings** returns to the retained selected channel; it does not assign the card's source. **Return to Views** restores the existing editor workspace.

Workspace panels remain mounted. Navigation uses the existing Studio helper to release transient drag/menu/focus ownership while retaining accepted configuration, selection, raw/JSON drafts and the shared preview owner. Reference query, filters and selection survive workspace changes within the session; they are not persisted to local storage. The ordinary workspace preference can reopen the reference tab next time.

Use Left/Right/Home/End in the tab bar, then Enter or Space to activate. All filters and result buttons have visible labels and keyboard focus. Status is conveyed in text as well as colour. At narrow widths, the list stacks above the detail; the shared preview remains Studio-owned.

See the [technical ECU library](../vehicle/ecu-reference/README.md) and [generated catalogue](../vehicle/ecu-reference/signals.md). Browser keyboard, narrow/wide layout and final screenshot validation are separate integration checks; native catalogue tests do not prove browser rendering or physical support.

## Inspecting the shipped decoder

Cards with a dashboard binding also show the authoritative runtime catalogue's request bytes, decoder identifier, reply byte width/order and implementation availability. Requests exclude CAN and ISO-TP framing. These fields describe the shipped definition, not a live reply and not your configured route or polling interval. A named candidate decoder can remain unavailable. Read the research conflict immediately above it before interpreting a source.

## Diagnostic context while researching

Use **Diagnostics** to inspect the existing event list, including severity, category, stable code, subject, sequence, elapsed time and repeated-event count. Warn/Error console output is enabled by default; Debug/Trace is opt-in and shares a ten-events-per-second allowance. Browser output uses fixed severity colours and keeps severity text; generated messages are data, not console formatting strings. The store holds at most512 events and exports at most2MiB of JSONL.

The browser developer console belongs to the browser. It cannot write directly into the PowerShell terminal running Trunk. Native helper job output has its own bounded stream and is separate from browser editing events. Enable **Forward Warn/Error context to local helper terminal** in Diagnostics after connecting the helper. This opt-in lasts only for the current browser session. It sends approved code/category/severity plus workspace, event sequence, client elapsed time and the accepted revision at event capture. Browser console lines include workspace/revision too; the stored event's local message remains available for correlation. The relay never sends messages, subjects, configuration, artwork or capability URLs in its body.

The relay owns a separate request slot, a16-record queue and a two-second desired abort deadline. Both ends allow at most10 records per rolling second; each batch contains at most4 events. Disconnect or opting out clears pending context and aborts the request. An unsettled request keeps its lease, so a stalled relay cannot accumulate more requests. Unknown event codes, full queues, rate limits and uncertain delivery count as dropped; batches are not replayed. Native terminal output uses an independent32-record mailbox and never blocks job pipe readers. Colours respect `NO_COLOR` and terminal detection. An acknowledgement means mailbox admission, not proof of display. Controls report availability, queued, admitted, dropped and pending state; forwarding failures remain local and are not recursively forwarded. Reference browsing does not open a vehicle connection or produce per-frame logs. Do not paste VIN, capability URLs, full configuration or private correspondence into a diagnostic report.
