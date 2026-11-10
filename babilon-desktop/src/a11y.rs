//! AccessKit plumbing: focusable read-only boxes, live region, focus order.
//!
//! Bevy 0.16 only auto-creates AccessKit nodes for `Button` / `Image` /
//! `Label` entities — a bare `Text` is invisible to JAWS/NVDA. The boxes
//! below are read-only `MultilineTextInput` nodes: focusable *and*
//! readable, so screen readers can browse and move focus onto each line.
use accesskit::{Action, Live, Role};
use bevy::a11y::{AccessibilityNode, ActionRequest as A11yActionRequest};
use bevy::input_focus::InputFocus;
use bevy::prelude::*;

/// Read-only edit box for one side-panel line.
///
/// Bevy 0.16 only auto-creates AccessKit nodes for `Button` / `Image` /
/// `Label` entities — a bare `Text` is invisible to JAWS/NVDA. A `Label`
/// node (`Role::Label`) is static text: screen readers can browse it with
/// the virtual cursor but it never takes focus, so Tab/focus navigation
/// skips it and the tree has no focusable leaves. A read-only
/// `MultilineTextInput` is focusable *and* readable (`value` = visible
/// text), so JAWS/NVDA can both browse and move focus onto each line —
/// a working accessible focus tree.
///
/// No `Label` component is attached on purpose: `bevy_ui::label_changed`
/// resets any explicit node on a `Label` entity back to `Role::Label` in
/// `PostUpdate`. A bare explicit `AccessibilityNode` still joins the tree
/// via its `ChildOf` chain, and keeps whatever role we give it.
/// The node value is kept in sync with the `Text` in `refresh_ui`.
pub(crate) fn readonly_box(label: &str, value: &str) -> AccessibilityNode {
    let mut node = accesskit::Node::new(Role::MultilineTextInput);
    node.set_label(label.to_string().into_boxed_str());
    node.set_value(value.to_string().into_boxed_str());
    node.set_read_only();
    node.add_action(Action::Focus);
    AccessibilityNode::from(node)
}

/// Live announcement box for the status line ("Acum lovește!", scores…).
///
/// Same read-only edit box as above, plus `Live::Polite` + atomic so
/// NVDA/JAWS speak every update without moving focus, like an ARIA live
/// region. Focusable too: `I` moves Bevy's `InputFocus` (mirrored to
/// AccessKit focus) here from the keyboard side, and a screen reader can
/// move focus onto it from its side.
pub(crate) fn live_box(label: &str, value: &str) -> AccessibilityNode {
    let mut node = accesskit::Node::new(Role::MultilineTextInput);
    node.set_label(label.to_string().into_boxed_str());
    node.set_value(value.to_string().into_boxed_str());
    node.set_read_only();
    node.set_live(Live::Polite);
    node.set_live_atomic();
    node.add_action(Action::Focus);
    AccessibilityNode::from(node)
}

/// Handles of the side info block, so a key press can move screen-reader
/// focus through it (`I` cycles, `Esc` back to the game).
///
/// `order` is the full Tab/focus order (visual order); `status` is the
/// live announcements box where the cycle starts.
#[derive(Resource)]
pub(crate) struct InfoPanel {
    pub(crate) status: Entity,
    pub(crate) order: Vec<Entity>,
}

pub(crate) fn set_node_value(node: &mut AccessibilityNode, text: &str) {
    node.set_value(text.to_string().into_boxed_str());
}

pub(crate) fn info_focus_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut focus: ResMut<InputFocus>,
    panel: Option<Res<InfoPanel>>,
) {
    if keys.just_pressed(KeyCode::KeyI) {
        if let Some(p) = panel {
            if p.order.is_empty() {
                return;
            }
            let next = match focus.get() {
                Some(cur) => p
                    .order
                    .iter()
                    .position(|&e| e == cur)
                    .map(|i| p.order[(i + 1) % p.order.len()])
                    .unwrap_or(p.status),
                None => p.status,
            };
            focus.set(next);
        }
    } else if keys.just_pressed(KeyCode::Escape) {
        focus.clear();
    }
}

/// Screen-reader → game focus requests.
///
/// When JAWS/NVDA itself moves (virtual cursor, touch exploration), winit
/// forwards an AccessKit `Focus` action as an `ActionRequest` event — honor
/// it so focus stays in sync both ways. Unknown/stale targets are ignored.
pub(crate) fn a11y_focus_requests(
    mut events: EventReader<A11yActionRequest>,
    mut focus: ResMut<InputFocus>,
    nodes: Query<(), With<AccessibilityNode>>,
) {
    for req in events.read() {
        if req.action == Action::Focus {
            let e = Entity::from_bits(req.target.0);
            if nodes.contains(e) {
                focus.set(e);
            }
        }
    }
}

pub(crate) fn refresh_box(node: &mut AccessibilityNode, label: &str, value: &str, live: bool) {
    // Self-healing: re-assert the read-only-box shape every refresh so the
    // focus tree stays intact even if another system touched the node.
    node.set_role(Role::MultilineTextInput);
    node.set_label(label.to_string().into_boxed_str());
    node.set_read_only();
    node.add_action(Action::Focus);
    if live {
        node.set_live(Live::Polite);
        node.set_live_atomic();
    }
    set_node_value(node, value);
}
