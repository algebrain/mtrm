use super::*;
use proptest::prelude::*;

#[test]
fn new_layout_contains_single_root_pane() {
    let layout = LayoutTree::new(PaneId::new(1));

    assert_eq!(layout.focused_pane(), PaneId::new(1));
    assert_eq!(layout.pane_ids(), vec![PaneId::new(1)]);
    assert!(layout.contains(PaneId::new(1)));
}

#[test]
fn split_focused_vertical_creates_second_pane_and_focuses_it() {
    let mut layout = LayoutTree::new(PaneId::new(1));

    let new_pane = layout.split_focused(SplitDirection::Vertical, PaneId::new(2));

    assert_eq!(new_pane, PaneId::new(2));
    assert_eq!(layout.focused_pane(), PaneId::new(2));
    assert_eq!(layout.pane_ids(), vec![PaneId::new(1), PaneId::new(2)]);
}

#[test]
fn split_focused_horizontal_creates_expected_rectangles() {
    let mut layout = LayoutTree::new(PaneId::new(1));
    layout.split_focused(SplitDirection::Horizontal, PaneId::new(2));

    let placements = layout.placements(Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 20,
    });

    assert_eq!(placements.len(), 2);
    assert_eq!(
        placements[0].rect,
        Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 10
        }
    );
    assert_eq!(
        placements[1].rect,
        Rect {
            x: 0,
            y: 10,
            width: 80,
            height: 10
        }
    );
}

#[test]
fn split_with_same_direction_flattens_into_one_container() {
    let mut layout = LayoutTree::new(PaneId::new(1));
    layout.split_focused(SplitDirection::Vertical, PaneId::new(2));
    layout.split_focused(SplitDirection::Vertical, PaneId::new(3));

    match &layout.root {
        LayoutNode::Split {
            direction,
            children,
        } => {
            assert_eq!(*direction, SplitDirection::Vertical);
            assert_eq!(children.len(), 3);
            assert!(matches!(children[0].node, LayoutNode::Pane(id) if id == PaneId::new(1)));
            assert!(matches!(children[1].node, LayoutNode::Pane(id) if id == PaneId::new(2)));
            assert!(matches!(children[2].node, LayoutNode::Pane(id) if id == PaneId::new(3)));
        }
        other => panic!("expected root split, got {other:?}"),
    }
}

#[test]
fn split_with_other_direction_creates_nested_container() {
    let mut layout = LayoutTree::new(PaneId::new(1));
    layout.split_focused(SplitDirection::Vertical, PaneId::new(2));
    layout.split_focused(SplitDirection::Horizontal, PaneId::new(3));

    match &layout.root {
        LayoutNode::Split {
            direction,
            children,
        } => {
            assert_eq!(*direction, SplitDirection::Vertical);
            assert_eq!(children.len(), 2);
            assert!(matches!(children[0].node, LayoutNode::Pane(id) if id == PaneId::new(1)));
            match &children[1].node {
                LayoutNode::Split {
                    direction,
                    children,
                } => {
                    assert_eq!(*direction, SplitDirection::Horizontal);
                    assert_eq!(children.len(), 2);
                }
                other => panic!("expected nested split, got {other:?}"),
            }
        }
        other => panic!("expected root split, got {other:?}"),
    }
}

#[test]
fn resize_right_changes_boundary_by_one_cell() {
    let mut layout = LayoutTree::new(PaneId::new(1));
    layout.split_focused(SplitDirection::Vertical, PaneId::new(2));
    layout.focus_pane(PaneId::new(1)).unwrap();
    let area = Rect {
        x: 0,
        y: 0,
        width: 40,
        height: 12,
    };

    let before = layout.placements(area);
    let changed = layout.resize_focused(ResizeDirection::Right, area).unwrap();
    let after = layout.placements(area);

    assert!(changed);
    assert_eq!(after[0].rect.width, before[0].rect.width + 1);
    assert_eq!(after[1].rect.width + 1, before[1].rect.width);
}

