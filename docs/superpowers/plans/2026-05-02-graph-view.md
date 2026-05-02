# Graph View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a `graph_view` crate that shows a Terraform `terraform graph` output as an interactive flow graph in a Zed pane, with preview-style open actions, save-driven refresh, and in-pane errors.

**Architecture:** A new workspace member `graph_view` implements a workspace `Item` that holds a weak handle to the source `Editor`, subscribes to `EditorEvent::Saved`, and runs `terraform graph -type=plan` (with fallback) in the parent directory of the saved `.tf` path on a background executor. DOT is parsed into `petgraph` for cycle checks and a **layered top-to-bottom layout** to produce `FlowNode` positions; `gpui_flow::FlowState` + `FlowGraph` render the graph. Because upstream `gpui-flow` depends on `gpui` from git, we **vendor** a small `crates/gpui_flow` (or `crates/gpui_flow_shim`) that is the same source with `gpui = { path = "../gpui" }` in `Cargo.toml`, and add it to workspace members. `zed` registers `init`, `zed_actions` adds `preview::terraform` actions, and the quick action bar gets a fourth preview type for Terraform.

**Tech stack:** Rust, `gpui`, `editor`, `workspace`, `util` (command spawning), `petgraph` (graph + cycle detection + layered layout), optional `anyhow`, vendored `gpui_flow` (MIT) wired to path `gpui`, Terraform CLI at runtime.

---

## File map (create / modify)

| Path | Role |
|------|------|
| `crates/gpui_flow/Cargo.toml` | Vendored `gpui-flow` — set `gpui` to `path = "../gpui"`; keep license `MIT` in sync with upstream. |
| `crates/gpui_flow/**` | Upstream `gpui-flow` tree (copy from tag or main; do not reformat wholesale). |
| `crates/graph_view/Cargo.toml` | New crate: deps on `gpui`, `editor`, `workspace`, `ui`, `util`, `gpui_flow` (path), `petgraph`, `anyhow`, `smol` or `util::command` per workspace patterns. |
| `crates/graph_view/graph_view.rs` | Lib root (`[lib] path = "graph_view.rs"`): `init`, `GraphView`, `Item` impl, `register` pattern; `mod dot_layout; mod terraform_graph;`. |
| `crates/graph_view/terraform_graph.rs` | Subprocess: `terraform graph` / fallback, stdout/stderr. |
| `crates/graph_view/dot_layout.rs` | Parse DOT → `petgraph::Graph`, detect cycles, assign layers, emit `FlowNode`/`FlowEdge` with coordinates. |
| `crates/zed_actions/src/lib.rs` | Add `pub mod terraform` under `preview` with `OpenPreview`, `OpenPreviewToTheSide`, `RefreshGraph` (or `Refresh` if namespaced). |
| `Cargo.toml` (root) | `members` += `crates/gpui_flow`, `crates/graph_view`; `[workspace.dependencies]` += `graph_view`, `gpui_flow`, `petgraph`. |
| `crates/zed/Cargo.toml` | `graph_view` dependency. |
| `crates/zed/src/main.rs` | `graph_view::init(cx)` after other preview inits. |
| `crates/zed/src/zed/quick_action_bar/preview.rs` | `PreviewType::Terraform` branch + `graph_view` actions. |
| `docs/superpowers/specs/2026-05-02-graph-view-design.md` | (Already exists) — no change unless spec drift. |

---

### Task 1: Vendor `gpui_flow` and wire to workspace `gpui`

