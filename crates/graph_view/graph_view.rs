use gpui::AppContext;

mod dot_layout;

pub use dot_layout::{is_dag, parse_dot_to_digraph, ParsedDot};

pub fn init(_cx: &mut impl AppContext) {}