#[test]
fn resize_left_after_resize_right_reverts_boundary_by_one_cell() {
    let mut layout = LayoutTree::new(PaneId::new(1));
    layout.split_focused(SplitDirection::Vertical, PaneId::new(2));
    layout.focus_pane(PaneId::new(1)).unwrap();
    let area = Rect {
        x: 0,
        y: 0,
        width: 40,
        height: 12,
    };

    let before = layout.placements(area);
    assert!(layout.resize_focused(ResizeDirection::Right, area).unwrap());
    let changed = layout.resize_focused(ResizeDirection::Left, area).unwrap();
    let after = layout.placements(area);

    assert!(changed);
    assert_eq!(after[0].rect.width, before[0].rect.width);
    assert_eq!(after[1].rect.width, before[1].rect.width);
}

#[test]
fn resize_respects_minimum_pane_width() {
    let mut layout = LayoutTree::new(PaneId::new(1));
    layout.split_focused(SplitDirection::Vertical, PaneId::new(2));
    layout.focus_pane(PaneId::new(1)).unwrap();
    let area = Rect {
        x: 0,
        y: 0,
        width: 16,
        height: 12,
    };

    let changed = layout.resize_focused(ResizeDirection::Right, area).unwrap();

    assert!(!changed);
}

#[test]
fn close_focused_removes_pane_and_focuses_sibling() {
    let mut layout = LayoutTree::new(PaneId::new(1));
    layout.split_focused(SplitDirection::Vertical, PaneId::new(2));

    let closed = layout.close_focused().unwrap();

    assert_eq!(closed, PaneId::new(2));
    assert_eq!(layout.focused_pane(), PaneId::new(1));
    assert_eq!(layout.pane_ids(), vec![PaneId::new(1)]);
}

#[test]
fn close_focused_collapses_degenerate_container() {
    let mut layout = LayoutTree::new(PaneId::new(1));
    layout.split_focused(SplitDirection::Vertical, PaneId::new(2));
    layout.split_focused(SplitDirection::Horizontal, PaneId::new(3));

    let closed = layout.close_focused().unwrap();

    assert_eq!(closed, PaneId::new(3));
    assert_eq!(layout.focused_pane(), PaneId::new(2));
    match &layout.root {
        LayoutNode::Split {
            direction,
            children,
        } => {
            assert_eq!(*direction, SplitDirection::Vertical);
            assert_eq!(children.len(), 2);
        }
        other => panic!("expected root split, got {other:?}"),
    }
}

#[test]
fn close_last_pane_is_rejected() {
    let mut layout = LayoutTree::new(PaneId::new(1));

    let error = layout.close_focused().unwrap_err();

    assert_eq!(error, LayoutError::CannotCloseLastPane);
}

#[test]
fn focus_pane_rejects_unknown_pane() {
    let mut layout = LayoutTree::new(PaneId::new(1));

    let error = layout.focus_pane(PaneId::new(99)).unwrap_err();

    assert_eq!(error, LayoutError::PaneNotFound(PaneId::new(99)));
}

#[test]
fn move_focus_uses_direction_on_composite_layout() {
    let mut layout = LayoutTree::new(PaneId::new(1));
    layout.split_focused(SplitDirection::Vertical, PaneId::new(2));
    layout.split_focused(SplitDirection::Horizontal, PaneId::new(3));

    layout.focus_pane(PaneId::new(3)).unwrap();
    assert_eq!(
        layout.move_focus(FocusMoveDirection::Left).unwrap(),
        PaneId::new(1)
    );
    assert_eq!(
        layout.move_focus(FocusMoveDirection::Right).unwrap(),
        PaneId::new(2)
    );
    assert_eq!(
        layout.move_focus(FocusMoveDirection::Down).unwrap(),
        PaneId::new(3)
    );
}

