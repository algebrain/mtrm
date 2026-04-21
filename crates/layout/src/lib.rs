//! Раскладка окон: дерево разбиений, фокус и операции над ним.

mod geometry;
mod snapshot;

use mtrm_core::{FocusMoveDirection, PaneId, ResizeDirection, SplitDirection};
use serde::{Deserialize, Serialize};

use self::geometry::{
    find_resize_target, focus_score, is_in_direction, normalize, resize_axis_extent,
    resolve_resize_operation, sibling_focus_after_removal, split_rects,
};

const DEFAULT_CHILD_WEIGHT: u16 = 1;
const MIN_PANE_WIDTH: u16 = 8;
const MIN_PANE_HEIGHT: u16 = 4;

fn default_child_weight() -> u16 {
    DEFAULT_CHILD_WEIGHT
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum LayoutNodeSnapshot {
    Pane {
        pane_id: PaneId,
    },
    Split {
        direction: SplitDirection,
        children: Vec<LayoutSplitChildSnapshot>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LayoutSplitChildSnapshot {
    #[serde(default = "default_child_weight")]
    weight: u16,
    node: LayoutNodeSnapshot,
}

#[derive(Debug, Clone)]
pub struct LayoutTree {
    root: LayoutNode,
    focused_pane: PaneId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LayoutNode {
    Pane(PaneId),
    Split {
        direction: SplitDirection,
        children: Vec<SplitChild>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SplitChild {
    weight: u16,
    node: LayoutNode,
}

enum CloseOutcome {
    NotFound,
    Closed {
        replacement: Option<LayoutNode>,
        new_focus: PaneId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResizeTarget {
    path_to_split: Vec<usize>,
    child_index: usize,
    child_rects: Vec<Rect>,
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
        let outcome = self.root.close_pane(pane_to_close);
        let (replacement, new_focus) = match outcome {
            CloseOutcome::NotFound => return Err(LayoutError::PaneNotFound(pane_to_close)),
            CloseOutcome::Closed {
                replacement,
                new_focus,
            } => (replacement, new_focus),
        };

        if let Some(node) = replacement {
            self.root = node;
        }
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

    pub fn resize_focused(
        &mut self,
        direction: ResizeDirection,
        area: Rect,
    ) -> Result<bool, LayoutError> {
        if !self.root.contains(self.focused_pane) {
            return Err(LayoutError::PaneNotFound(self.focused_pane));
        }

        let Some(target) = find_resize_target(&self.root, area, self.focused_pane, direction)
        else {
            return Ok(false);
        };

        let Some((_, _, shrinks, grows)) = resolve_resize_operation(
            target.child_index,
            target.child_rects.len(),
            direction,
        )
        else {
            return Ok(false);
        };

        let mut extents: Vec<u16> = target
            .child_rects
            .iter()
            .map(|rect| resize_axis_extent(*rect, direction))
            .collect();

        if extents[shrinks] <= 1 {
            return Ok(false);
        }

        extents[shrinks] = extents[shrinks].saturating_sub(1);
        extents[grows] = extents[grows].saturating_add(1);

        let mut candidate = self.root.clone();
        let split = candidate
            .split_mut_at_path(&target.path_to_split)
            .expect("resize target must point to a split");
        let LayoutNode::Split { children, .. } = split else {
            unreachable!("resize target path must resolve to split node");
        };
        debug_assert_eq!(children.len(), extents.len());
        for (child, extent) in children.iter_mut().zip(extents) {
            child.weight = extent.max(DEFAULT_CHILD_WEIGHT);
        }

        let placements = {
            let mut placements = Vec::new();
            candidate.collect_placements(area, self.focused_pane, &mut placements);
            placements
        };
        if placements.iter().any(|placement| {
            placement.rect.width < MIN_PANE_WIDTH || placement.rect.height < MIN_PANE_HEIGHT
        }) {
            return Ok(false);
        }

        self.root = candidate;
        Ok(true)
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
        let root = normalize(LayoutNode::from_snapshot(snapshot.root)?);
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
            Self::Split { children, .. } => children.iter().any(|child| child.node.contains(pane_id)),
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
                    children: vec![
                        SplitChild::pane(old_pane),
                        SplitChild::pane(new_pane),
                    ],
                };
                true
            }
            Self::Pane(_) => false,
            Self::Split {
                direction: container_direction,
                children,
            } => {
                for index in 0..children.len() {
                    if !children[index].node.contains(pane_id) {
                        continue;
                    }

                    if matches!(children[index].node, Self::Pane(id) if id == pane_id) {
                        let existing = children.remove(index);
                        if *container_direction == direction {
                            let weight = existing.weight.max(DEFAULT_CHILD_WEIGHT);
                            children.insert(index, SplitChild::new(weight, Self::Pane(new_pane)));
                            children.insert(index, SplitChild::new(weight, existing.node));
                        } else {
                            children.insert(
                                index,
                                SplitChild::new(
                                    existing.weight.max(DEFAULT_CHILD_WEIGHT),
                                    Self::Split {
                                        direction,
                                        children: vec![
                                            SplitChild::pane(pane_id),
                                            SplitChild::pane(new_pane),
                                        ],
                                    },
                                ),
                            );
                        }
                        return true;
                    }

                    if children[index]
                        .node
                        .replace_pane_with_split(pane_id, direction, new_pane)
                    {
                        return true;
                    }
                }
                false
            }
        }
    }

    fn close_pane(&mut self, pane_id: PaneId) -> CloseOutcome {
        match self {
            Self::Pane(_) => CloseOutcome::NotFound,
            Self::Split {
                direction,
                children,
            } => {
                if let Some(index) = children
                    .iter()
                    .position(|child| matches!(child.node, Self::Pane(id) if id == pane_id))
                {
                    children.remove(index);
                    let new_focus = sibling_focus_after_removal(children, index)
                        .expect("split with remaining children must have focus target");
                    return match children.len() {
                        0 => CloseOutcome::NotFound,
                        1 => CloseOutcome::Closed {
                            replacement: Some(children.remove(0).node),
                            new_focus,
                        },
                        _ => CloseOutcome::Closed {
                            replacement: None,
                            new_focus,
                        },
                    };
                }

                for child_index in 0..children.len() {
                    let outcome = children[child_index].node.close_pane(pane_id);
                    let (replacement, new_focus) = match outcome {
                        CloseOutcome::NotFound => continue,
                        CloseOutcome::Closed {
                            replacement,
                            new_focus,
                        } => (replacement, new_focus),
                    };

                    if let Some(node) = replacement {
                        match node {
                            Self::Split {
                                direction: child_direction,
                                children: grand_children,
                            } if child_direction == *direction => {
                                let inherited_weight = children[child_index].weight;
                                children.remove(child_index);
                                for grand_child in grand_children.into_iter().rev() {
                                    let merged_weight = inherited_weight
                                        .max(grand_child.weight)
                                        .max(DEFAULT_CHILD_WEIGHT);
                                    children.insert(
                                        child_index,
                                        SplitChild::new(merged_weight, grand_child.node),
                                    );
                                }
                            }
                            other => {
                                children[child_index].node = other;
                            }
                        }
                    }

                    return match children.len() {
                        0 => CloseOutcome::NotFound,
                        1 => CloseOutcome::Closed {
                            replacement: Some(children.remove(0).node),
                            new_focus,
                        },
                        _ => CloseOutcome::Closed {
                            replacement: None,
                            new_focus,
                        },
                    };
                }

                CloseOutcome::NotFound
            }
        }
    }

    fn first_pane_id(&self) -> Option<PaneId> {
        match self {
            Self::Pane(id) => Some(*id),
            Self::Split { children, .. } => children
                .iter()
                .find_map(|child| child.node.first_pane_id()),
        }
    }

    fn collect_panes(&self, pane_ids: &mut Vec<PaneId>) {
        match self {
            Self::Pane(id) => pane_ids.push(*id),
            Self::Split { children, .. } => {
                for child in children {
                    child.node.collect_panes(pane_ids);
                }
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
                children,
            } => {
                let child_rects = split_rects(area, *direction, children);
                for (child, child_area) in children.iter().zip(child_rects) {
                    child.node.collect_placements(child_area, focused_pane, out);
                }
            }
        }
    }

    fn to_snapshot(&self) -> LayoutNodeSnapshot {
        match self {
            Self::Pane(pane_id) => LayoutNodeSnapshot::Pane { pane_id: *pane_id },
            Self::Split {
                direction,
                children,
            } => LayoutNodeSnapshot::Split {
                direction: *direction,
                children: children
                    .iter()
                    .map(|child| LayoutSplitChildSnapshot {
                        weight: child.weight,
                        node: child.node.to_snapshot(),
                    })
                    .collect(),
            },
        }
    }

    fn from_snapshot(snapshot: LayoutNodeSnapshot) -> Result<Self, LayoutError> {
        match snapshot {
            LayoutNodeSnapshot::Pane { pane_id } => Ok(Self::Pane(pane_id)),
            LayoutNodeSnapshot::Split {
                direction,
                children,
            } => Ok(Self::Split {
                direction,
                children: children
                    .into_iter()
                    .map(|child| {
                        Ok(SplitChild::new(
                            child.weight,
                            Self::from_snapshot(child.node)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, LayoutError>>()?,
            }),
        }
    }

    fn split_mut_at_path(&mut self, path: &[usize]) -> Option<&mut Self> {
        let mut node = self;
        for &index in path {
            match node {
                Self::Pane(_) => return None,
                Self::Split { children, .. } => {
                    node = &mut children.get_mut(index)?.node;
                }
            }
        }
        matches!(node, Self::Split { .. }).then_some(node)
    }
}

impl SplitChild {
    fn new(weight: u16, node: LayoutNode) -> Self {
        Self {
            weight: weight.max(DEFAULT_CHILD_WEIGHT),
            node,
        }
    }

    fn pane(pane_id: PaneId) -> Self {
        Self::new(DEFAULT_CHILD_WEIGHT, LayoutNode::Pane(pane_id))
    }
}

#[cfg(test)]
mod tests;
