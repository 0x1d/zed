use gpui::{
    AnyElement, App, FontWeight, SharedString, TextOverflow, Window, div, prelude::*, px, relative,
};

use gpui_flow::FlowNode;

/// Resource type line — subdued caption (Bench-like mono hint on second line).
const TYPE_COLOR: u32 = 0x71717a;
/// Resource name — primary emphasis.
const NAME_COLOR: u32 = 0x000000;

/// Two-line Terraform labels: `resource_type` then `name` (encoded as `type\\nname` on [`FlowNode::label`]).
pub fn flow_graph_node_renderer(
    node: &FlowNode,
    _window: &mut Window,
    _cx: &mut App,
) -> AnyElement {
    let label = node.label.as_ref();
    if label.is_empty() {
        return div()
            .text_sm()
            .line_height(relative(1.1))
            .text_color(gpui::rgb(NAME_COLOR))
            .min_w_0()
            .max_w_full()
            .overflow_x_hidden()
            .whitespace_nowrap()
            .text_overflow(TextOverflow::Truncate("".into()))
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
            .max_w_full()
            .overflow_x_hidden()
            .px(px(4.0))
            .child(
                div()
                    .text_xs()
                    .line_height(relative(1.1))
                    .italic()
                    .opacity(0.92)
                    .text_color(gpui::rgb(TYPE_COLOR))
                    .text_center()
                    .min_w_0()
                    .max_w_full()
                    .whitespace_nowrap()
                    .overflow_x_hidden()
                    .text_overflow(TextOverflow::Truncate("".into()))
                    .w_full()
                    .child(SharedString::from(*type_line)),
            )
            .child(
                div()
                    .text_sm()
                    .line_height(relative(1.1))
                    .font_weight(FontWeight::BOLD)
                    .text_color(gpui::rgb(NAME_COLOR))
                    .text_center()
                    .min_w_0()
                    .max_w_full()
                    .whitespace_nowrap()
                    .overflow_x_hidden()
                    .text_overflow(TextOverflow::Truncate("".into()))
                    .w_full()
                    .child(SharedString::from(*name_line)),
            )
            .into_any_element(),
        _ => div()
            .text_sm()
            .line_height(relative(1.1))
            .text_color(gpui::rgb(NAME_COLOR))
            .text_center()
            .min_w_0()
            .max_w_full()
            .whitespace_nowrap()
            .overflow_x_hidden()
            .text_overflow(TextOverflow::Truncate("".into()))
            .w_full()
            .px(px(4.0))
            .child(SharedString::from(label.to_string()))
            .into_any_element(),
    }
}