#[test]
fn placements_return_all_panes_without_duplicates() {
    let mut layout = LayoutTree::new(PaneId::new(1));
    layout.split_focused(SplitDirection::Vertical, PaneId::new(2));
    layout.split_focused(SplitDirection::Horizontal, PaneId::new(3));

    let placements = layout.placements(Rect {
        x: 0,
        y: 0,
        width: 120,
        height: 40,
    });

    let pane_ids: Vec<_> = placements
        .iter()
        .map(|placement| placement.pane_id)
        .collect();

    assert_eq!(pane_ids.len(), 3);
    assert!(pane_ids.contains(&PaneId::new(1)));
    assert!(pane_ids.contains(&PaneId::new(2)));
    assert!(pane_ids.contains(&PaneId::new(3)));
}

#[test]
fn snapshot_roundtrip_preserves_structure_and_focus() {
    let mut layout = LayoutTree::new(PaneId::new(1));
    layout.split_focused(SplitDirection::Vertical, PaneId::new(2));
    layout.split_focused(SplitDirection::Horizontal, PaneId::new(3));
    layout.focus_pane(PaneId::new(2)).unwrap();

    let snapshot = layout.to_snapshot();
    let restored = LayoutTree::from_snapshot(snapshot).unwrap();

    assert_eq!(restored.focused_pane(), PaneId::new(2));
    assert_eq!(restored.pane_ids(), layout.pane_ids());
    assert_eq!(
        restored.placements(Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 50,
        }),
        layout.placements(Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 50,
        })
    );
    assert_normalized(&restored.root);
}

#[test]
fn normalize_flattens_same_direction_levels_from_binary_snapshot() {
    let snapshot: LayoutSnapshot = serde_yaml::from_str(
            "root:\n  Split:\n    direction: Vertical\n    first:\n      Pane:\n        pane_id: 1\n    second:\n      Split:\n        direction: Vertical\n        first:\n          Pane:\n            pane_id: 2\n        second:\n          Pane:\n            pane_id: 3\nfocused_pane: 1\n",
        )
        .unwrap();

    let restored = LayoutTree::from_snapshot(snapshot).unwrap();

    match &restored.root {
        LayoutNode::Split {
            direction,
            children,
        } => {
            assert_eq!(*direction, SplitDirection::Vertical);
            assert_eq!(children.len(), 3);
        }
        other => panic!("expected root split, got {other:?}"),
    }
}

fn assert_normalized(node: &LayoutNode) {
    match node {
        LayoutNode::Pane(_) => {}
        LayoutNode::Split {
            direction,
            children,
        } => {
            assert!(children.len() >= 2);
            for child in children {
                match &child.node {
                    LayoutNode::Split {
                        direction: child_direction,
                        ..
                    } => assert_ne!(direction, child_direction),
                    LayoutNode::Pane(_) => {}
                }
                assert_normalized(&child.node);
            }
        }
    }
}

prop_compose! {
    fn layout_operations()(ops in prop::collection::vec(
        prop_oneof![
            Just((0_u8, SplitDirection::Vertical)),
            Just((0_u8, SplitDirection::Horizontal)),
            Just((1_u8, SplitDirection::Vertical)),
        ],
        1..32,
    )) -> Vec<(u8, SplitDirection)> {
        ops
    }
}

proptest! {
    #[test]
    fn tree_remains_valid_after_random_splits_and_closes(ops in layout_operations()) {
        let mut layout = LayoutTree::new(PaneId::new(1));
        let mut next_pane_id = 2_u64;

        for (kind, direction) in ops {
            match kind {
                0 => {
                    layout.split_focused(direction, PaneId::new(next_pane_id));
                    next_pane_id += 1;
                }
                _ => {
                    let _ = layout.close_focused();
                }
            }

            let pane_ids = layout.pane_ids();
            prop_assert!(!pane_ids.is_empty());
            prop_assert!(pane_ids.contains(&layout.focused_pane()));

            let placements = layout.placements(Rect {
                x: 0,
                y: 0,
                width: 120,
                height: 60,
            });

            prop_assert_eq!(placements.len(), pane_ids.len());
            assert_normalized(&layout.root);

            let mut seen = std::collections::BTreeSet::new();
            for placement in placements {
                prop_assert!(seen.insert(placement.pane_id));
            }
        }
    }
}
