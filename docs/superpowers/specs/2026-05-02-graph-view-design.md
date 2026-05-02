# Graph View design (Terraform, `graph_view` crate)

## Summary

Add a new workspace crate, **`graph_view`**, that provides a **Graph View** pane in the Zed editor. For **v1**, the only supported source is an open **`.tf`** (Terraform) file. The view runs **`terraform graph`** with working directory set to the **parent directory of the active buffer’s file path**, renders the resulting **DOT** as a **directed acyclic graph** using the **[gpui-flow](https://github.com/pacifio/gpui-flow)** library: **automatic layout**, **top-to-bottom** flow, **center/fit** in the viewport.

Integration follows existing **preview-style** patterns (e.g. `csv_preview`): **command palette** actions, **quick action bar** when the active item is a qualifying editor, and **two open variants** (active pane vs split to the side).

## Goals

- User can open **Graph View** from a **Terraform** buffer and see a **visual dependency graph**.
- **Refresh** happens **on save** of the linked buffer and via an explicit **Refresh graph** action.
- **Failures** (missing binary, non-zero exit, unusable output) are **visible in the pane**, not silent.

## Non-goals (v1)

- Languages other than **`.tf`**.
- Walking ancestor directories to find a “Terraform root”; CWD is **always** the directory of the **active file** (see constraints).
- **`.tfvars`** as a primary graph source (may be added later).
- Requiring **Graphviz** on the user machine for layout (prefer **`gpui-flow`**-only layout; system `dot` remains a possible **future fallback** if integration demands it).

## User-visible behavior

### Eligibility

- Graph View actions apply when the **active item** is an **editor** whose buffer has a **saved** path ending in **`.tf`**.
- **Unsaved buffers** (no path on disk): show a clear message that **saving the file is required** before generating a graph (avoids ambiguous CWD).

### Opening the view

- **Command palette:** e.g. open graph / open to side (exact naming follows existing preview conventions).
- **Quick action bar:** icon or control appears when a qualifying Terraform editor is active (same pattern as Markdown/CSV preview wiring).
- **Two actions:** open in the **active pane** vs **split to the side** (mirror CSV preview behavior).

### Terraform command

- Working directory: **`parent` of the active `.tf` file path**.
- Primary invocation: **`terraform graph -type=plan`**.
- If that fails in a way consistent with **unsupported `-type=plan`** (or equivalent), **retry** with **`terraform graph`** (no plan type).
- **stdin:** not used; **stdout** is DOT text; **stderr** is captured for error UI.

### Rendering

- Parse DOT into an internal **directed graph** representation suitable for **`gpui-flow`**.
- Layout: **DAG**, **vertical** primary direction (**top to bottom**), **automatic** layout, **fit/center** content in the view on initial open and after refresh (exact interaction matches **`gpui-flow`** capabilities). If the parsed graph is **not** acyclic, show an **in-pane error** (or the closest **`gpui-flow`** supports) rather than undefined layout.
- If Terraform output is **not** a valid/usable graph for the renderer, show an **error state** with relevant stderr/stdout excerpts.

### Refresh

- **On save:** when the **bound editor buffer** is saved, refresh the graph (same subprocess pipeline).
- **Manual:** **Refresh graph** action always available from the graph item (and palette if useful).

## Architecture

| Piece | Responsibility |
|--------|------------------|
| **`graph_view` crate** | `Item` implementation for the graph pane; subprocess invocation; DOT handling; **`gpui-flow`** integration; refresh subscriptions; error UI |
| **Editor / workspace wiring** | Register `init`, actions, quick action bar hooks in **`zed`** (or equivalent central init), analogous to `csv_preview::init` |

### Data flow

1. Resolve **active Terraform editor** and **file path** → `cwd = path.parent()`.
2. Spawn **`terraform graph -type=plan`** in `cwd`; on plan-mode failure, **`terraform graph`**.
3. **Parse** stdout DOT → graph model → **`gpui-flow`** scene.
4. **Subscribe** to buffer save events for the linked editor; **debounce** only if needed to avoid overlapping runs (cancel prior task on new run).

### Error handling

| Condition | Behavior |
|------------|----------|
| Terraform not in `PATH` | In-pane error; suggest installing Terraform |
| Non-zero exit | Show stderr; optional short stdout snippet |
| Plan mode unsupported | Automatic fallback to default `terraform graph` (already specified) |
| Invalid DOT / empty graph | In-pane error; do not show a blank success state |

### Dependencies

- **`gpui-flow`**: add as a **git** (or crates.io if published) dependency; verify **license compatibility** with the Zed workspace before merge.
- **DOT parsing**: use a maintained Rust parser or minimal DOT subset parser consistent with `terraform graph` output—choice is an implementation detail as long as behavior matches Terraform’s DOT for supported versions.

## Testing

- **Unit tests:** DOT → graph model fixtures (small graphs, cycles handled as error or filtered per parser policy—**document chosen policy** in implementation).
- **Integration tests** against real `terraform`: **optional** in CI if Terraform is not available; document **manual test** steps (open fixture `.tf`, assert graph appears).

## Open points for implementation (not product ambiguity)

- Exact **`gpui-flow`** API surface for **fit** and **direction** (resolved during implementation spike).
- Precise **feature flag** vs always-on: default to matching **`csv_preview`** / product preference at implementation time.

## References

- [Terraform `graph` command](https://developer.hashicorp.com/terraform/cli/commands/graph)
- [gpui-flow repository](https://github.com/pacifio/gpui-flow)
