use std::path::PathBuf;

use editor::{Editor, EditorEvent};
use gpui::{
    div, px, prelude::FluentBuilder as _, AppContext, Context, Entity, FocusHandle, Focusable,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Task,
};
use gpui_flow::{BackgroundPattern, FlowGraph, FlowState};
use ui::prelude::*;
use workspace::Workspace;

mod dot_layout;
mod terraform_graph;

pub use dot_layout::{
    is_dag, layout_flow_graph, parse_dot_to_digraph, FlowGraphModel, ParsedDot,
};
pub use terraform_graph::run_terraform_graph;

const FLOW_BG: u32 = 0xf8f8f8;
const FLOW_GRID: u32 = 0xd4d4d4;
const FLOW_NODE_BG: u32 = 0xffffff;
const FLOW_NODE_BORDER: u32 = 0xe2e2e2;

pub fn init(_cx: &mut impl AppContext) {}

/// Returns the absolute path of the editor buffer when it is a singleton local `.tf` file.
pub fn terraform_file_path(editor: &Entity<Editor>, cx: &App) -> Option<PathBuf> {
    editor
        .read(cx)
        .buffer()
        .read(cx)
        .as_singleton()
        .and_then(|buffer| {
            let file = buffer.read(cx).file()?;
            let path = file.as_local()?.abs_path(cx);
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("tf"))
                .then_some(path)
        })
}

pub struct GraphView {
    pub focus_handle: FocusHandle,
    pub source_editor: Entity<Editor>,
    pub flow_state: Entity<FlowState>,
    flow_graph: Entity<FlowGraph>,
    graph_task: Option<Task<()>>,
    graph_generation: u64,
    pending_fit_view: bool,
    last_container: Option<(f32, f32)>,
    pub last_error: Option<SharedString>,
    _subscription: Subscription,
}

impl GraphView {
    pub fn new(source_editor: &Entity<Editor>, cx: &mut Context<Workspace>) -> Entity<Self> {
        cx.new(|cx| Self::create(source_editor, cx))
    }

    fn create(source_editor: &Entity<Editor>, cx: &mut Context<Self>) -> Self {
        let flow_state = cx.new(|_| FlowState::new(Vec::new(), Vec::new()));
        let flow_graph = cx.new(|cx| {
            FlowGraph::new(flow_state.clone(), cx)
                .bg_color(FLOW_BG)
                .grid_color(FLOW_GRID)
                .bg_pattern(BackgroundPattern::Dots)
                .node_bg_color(FLOW_NODE_BG)
                .node_border_color(FLOW_NODE_BORDER)
        });

        let editor = source_editor.clone();
        let subscription = cx.subscribe(&editor, |this, _, event, cx| {
            if matches!(event, EditorEvent::Saved) {
                this.refresh(cx);
            }
        });

        let mut view = Self {
            focus_handle: cx.focus_handle(),
            source_editor: source_editor.clone(),
            flow_state,
            flow_graph,
            graph_task: None,
            graph_generation: 0,
            pending_fit_view: false,
            last_container: None,
            last_error: None,
            _subscription: subscription,
        };

        view.refresh(cx);
        view
    }

    pub fn flow_graph(&self) -> &Entity<FlowGraph> {
        &self.flow_graph
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.graph_task = None;

        let Some(terraform_path) = terraform_file_path(&self.source_editor, cx) else {
            self.last_error =
                Some("Save the `.tf` file to generate the graph.".into());
            cx.notify();
            return;
        };

        let Some(cwd) = terraform_path
            .parent()
            .map(|parent| parent.to_path_buf())
        else {
            self.last_error = Some(
                "Could not determine Terraform working directory from the file path.".into(),
            );
            cx.notify();
            return;
        };

        self.graph_generation = self.graph_generation.wrapping_add(1);
        let generation = self.graph_generation;

        self.graph_task = Some(cx.spawn(async move |graph_view, cx| {
            let result = {
                let cwd = cwd.clone();
                let executor = cx.background_executor().clone();
                executor
                    .await_on_background(async move { run_terraform_graph(&cwd).await })
                    .await
            };

            let Some(_) = graph_view.upgrade() else {
                return;
            };

            let _ = graph_view.update(cx, |view, cx| {
                if view.graph_generation != generation {
                    return;
                }

                match result {
                    Ok(dot) => match parse_dot_to_digraph(&dot)
                        .and_then(|parsed| layout_flow_graph(&parsed.graph))
                    {
                        Ok(model) => {
                            view.flow_state.update(cx, |state, _| {
                                state.set_nodes(model.nodes);
                                state.set_edges(model.edges);
                            });
                            view.last_error = None;
                            view.pending_fit_view = true;
                        }
                        Err(error) => {
                            view.last_error = Some(error.to_string().into());
                        }
                    },
                    Err(error) => {
                        view.last_error = Some(error.to_string().into());
                    }
                }

                cx.notify();
            });
        }));
    }
}

impl Focusable for GraphView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for GraphView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let container_width = viewport.width.as_f32();
        let container_height = viewport.height.as_f32();

        let size_changed = self.last_container.is_none_or(|(width, height)| {
            (width - container_width).abs() > 0.5 || (height - container_height).abs() > 0.5
        });

        if self.pending_fit_view || size_changed {
            self.flow_state.update(cx, |state, _| {
                state.fit_view(40.0, container_width, container_height);
            });
            self.pending_fit_view = false;
            self.last_container = Some((container_width, container_height));
        }

        div()
            .size_full()
            .relative()
            .track_focus(&self.focus_handle)
            .child(self.flow_graph.clone())
            .when_some(self.last_error.clone(), |stack, message| {
                stack.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .p_4()
                        .bg(gpui::rgba(0x000000aa))
                        .child(
                            div()
                                .max_w(px(480.))
                                .p_4()
                                .rounded_md()
                                .border_1()
                                .border_color(gpui::rgb(0x3f3f46))
                                .bg(gpui::rgb(0x18181b))
                                .text_color(gpui::rgb(0xf4f4f5))
                                .text_sm()
                                .child(message),
                        ),
                )
            })
    }
}
