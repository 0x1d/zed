//! Terraform dependency graph: DOT parsing, layout, subprocess, and optional Zed editor integration.

mod dot_layout;
mod flow_state_config;
mod node_ui;
mod terraform_graph;

#[cfg(feature = "editor")]
mod editor;

#[cfg(feature = "editor")]
pub use editor::{
    init, GraphView, OpenPreview, OpenPreviewToTheSide, RefreshGraph, terraform_file_path,
};

pub use dot_layout::{
    is_dag, layout_flow_graph, parse_dot_to_digraph, terraform_display_label, terraform_label_parts,
    FlowGraphModel, ParsedDot, TerraformLabelParts,
};
pub use node_ui::flow_graph_node_renderer;
pub use flow_state_config::configure_flow_state_for_fit;
pub use terraform_graph::run_terraform_graph;
