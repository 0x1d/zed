//! Terraform dependency graph: DOT parsing, layout, subprocess, and optional Zed editor integration.

mod dot_layout;
mod flow_state_config;
mod layout_settings;
mod node_ui;
mod terraform_graph;

#[cfg(feature = "editor")]
mod editor;

#[cfg(feature = "editor")]
pub use editor::{
    init, GraphView, OpenPreview, OpenPreviewToTheSide, RefreshGraph, terraform_file_path,
    ToggleDependencyFlow, ToggleLayoutDirection,
};

pub use dot_layout::{
    FlowGraphModel, ParsedDot, TerraformDependencyFlow, TerraformLabelParts,
    TerraformLayoutDirection, TerraformLayoutOptions, is_dag, layout_flow_graph,
    layout_flow_graph_with_options, layout_flow_graph_with_options_and_sizes,
    parse_dot_to_digraph, terraform_display_label, terraform_label_parts,
};
pub use layout_settings::{load_layout_options, save_layout_options};
pub use node_ui::flow_graph_node_renderer;
pub use flow_state_config::configure_flow_state_for_fit;
pub use terraform_graph::run_terraform_graph;
