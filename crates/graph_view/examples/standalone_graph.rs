//! Standalone window: runs `terraform graph` in a fixture directory and shows the flow graph.
//!
//! Does **not** link the full Zed editor stack (`graph_view` built with `--no-default-features`).
//!
//! Prerequisites:
//! - `terraform` on `PATH`
//! - From this crate directory:
//!   `cd examples/fixtures/vercel_supabase_stack && terraform init`
//!
//! Run from workspace root:
//! ```text
//! cargo run -p graph_view --example standalone_graph --no-default-features
//! ```

#![cfg_attr(not(target_family = "wasm"), allow(clippy::disallowed_methods))]

use std::path::PathBuf;

use gpui::{
    App, Bounds, Context, Entity, FocusHandle, Focusable, ParentElement as _, Render, SharedString,
    Styled as _, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use gpui_flow::{BackgroundPattern, FlowGraph, FlowState};
use gpui_platform::application;
use graph_view::{
    configure_flow_state_for_fit, layout_flow_graph, parse_dot_to_digraph, run_terraform_graph,
};

const FLOW_BG: u32 = 0xf8f8f8;
const FLOW_GRID: u32 = 0xd4d4d4;
const FLOW_NODE_BG: u32 = 0xffffff;
const FLOW_NODE_BORDER: u32 = 0xe2e2e2;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/fixtures/vercel_supabase_stack")
}

struct StandaloneGraph {
    focus_handle: FocusHandle,
    flow_state: Entity<FlowState>,
    flow_graph: Entity<FlowGraph>,
    status_line: SharedString,
    graph_task: Option<gpui::Task<()>>,
}

impl StandaloneGraph {
    fn new(cx: &mut Context<Self>) -> Self {
        let flow_state = cx.new(|_| {
            let mut state = FlowState::new(Vec::new(), Vec::new());
            configure_flow_state_for_fit(&mut state);
            state
        });
        let flow_graph = cx.new(|cx| {
            FlowGraph::new(flow_state.clone(), cx)
                .bg_color(FLOW_BG)
                .grid_color(FLOW_GRID)
                .bg_pattern(BackgroundPattern::Dots)
                .node_bg_color(FLOW_NODE_BG)
                .node_border_color(FLOW_NODE_BORDER)
        });

        let mut view = Self {
            focus_handle: cx.focus_handle(),
            flow_state,
            flow_graph,
            status_line: SharedString::default(),
            graph_task: None,
        };
        view.refresh(cx);
        view
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let cwd = fixture_dir();
        self.status_line = format!("Running terraform graph in {} …", cwd.display()).into();
        cx.notify();

        self.graph_task = Some(cx.spawn(async move |this, cx| {
            let result = run_terraform_graph(&cwd).await;

            this.update(cx, |view, cx| {
                match result {
                    Ok(dot) => match parse_dot_to_digraph(&dot)
                        .and_then(|parsed| layout_flow_graph(&parsed.graph))
                    {
                        Ok(model) => {
                            view.flow_state.update(cx, |state, _| {
                                configure_flow_state_for_fit(state);
                                state.set_nodes(model.nodes);
                                state.set_edges(model.edges);
                            });
                            view.status_line =
                                format!("Loaded graph from {} ({})", cwd.display(), dot.lines().count())
                                    .into();
                        }
                        Err(error) => {
                            view.status_line = format!("Layout/parse error: {error:#}").into();
                        }
                    },
                    Err(error) => {
                        view.status_line = format!(
                            "terraform graph failed: {error:#}\nRun `terraform init` in {}",
                            cwd.display()
                        )
                        .into();
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }
}

impl Focusable for StandaloneGraph {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for StandaloneGraph {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let w = viewport.width.as_f32();
        let h = viewport.height.as_f32();
        self.flow_state.update(cx, |state, _| {
            configure_flow_state_for_fit(state);
            state.fit_view(80.0, w, h);
        });

        // FlowGraph must fill the same rectangle used by fit_view (full client area). A flex toolbar
        // above would shift the graph pane without updating viewport math, so edges and nodes
        // misalign ("floating" edges). Overlay the toolbar instead.
        div()
            .size_full()
            .relative()
            .bg(rgb(0x1c1c1c))
            .track_focus(&self.focus_handle)
            .child(div().size_full().child(self.flow_graph.clone()))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .py_2()
                    .bg(rgb(0x27272a))
                    .border_b_1()
                    .border_color(rgb(0x3f3f46))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xe4e4e7))
                            .child("graph_view standalone — vercel_supabase_stack fixture"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0xa1a1aa))
                            .max_w(px(720.))
                            .child(self.status_line.clone()),
                    ),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1400.0), px(880.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| StandaloneGraph::new(cx)),
        )
        .expect("open window");
        cx.activate(true);
    });
}
