# Using ECU Reference in Studio

Open **ECU Reference** in the workspace tab bar or command palette. The reference is embedded and works offline; public source links open only when clicked. It provides documentary context, not a live vehicle connection or capture tool.

Search by a signal name, DID, unit or topic: `39C0`, `boost`, `DQ381`, `GPF`, `E5`, `gear`, `65535`. Search is case-insensitive and matches all whitespace-separated terms. Section and status filters combine with the query. A no-results state offers a clear-filter action and retains the selected detail. Queries are capped at 256 UTF-8 bytes, with an explicit truncation notice.

The list shows section and textual status. Select a card to read applicability, semantics, units, documentary scaling, qualification limits and public sources. Generic decoder implementation is not exact-car support. Conflicts show competing definitions rather than silently selecting one. Nothing on a card enables polling, changes a widget or supplies a warning threshold.

In **Data & warnings → Channel sources**, choose **Why this source? Open ECU Reference** to inspect that definition. The link clears search filters so the selected entry is visible. **Open channel source settings** returns to the retained selected channel; it does not assign the card's source. **Return to Views** restores the existing editor workspace.

Workspace panels remain mounted. Navigation uses the existing Studio helper to release transient drag/menu/focus ownership while retaining accepted configuration, selection, raw/JSON drafts and the shared preview owner. Reference query, filters and selection survive workspace changes within the session; they are not persisted to local storage. The ordinary workspace preference can reopen the reference tab next time.

Use Left/Right/Home/End in the tab bar, then Enter or Space to activate. All filters and result buttons have visible labels and keyboard focus. Status is conveyed in text as well as colour. At narrow widths, the list stacks above the detail; the shared preview remains Studio-owned.

See the [technical ECU library](../vehicle/ecu-reference/README.md) and [generated catalogue](../vehicle/ecu-reference/signals.md). Browser keyboard, narrow/wide layout and final screenshot validation are separate integration checks; native catalogue tests do not prove browser rendering or physical support.
