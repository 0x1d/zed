use gpui::{AnyElement, App, FontWeight, SharedString, Window, div, prelude::*, px};

use gpui_flow::FlowNode;

/// Resource type line — subdued caption (Bench-like mono hint on second line).
const TYPE_COLOR: u32 = 0x71717a;
/// Resource name — primary emphasis.
const NAME_COLOR: u32 = 0x171717;

/// Two-line Terraform labels: `resource_type` then `name` (encoded as `type\\nname` on [`FlowNode::label`]).
pub fn flow_graph_node_renderer(node: &FlowNode, _window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = node.label.as_ref();
    if label.is_empty() {
        return div()
            .text_sm()
            .text_color(gpui::rgb(NAME_COLOR))
            .child(node.id.to_string())
            .into_any_element();
    }

    let parts: Vec<&str> = label.splitn(2, '\n').collect();
    match parts.as_slice() {
        [type_line, name_line] if !name_line.is_empty() => div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .w_full()
            .min_w_0()
            .px(px(4.0))
            .child(
                div()
                    .text_xs()
                    .italic()
                    .opacity(0.92)
                    .text_color(gpui::rgb(TYPE_COLOR))
                    .text_center()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .w_full()
                    .child(SharedString::from(*type_line)),
            )
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(gpui::rgb(NAME_COLOR))
                    .text_center()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .w_full()
                    .child(SharedString::from(*name_line)),
            )
            .into_any_element(),
        _ => div()
            .text_sm()
            .text_color(gpui::rgb(NAME_COLOR))
            .text_center()
            .whitespace_nowrap()
            .overflow_hidden()
            .w_full()
            .px(px(4.0))
            .child(SharedString::from(label.to_string()))
            .into_any_element(),
    }
}
