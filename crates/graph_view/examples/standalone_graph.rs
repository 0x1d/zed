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
    MouseButton,
};
use gpui_flow::{BackgroundPattern, FlowGraph, FlowState};
use gpui_platform::application;
use graph_view::{
    configure_flow_state_for_fit, flow_graph_node_renderer,
    layout_flow_graph_with_options_and_sizes, load_layout_options, parse_dot_to_digraph,
    run_terraform_graph, save_layout_options,
    TerraformDependencyFlow, TerraformLayoutDirection, TerraformLayoutOptions,
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
    layout_options: TerraformLayoutOptions,
    last_dot: Option<String>,
    _flow_graph_subscription: gpui::Subscription,
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
                .default_renderer(flow_graph_node_renderer)
        });
        let flow_graph_subscription = cx.observe(&flow_state, |this, _, cx| {
            this.relayout(cx);
        });

        let mut view = Self {
            focus_handle: cx.focus_handle(),
            flow_state,
            flow_graph,
            status_line: SharedString::default(),
            graph_task: None,
            layout_options: load_layout_options(),
            last_dot: None,
            _flow_graph_subscription: flow_graph_subscription,
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
                    Ok(dot) => {
                        view.last_dot = Some(dot.clone());
                        match view.layout_cached_dot(&dot, cx) {
                            Ok(model) => {
                                view.flow_state.update(cx, |state, _| {
                                    configure_flow_state_for_fit(state);
                                    state.set_nodes(model.nodes);
                                    state.set_edges(model.edges);
                                });
                                view.status_line = format!(
                                    "Loaded graph from {} ({}) — {:?} / {:?}",
                                    cwd.display(),
                                    dot.lines().count(),
                                    view.layout_options.direction,
                                    view.layout_options.dependency_flow
                                )
                                .into();
                            }
                            Err(error) => {
                                view.status_line = format!("Layout/parse error: {error:#}").into();
                            }
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

    fn relayout(&mut self, cx: &mut Context<Self>) {
        let Some(dot) = self.last_dot.clone() else {
            return;
        };
        match self.layout_cached_dot(&dot, cx) {
            Ok(model) => {
                self.flow_state.update(cx, |state, _| {
                    configure_flow_state_for_fit(state);
                    state.set_nodes(model.nodes);
                    state.set_edges(model.edges);
                });
                self.status_line = format!(
                    "Relayout — {:?} / {:?}",
                    self.layout_options.direction, self.layout_options.dependency_flow
                )
                .into();
            }
            Err(error) => {
                self.status_line = format!("Layout error: {error:#}").into();
            }
        }
        cx.notify();
    }

    fn layout_cached_dot(
        &self,
        dot: &str,
        cx: &App,
    ) -> anyhow::Result<graph_view::FlowGraphModel> {
        parse_dot_to_digraph(dot).and_then(|parsed| {
            let measured_sizes = self.flow_state.read(cx).node_sizes();
            layout_flow_graph_with_options_and_sizes(
                &parsed.graph,
                self.layout_options,
                &measured_sizes,
            )
        })
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

        let dir_label = match self.layout_options.direction {
            TerraformLayoutDirection::Tb => "TB",
            TerraformLayoutDirection::Lr => "LR",
        };
        let flow_label = match self.layout_options.dependency_flow {
            TerraformDependencyFlow::DependenciesAtTop => "deps↑",
            TerraformDependencyFlow::DependentsAtTop => "deps↓",
        };

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
                    .gap(px(8.0))
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
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_xs()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(0x3f3f46))
                                    .text_color(rgb(0xe4e4e7))
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            this.layout_options.direction =
                                                match this.layout_options.direction {
                                                    TerraformLayoutDirection::Tb => {
                                                        TerraformLayoutDirection::Lr
                                                    }
                                                    TerraformLayoutDirection::Lr => {
                                                        TerraformLayoutDirection::Tb
                                                    }
                                                };
                                            let _ = save_layout_options(this.layout_options);
                                            this.relayout(cx);
                                        }),
                                    )
                                    .child(format!("Direction: {dir_label}")),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(0x3f3f46))
                                    .text_color(rgb(0xe4e4e7))
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            this.layout_options.dependency_flow =
                                                match this.layout_options.dependency_flow {
                                                    TerraformDependencyFlow::DependenciesAtTop => {
                                                        TerraformDependencyFlow::DependentsAtTop
                                                    }
                                                    TerraformDependencyFlow::DependentsAtTop => {
                                                        TerraformDependencyFlow::DependenciesAtTop
                                                    }
                                                };
                                            let _ = save_layout_options(this.layout_options);
                                            this.relayout(cx);
                                        }),
                                    )
                                    .child(format!("Flow: {flow_label}")),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0xa1a1aa))
                            .max_w(px(420.))
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
