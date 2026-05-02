//! Viewport limits so `FlowState::fit_view` can zoom out enough for wide graphs.
//!
//! `gpui_flow` defaults `min_zoom` to **0.8**, which caps zoom-out and causes overlap when the
//! bounding box is wider than ~1.25× the panel width.

use gpui_flow::FlowState;

pub fn configure_flow_state_for_fit(state: &mut FlowState) {
    state.min_zoom = 0.05;
    state.max_zoom = 4.0;
}
