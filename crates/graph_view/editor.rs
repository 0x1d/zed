use std::any::TypeId;
use std::path::PathBuf;

use anyhow::Result;
use editor::{Editor, EditorEvent};
use gpui::{
    App, AnyEntity, Context, Entity, EventEmitter, FocusHandle, Focusable, ParentElement as _,
    Render, SharedString, Styled as _, Subscription, Task, Window, div, px,
    prelude::FluentBuilder as _,
};
use gpui_flow::{BackgroundPattern, FlowGraph, FlowState};
use project::Project;
use ui::prelude::*;
use workspace::item::{Item, ItemBufferKind, SaveOptions};
use workspace::{Pane, SplitDirection, Workspace};
pub use zed_actions::preview::terraform::{OpenPreview, OpenPreviewToTheSide, RefreshGraph};

use crate::{layout_flow_graph, parse_dot_to_digraph, run_terraform_graph};

const FLOW_BG: u32 = 0xf8f8f8;
const FLOW_GRID: u32 = 0xd4d4d4;
const FLOW_NODE_BG: u32 = 0xffffff;
const FLOW_NODE_BORDER: u32 = 0xe2e2e2;

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };
        GraphView::register(workspace, window, cx);
    })
    .detach();
}

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

    pub fn register(workspace: &mut Workspace, _window: &mut Window, _cx: &mut Context<Workspace>) {
        workspace.register_action(move |workspace, _: &OpenPreview, window, cx| {
            if let Some(editor) = Self::resolve_active_item_as_terraform_editor(workspace, cx) {
                let graph_view = Self::new(&editor, cx);
                workspace.active_pane().update(cx, |pane, cx| {
                    if let Some(existing_idx) =
                        Self::find_existing_graph_item_idx(pane, &editor, cx)
                    {
                        pane.activate_item(existing_idx, true, true, window, cx);
                    } else {
                        pane.add_item(Box::new(graph_view), true, true, None, window, cx);
                    }
                });
                cx.notify();
            }
        });

        workspace.register_action(move |workspace, _: &OpenPreviewToTheSide, window, cx| {
            if let Some(editor) = Self::resolve_active_item_as_terraform_editor(workspace, cx) {
                let graph_view = Self::new(&editor, cx);
                let pane = workspace
                    .find_pane_in_direction(SplitDirection::Right, cx)
                    .unwrap_or_else(|| {
                        workspace.split_pane(
                            workspace.active_pane().clone(),
                            SplitDirection::Right,
                            window,
                            cx,
                        )
                    });
                pane.update(cx, |pane, cx| {
                    if let Some(existing_idx) = Self::find_existing_graph_item_idx(pane, &editor, cx)
                    {
                        pane.activate_item(existing_idx, true, true, window, cx);
                    } else {
                        pane.add_item(Box::new(graph_view), false, false, None, window, cx);
                    }
                });
                editor.focus_handle(cx).focus(window, cx);
                cx.notify();
            }
        });

        workspace.register_action(move |workspace, _: &RefreshGraph, _window, cx| {
            if let Some(graph) = workspace
                .active_item(cx)
                .and_then(|item| item.act_as::<GraphView>(cx))
            {
                graph.update(cx, |view, cx| {
                    view.refresh(cx);
                });
            }
        });
    }

    fn find_existing_graph_item_idx(
        pane: &Pane,
        editor: &Entity<Editor>,
        cx: &App,
    ) -> Option<usize> {
        pane.items_of_type::<GraphView>()
            .find(|view| view.read(cx).source_editor == *editor)
            .and_then(|view| pane.index_for_item(&view))
    }

    pub fn resolve_active_item_as_terraform_editor(
        workspace: &Workspace,
        cx: &mut Context<Workspace>,
    ) -> Option<Entity<Editor>> {
        let editor = workspace
            .active_item(cx)
            .and_then(|item| item.act_as::<Editor>(cx))?;
        terraform_file_path(&editor, cx).map(|_| editor)
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
            self.last_error = Some("Save the `.tf` file to generate the graph.".into());
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

            graph_view
                .update(cx, |view, cx| {
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
                })
                .ok();
        }));
    }
}

impl Focusable for GraphView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<()> for GraphView {}

impl Item for GraphView {
    type Event = ();

    fn act_as_type<'a>(
        &'a self,
        type_id: TypeId,
        self_handle: &'a Entity<Self>,
        _: &'a App,
    ) -> Option<AnyEntity> {
        if type_id == TypeId::of::<Self>() {
            Some(self_handle.clone().into())
        } else if type_id == TypeId::of::<Editor>() {
            Some(self.source_editor.clone().into())
        } else {
            None
        }
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::GitGraph))
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        let buffer = self.source_editor.read(cx).buffer().read(cx);
        let title = buffer.title(cx);
        format!("Graph · {title}").into()
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("Terraform graph preview")
    }

    fn can_save(&self, cx: &App) -> bool {
        self.source_editor.read(cx).can_save(cx)
    }

    fn can_save_as(&self, cx: &App) -> bool {
        self.source_editor.read(cx).can_save_as(cx)
    }

    fn save(
        &mut self,
        options: SaveOptions,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.source_editor
            .update(cx, |editor, cx| editor.save(options, project, window, cx))
    }

    fn save_as(
        &mut self,
        project: Entity<Project>,
        path: project::ProjectPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.source_editor
            .update(cx, |editor, cx| editor.save_as(project, path, window, cx))
    }

    fn to_item_events(_event: &Self::Event, _f: &mut dyn FnMut(workspace::item::ItemEvent)) {}

    fn buffer_kind(&self, _cx: &App) -> ItemBufferKind {
        ItemBufferKind::Singleton
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
            .on_action(cx.listener(|this: &mut GraphView, _: &RefreshGraph, window, cx| {
                this.refresh(cx);
                cx.notify();
                let handle = this.focus_handle.clone();
                handle.focus(window, cx);
            }))
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