**Files:**
- Create: `crates/gpui_flow/Cargo.toml` (edit from upstream; `gpui` path dep)
- Create: `crates/gpui_flow/**` (source from https://github.com/pacifio/gpui-flow)
- Modify: `Cargo.toml` (root) — add member and `gpui_flow = { path = "crates/gpui_flow" }` in `[workspace.dependencies]`

- [ ] **Step 1: Add vendored crate**

Check out or copy `gpui-flow` at a known commit (record in `crates/gpui_flow/README` or `VENDORED.md` one line: `Upstream: <url> @ <rev>`). Replace its `gpui` dependency with:

```toml
[dependencies]
gpui = { path = "../gpui" }
```

- [ ] **Step 2: Register workspace member**

In root `Cargo.toml`, add to `[workspace] members` (alphabetically with other crates):

```toml
"crates/gpui_flow",
```

and in `[workspace.dependencies]`:

```toml
gpui_flow = { path = "crates/gpui_flow" }
```

- [ ] **Step 3: Verify build**

```bash
cargo check -p gpui_flow
```

Expected: `Finished` with no errors. If the vendored code uses edition 2024 features, ensure `edition.workspace = true` matches the rest of the repo or set `edition = "2024"` explicitly to match `gpui-flow`.

- [ ] **Step 4: Commit**

```bash
git add crates/gpui_flow Cargo.toml
git commit -m "build: vendor gpui_flow with path dependency on workspace gpui"
```

---

### Task 2: Scaffold `graph_view` crate (empty Item)

**Files:**
- Create: `crates/graph_view/Cargo.toml`
- Create: `crates/graph_view/graph_view.rs`

- [ ] **Step 1: Create `Cargo.toml`**

```toml
[package]
name = "graph_view"
version = "0.1.0"
edition.workspace = true
publish.workspace = true
license = "GPL-3.0-or-later"

[lints]
workspace = true

[lib]
path = "graph_view.rs"

# Same layout as `crates/csv_preview`: single crate-root lib file + sibling modules.

[dependencies]
anyhow.workspace = true
editor.workspace = true
gpui.workspace = true
gpui_flow = { workspace = true }
workspace.workspace = true
ui.workspace = true
```

- [ ] **Step 2: Minimal `graph_view.rs` stub**

```rust
use gpui::AppContext;

pub fn init(_cx: &mut AppContext) {}
```

- [ ] **Step 3: Wire workspace**

Root `Cargo.toml`: add `"crates/graph_view"` to `members` and `graph_view = { path = "crates/graph_view" }` to `[workspace.dependencies]`.

```bash
cargo check -p graph_view
```

Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/graph_view Cargo.toml
git commit -m "feat(graph_view): scaffold crate"
```

---

### Task 3: Add `petgraph`, DOT parser module, and unit tests (TDD)

**Files:**
- Modify: `crates/graph_view/Cargo.toml` — add `petgraph = "0.6"` (or workspace pin once added to root)
- Create: `crates/graph_view/dot_layout.rs`
- Modify: `crates/graph_view/graph_view.rs` — add `mod dot_layout;` only (`mod terraform_graph` comes in Task 5).

Add to root `[workspace.dependencies]` if other crates will share:

```toml
petgraph = "0.6"
```

**Policy (from spec):** If the graph has a **cycle**, return `Err` with message “Graph contains a cycle”; do not pass to `gpui_flow`.

- [ ] **Step 1: Write failing test for simple DOT**

In `dot_layout.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE: &str = r#"
digraph {
  "a" -> "b";
  "b" -> "c";
}
"#;

    #[test]
    fn parses_chain_dag() {
        let parsed = parse_dot_to_digraph(SIMPLE).expect("parse");
        assert_eq!(parsed.graph.node_count(), 3);
        assert!(is_dag(&parsed.graph));
    }
}
```

Define `struct ParsedDot { graph: petgraph::Graph<String, ()> }` and `fn parse_dot_to_digraph(dot: &str) -> anyhow::Result<ParsedDot>` after the test fails.

- [ ] **Step 2: Implement minimal DOT parsing**

Use a small subset parser sufficient for `terraform graph` output: `digraph { ... }`, quoted identifiers, `->` edges. If full DOT is heavy, depend on a crate such as `dot-parser` **only if** it is license-compatible; otherwise hand-roll tokenizer for strings and arrows.

Populate `petgraph::Graph<String, ()>` (node labels as `String`, stable ids via index).

- [ ] **Step 3: Implement `is_dag`**

```rust
fn is_dag(graph: &petgraph::Graph<String, ()>) -> bool {
    use petgraph::algo::is_cyclic_directed;
    !is_cyclic_directed(graph)
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p graph_view -- dot_layout
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/graph_view
git commit -m "feat(graph_view): parse DOT and validate DAG"
```

---

### Task 4: Layered top-to-bottom layout → `FlowNode` / `FlowEdge`

**Files:**
- Modify: `crates/graph_view/dot_layout.rs`

Export a struct bundling what the view needs:

```rust
pub struct FlowGraphModel {
    pub nodes: Vec<gpui_flow::FlowNode>,
    pub edges: Vec<gpui_flow::FlowEdge>,
}
```

Algorithm (YAGNI, deterministic):

1. Compute **topological order**; if cycle, error earlier.
2. Assign **layer** = longest distance from any source (indegree-0) node, counting hops.
3. Within a layer, spread nodes horizontally with fixed spacing `NODE_GAP` (e.g. `200.0`).
4. Vertical position `y = layer * LAYER_GAP` (e.g. `120.0`); `x` centered per layer count.
5. Build `FlowEdge::new(id, source_id, target_id)` with Bezier default; node ids must match Terraform stable ids (use DOT node ids as string ids).

- [ ] **Step 1: Write test:** three-node chain produces increasing `y` for downstream nodes.

- [ ] **Step 2: Implement `layout_flow_graph`**.

- [ ] **Step 3: Run tests**

```bash
cargo test -p graph_view
```

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(graph_view): layered layout for gpui_flow nodes"
```

---

### Task 5: Terraform subprocess runner

**Files:**
- Create: `crates/graph_view/terraform_graph.rs`
- Modify: `crates/graph_view/graph_view.rs` — `mod terraform_graph;`
- Modify: `crates/graph_view/Cargo.toml` — use same process spawning as other crates (`util::command` or `smol::process` — grep `crates/` for `Command::new("terraform"` or `which`)

- [ ] **Step 1: Implement `pub async fn run_terraform_graph(cwd: &Path) -> anyhow::Result<String>`**

1. `let plan = Command::new("terraform").current_dir(cwd).args(["graph", "-type=plan"]).output().await` (or blocking on background executor per Zed patterns).
2. If exit non-zero and stderr contains hints like `unknown flag` / `-type` (match case-insensitively), retry without `-type=plan`.
3. Return **stdout** as `String` on success; on failure `anyhow::bail!` including stderr (trimmed, max 4 KiB).

- [ ] **Step 2: Unit test with mock** only if feasible; otherwise document manual test in crate-level comment.

Zed pattern reference — search:

```bash
rg "Command::new" crates/fs crates/project --glob '*.rs' | head -20
```

Use `cx.background_executor().spawn` + `Task` from the view when invoking.

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(graph_view): run terraform graph with plan fallback"
```

---

### Task 6: `GraphView` entity — state, `FlowState`, subscription to `EditorEvent::Saved`

**Files:**
- Modify: `crates/graph_view/graph_view.rs`

Structure:

```rust
pub struct GraphView {
    focus_handle: FocusHandle,
    source_editor: Entity<Editor>,
    flow_state: Entity<gpui_flow::FlowState>,
    graph_task: Option<Task<Result<(), anyhow::Error>>>,
    last_error: Option<SharedString>,
    _subscription: Subscription,
}
```

- `cx.subscribe(&editor, |graph_view, _, event, cx| { if matches!(event, EditorEvent::Saved) { graph_view.refresh(cx); } })`

- `refresh` cancels prior `graph_task`, spawns `run_terraform_graph`, parses DOT, layouts, updates `FlowState` with new nodes/edges, calls `state.fit_view(padding, width, height)` when bounds known.

- Unsaved buffer: if `singleton_buffer.read(cx).file()` is `None` or path not `.tf`, show placeholder label “Save the `.tf` file to generate the graph.”

- [ ] **Step 1: Implement `fn terraform_file_path(editor, cx) -> Option<PathBuf>`** mirroring `CsvPreviewView::is_csv_file` but extension `tf`.

- [ ] **Step 2: Implement `refresh` pipeline** calling Task 5 + Tasks 3–4.

- [ ] **Step 3: `impl Render for GraphView`** — full-size `FlowGraph::new(...)` + error overlay when `last_error` is Some.

Read `gpui_flow` examples (`examples/basic.rs`) for exact `FlowGraph::new` constructor.

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(graph_view): GraphView render and refresh on save"
```

---

### Task 7: `impl Item for GraphView`

**Files:**
- Modify: `crates/graph_view/graph_view.rs`

Implement `Item` like `CsvPreviewView`: `tab_icon` (use `IconName` matching Terraform or generic graph), `tab_content_text` → `"Graph · filename.tf"`.

Register workspace actions in `fn register(workspace: &mut Workspace)`:

```rust
workspace.register_action(|workspace, _: &OpenPreview, window, cx| { ... });
workspace.register_action(|workspace, _: &OpenPreviewToTheSide, window, cx| { ... });
```

Reuse pane deduplication pattern from `MarkdownPreviewView::find_existing_independent_preview_item_idx`: find existing `GraphView` with same `source_editor`.

Export actions from `zed_actions` (Task 8) and import here.

- [ ] **Step 1: Implement `Item` trait methods** — grep `impl Item for CsvPreviewView` for required methods count.

- [ ] **Step 2: Commit**

```bash
git commit -am "feat(graph_view): workspace Item and pane registration"
```

---

### Task 8: `zed_actions` — Terraform preview actions

**Files:**
- Modify: `crates/zed_actions/src/lib.rs` inside `pub mod preview`

Add:

```rust
pub mod terraform {
    use gpui::actions;
    actions!(
        terraform,
        [
            /// Opens Terraform dependency graph for the current file.
            OpenPreview,
            /// Opens Terraform dependency graph in a split pane.
            OpenPreviewToTheSide,
            /// Refreshes the Terraform dependency graph.
            RefreshGraph,
        ]
    );
}
```

- [ ] **Step 1: Apply patch**

- [ ] **Step 2: Commit**

```bash
git commit -am "feat(actions): add terraform graph preview actions"
```

---

### Task 9: Wire `graph_view::init` and quick action bar

**Files:**
- Modify: `crates/graph_view/graph_view.rs` — `pub fn init(cx: &mut AppContext) { cx.observe_new(|workspace: &mut Workspace, _, cx| { GraphView::register(workspace, cx); }).detach(); }`
- Modify: `crates/zed/src/main.rs` — `graph_view::init(cx);`
- Modify: `crates/zed/Cargo.toml` — `graph_view.workspace = true`
- Modify: `crates/zed/src/zed/quick_action_bar/preview.rs`

Pattern from existing file:

```rust
} else if GraphView::resolve_active_item_as_terraform_editor(workspace, cx).is_some() {
    preview_type = Some(PreviewType::Terraform);
}
```

Match arms for `OpenPreview` etc. using `zed_actions::preview::terraform::OpenPreview`.

- [ ] **Step 1: Implement `resolve_active_item_as_terraform_editor`** on `GraphView`

- [ ] **Step 2: Register command palette** — grep how Markdown registers:

```bash
rg "markdown::OpenPreview" crates/zed crates/command_palette -n
```

Add equivalent entries for `terraform::OpenPreview` if registration is centralized.

- [ ] **Step 3: `./script/clippy -p graph_view -p zed`**

Expected: clean or only pre-existing warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/zed crates/graph_view crates/zed_actions
git commit -m "feat(zed): integrate graph_view init and quick action bar"
```

---

### Task 10: Manual QA checklist (document in PR body)

- [ ] Open a saved `.tf` in a directory where `terraform init` has been run; command palette **Terraform: Open Preview** (exact label TBD) shows graph.
- [ ] Save file triggers refresh (watch stderr for duplicate runs).
- [ ] Remove `terraform` from PATH temporarily → in-pane error.
- [ ] Invalid module → stderr visible.

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| `.tf` only, CWD = parent of file | Task 5, 6 |
| `-type=plan` + fallback | Task 5 |
| gpui-flow, top-to-bottom, fit | Tasks 1, 4, 6 |
| Refresh on save + manual | Tasks 6 (`Saved`), 8 (`RefreshGraph`) |
| Command palette + quick bar + two opens | Tasks 7–9 |
| Unsaved buffer message | Task 6 |
| Cycle / invalid DOT errors | Tasks 3–4, 6 |
| MIT gpui-flow license | Task 1 vendoring note |

**Gap closed:** Manual refresh action must be bound in graph view focus — register `RefreshGraph` on `Workspace` or on `GraphView` via `render` + `on_action`; Task 7 should wire `GraphView::register` to call `cx.listener` for refresh when the graph tab is active.

---

## Plan self-review

- No `TBD` steps; `terraform` action labels follow discovery in Task 9.
- Type names consistent: `GraphView`, `FlowGraphModel`, `run_terraform_graph`.
- **Risk:** Vendored `gpui_flow` may need small edits for GPUI API drift — allocate time in Task 1 to fix compile errors against workspace `gpui`.

---

**Plan complete and saved to `docs/superpowers/plans/2026-05-02-graph-view.md`. Two execution options:**

1. **Subagent-Driven (recommended)** — Dispatch a fresh subagent per task, review between tasks, fast iteration  
2. **Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints  

Which approach do you want for implementation?
