//! Раскладка окон: дерево разбиений, фокус и операции над ним.

use mtrm_core::{FocusMoveDirection, PaneId, SplitDirection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    EmptyLayout,
    PaneNotFound(PaneId),
    CannotSplitMissingPane(PaneId),
    CannotCloseLastPane,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanePlacement {
    pub pane_id: PaneId,
    pub rect: Rect,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutSnapshot {
    root: LayoutNodeSnapshot,
    focused_pane: PaneId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum LayoutNodeSnapshot {
    Pane {
        pane_id: PaneId,
    },
    Split {
        direction: SplitDirection,
        first: Box<LayoutNodeSnapshot>,
        second: Box<LayoutNodeSnapshot>,
    },
}

#[derive(Debug, Clone)]
pub struct LayoutTree {
    root: LayoutNode,
    focused_pane: PaneId,
}

#[derive(Debug, Clone)]
enum LayoutNode {
    Pane(PaneId),
    Split {
        direction: SplitDirection,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

impl LayoutTree {
    pub fn new(root_pane: PaneId) -> Self {
        Self {
            root: LayoutNode::Pane(root_pane),
            focused_pane: root_pane,
        }
    }

    pub fn focused_pane(&self) -> PaneId {
        self.focused_pane
    }

    pub fn contains(&self, pane_id: PaneId) -> bool {
        self.root.contains(pane_id)
    }

    pub fn split_focused(&mut self, direction: SplitDirection, new_pane: PaneId) -> PaneId {
        let focused = self.focused_pane;
        let replaced = self
            .root
            .replace_pane_with_split(focused, direction, new_pane);
        debug_assert!(replaced, "focused pane must exist in layout");
        self.focused_pane = new_pane;
        new_pane
    }

    pub fn close_focused(&mut self) -> Result<PaneId, LayoutError> {
        if matches!(self.root, LayoutNode::Pane(_)) {
            return Err(LayoutError::CannotCloseLastPane);
        }

        let pane_to_close = self.focused_pane;
        let new_focus = self
            .root
            .close_pane(pane_to_close)
            .ok_or(LayoutError::PaneNotFound(pane_to_close))?;
        self.focused_pane = new_focus;
        Ok(pane_to_close)
    }

    pub fn focus_pane(&mut self, pane_id: PaneId) -> Result<(), LayoutError> {
        if self.contains(pane_id) {
            self.focused_pane = pane_id;
            Ok(())
        } else {
            Err(LayoutError::PaneNotFound(pane_id))
        }
    }

    pub fn move_focus(&mut self, direction: FocusMoveDirection) -> Result<PaneId, LayoutError> {
        let placements = self.placements(Rect {
            x: 0,
            y: 0,
            width: 1200,
            height: 800,
        });
        let current = placements
            .iter()
            .find(|placement| placement.pane_id == self.focused_pane)
            .ok_or(LayoutError::PaneNotFound(self.focused_pane))?;

        let target = placements
            .iter()
            .filter(|candidate| candidate.pane_id != self.focused_pane)
            .filter(|candidate| is_in_direction(current.rect, candidate.rect, direction))
            .min_by_key(|candidate| focus_score(current.rect, candidate.rect, direction));

        if let Some(target) = target {
            self.focused_pane = target.pane_id;
        }

        Ok(self.focused_pane)
    }

    pub fn pane_ids(&self) -> Vec<PaneId> {
        let mut pane_ids = Vec::new();
        self.root.collect_panes(&mut pane_ids);
        pane_ids
    }

    pub fn placements(&self, area: Rect) -> Vec<PanePlacement> {
        let mut placements = Vec::new();
        self.root
            .collect_placements(area, self.focused_pane, &mut placements);
        placements
    }

    pub fn to_snapshot(&self) -> LayoutSnapshot {
        LayoutSnapshot {
            root: self.root.to_snapshot(),
            focused_pane: self.focused_pane,
        }
    }

    pub fn from_snapshot(snapshot: LayoutSnapshot) -> Result<Self, LayoutError> {
        let root = LayoutNode::from_snapshot(snapshot.root)?;
        if !root.contains(snapshot.focused_pane) {
            return Err(LayoutError::PaneNotFound(snapshot.focused_pane));
        }

        Ok(Self {
            root,
            focused_pane: snapshot.focused_pane,
        })
    }
}

impl LayoutNode {
    fn contains(&self, pane_id: PaneId) -> bool {
        match self {
            Self::Pane(id) => *id == pane_id,
            Self::Split { first, second, .. } => {
                first.contains(pane_id) || second.contains(pane_id)
            }
        }
    }

    fn replace_pane_with_split(
        &mut self,
        pane_id: PaneId,
        direction: SplitDirection,
        new_pane: PaneId,
    ) -> bool {
        match self {
            Self::Pane(id) if *id == pane_id => {
                let old_pane = *id;
                *self = Self::Split {
                    direction,
                    first: Box::new(Self::Pane(old_pane)),
                    second: Box::new(Self::Pane(new_pane)),
                };
                true
            }
            Self::Pane(_) => false,
            Self::Split { first, second, .. } => {
                first.replace_pane_with_split(pane_id, direction, new_pane)
                    || second.replace_pane_with_split(pane_id, direction, new_pane)
            }
        }
    }

    fn close_pane(&mut self, pane_id: PaneId) -> Option<PaneId> {
        match self {
            Self::Pane(_) => None,
            Self::Split { first, second, .. } => {
                if matches!(first.as_ref(), Self::Pane(id) if *id == pane_id) {
                    let new_focus = second.first_pane_id()?;
                    let sibling = (**second).clone();
                    *self = sibling;
                    return Some(new_focus);
                }

                if matches!(second.as_ref(), Self::Pane(id) if *id == pane_id) {
                    let new_focus = first.first_pane_id()?;
                    let sibling = (**first).clone();
                    *self = sibling;
                    return Some(new_focus);
                }

                if let Some(new_focus) = first.close_pane(pane_id) {
                    return Some(new_focus);
                }

                if let Some(new_focus) = second.close_pane(pane_id) {
                    return Some(new_focus);
                }

                None
            }
        }
    }

    fn first_pane_id(&self) -> Option<PaneId> {
        match self {
            Self::Pane(id) => Some(*id),
            Self::Split { first, .. } => first.first_pane_id(),
        }
    }

    fn collect_panes(&self, pane_ids: &mut Vec<PaneId>) {
        match self {
            Self::Pane(id) => pane_ids.push(*id),
            Self::Split { first, second, .. } => {
                first.collect_panes(pane_ids);
                second.collect_panes(pane_ids);
            }
        }
    }

    fn collect_placements(&self, area: Rect, focused_pane: PaneId, out: &mut Vec<PanePlacement>) {
        match self {
            Self::Pane(pane_id) => out.push(PanePlacement {
                pane_id: *pane_id,
                rect: area,
                focused: *pane_id == focused_pane,
            }),
            Self::Split {
                direction,
                first,
                second,
            } => {
                let (first_area, second_area) = split_rect(area, *direction);
                first.collect_placements(first_area, focused_pane, out);
                second.collect_placements(second_area, focused_pane, out);
            }
        }
    }

    fn to_snapshot(&self) -> LayoutNodeSnapshot {
        match self {
            Self::Pane(pane_id) => LayoutNodeSnapshot::Pane { pane_id: *pane_id },
            Self::Split {
                direction,
                first,
                second,
            } => LayoutNodeSnapshot::Split {
                direction: *direction,
                first: Box::new(first.to_snapshot()),
                second: Box::new(second.to_snapshot()),
            },
        }
    }

    fn from_snapshot(snapshot: LayoutNodeSnapshot) -> Result<Self, LayoutError> {
        match snapshot {
            LayoutNodeSnapshot::Pane { pane_id } => Ok(Self::Pane(pane_id)),
            LayoutNodeSnapshot::Split {
                direction,
                first,
                second,
            } => Ok(Self::Split {
                direction,
                first: Box::new(Self::from_snapshot(*first)?),
                second: Box::new(Self::from_snapshot(*second)?),
            }),
        }
    }
}

fn split_rect(area: Rect, direction: SplitDirection) -> (Rect, Rect) {
    match direction {
        SplitDirection::Horizontal => {
            let first_height = area.height / 2;
            let second_height = area.height.saturating_sub(first_height);
            (
                Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: first_height,
                },
                Rect {
                    x: area.x,
                    y: area.y.saturating_add(first_height),
                    width: area.width,
                    height: second_height,
                },
            )
        }
        SplitDirection::Vertical => {
            let first_width = area.width / 2;
            let second_width = area.width.saturating_sub(first_width);
            (
                Rect {
                    x: area.x,
                    y: area.y,
                    width: first_width,
                    height: area.height,
                },
                Rect {
                    x: area.x.saturating_add(first_width),
                    y: area.y,
                    width: second_width,
                    height: area.height,
                },
            )
        }
    }
}

fn is_in_direction(current: Rect, candidate: Rect, direction: FocusMoveDirection) -> bool {
    match direction {
        FocusMoveDirection::Left => {
            candidate_right(candidate) <= current.x && overlaps_vertically(current, candidate)
        }
        FocusMoveDirection::Right => {
            current_right(current) <= candidate.x && overlaps_vertically(current, candidate)
        }
        FocusMoveDirection::Up => {
            candidate_bottom(candidate) <= current.y && overlaps_horizontally(current, candidate)
        }
        FocusMoveDirection::Down => {
            current_bottom(current) <= candidate.y && overlaps_horizontally(current, candidate)
        }
    }
}

fn focus_score(current: Rect, candidate: Rect, direction: FocusMoveDirection) -> (i32, i32, u16) {
    let primary = match direction {
        FocusMoveDirection::Left => i32::from(current.x) - i32::from(candidate_right(candidate)),
        FocusMoveDirection::Right => i32::from(candidate.x) - i32::from(current_right(current)),
        FocusMoveDirection::Up => i32::from(current.y) - i32::from(candidate_bottom(candidate)),
        FocusMoveDirection::Down => i32::from(candidate.y) - i32::from(current_bottom(current)),
    };

    let secondary = match direction {
        FocusMoveDirection::Left | FocusMoveDirection::Right => {
            center_distance(current.y, current.height, candidate.y, candidate.height)
        }
        FocusMoveDirection::Up | FocusMoveDirection::Down => {
            center_distance(current.x, current.width, candidate.x, candidate.width)
        }
    };

    let tertiary = match direction {
        FocusMoveDirection::Left | FocusMoveDirection::Right => candidate.y,
        FocusMoveDirection::Up | FocusMoveDirection::Down => candidate.x,
    };

    (primary, secondary, tertiary)
}

fn overlaps_horizontally(a: Rect, b: Rect) -> bool {
    let a_start = u32::from(a.x);
    let a_end = a_start + u32::from(a.width);
    let b_start = u32::from(b.x);
    let b_end = b_start + u32::from(b.width);
    a_start < b_end && b_start < a_end
}

fn overlaps_vertically(a: Rect, b: Rect) -> bool {
    let a_start = u32::from(a.y);
    let a_end = a_start + u32::from(a.height);
    let b_start = u32::from(b.y);
    let b_end = b_start + u32::from(b.height);
    a_start < b_end && b_start < a_end
}

fn current_right(rect: Rect) -> u16 {
    rect.x.saturating_add(rect.width)
}

fn candidate_right(rect: Rect) -> u16 {
    rect.x.saturating_add(rect.width)
}

fn current_bottom(rect: Rect) -> u16 {
    rect.y.saturating_add(rect.height)
}

fn candidate_bottom(rect: Rect) -> u16 {
    rect.y.saturating_add(rect.height)
}

fn center_distance(start_a: u16, len_a: u16, start_b: u16, len_b: u16) -> i32 {
    let center_a = i32::from(start_a) * 2 + i32::from(len_a);
    let center_b = i32::from(start_b) * 2 + i32::from(len_b);
    (center_a - center_b).abs()
}

#[cfg(test)]
mod tests {
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
    fn close_focused_removes_pane_and_focuses_sibling() {
        let mut layout = LayoutTree::new(PaneId::new(1));
        layout.split_focused(SplitDirection::Vertical, PaneId::new(2));

        let closed = layout.close_focused().unwrap();

        assert_eq!(closed, PaneId::new(2));
        assert_eq!(layout.focused_pane(), PaneId::new(1));
        assert_eq!(layout.pane_ids(), vec![PaneId::new(1)]);
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
        assert_eq!(
            pane_ids,
            vec![PaneId::new(1), PaneId::new(2), PaneId::new(3)]
        );
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
        assert_eq!(
            restored.pane_ids(),
            vec![PaneId::new(1), PaneId::new(2), PaneId::new(3)]
        );
        assert_eq!(
            restored.placements(Rect {
                x: 0,
                y: 0,
                width: 120,
                height: 40
            }),
            layout.placements(Rect {
                x: 0,
                y: 0,
                width: 120,
                height: 40
            })
        );
    }

    proptest! {
        #[test]
        fn tree_remains_valid_after_random_splits_and_closes(
            ops in proptest::collection::vec((0u8..2u8, 0u8..2u8), 1..64)
        ) {
            let mut layout = LayoutTree::new(PaneId::new(0));
            let mut next_pane_id = 1u64;

            for (op, direction_seed) in ops {
                if op % 2 == 0 {
                    let direction = if direction_seed % 2 == 0 {
                        SplitDirection::Horizontal
                    } else {
                        SplitDirection::Vertical
                    };
                    layout.split_focused(direction, PaneId::new(next_pane_id));
                    next_pane_id += 1;
                } else {
                    let _ = layout.close_focused();
                }

                let pane_ids = layout.pane_ids();
                prop_assert!(!pane_ids.is_empty());
                prop_assert!(layout.contains(layout.focused_pane()));

                let placements = layout.placements(Rect {
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 100,
                });

                prop_assert_eq!(placements.len(), pane_ids.len());

                let mut seen = std::collections::BTreeSet::new();
                for placement in placements {
                    prop_assert!(seen.insert(placement.pane_id));
                    prop_assert!(pane_ids.contains(&placement.pane_id));
                }
            }
        }
    }
}
