//! Раскладка окон: дерево разбиений, фокус и операции над ним.

use mtrm_core::{FocusMoveDirection, PaneId, ResizeDirection, SplitDirection};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const DEFAULT_CHILD_WEIGHT: u16 = 1;
const MIN_PANE_WIDTH: u16 = 8;
const MIN_PANE_HEIGHT: u16 = 4;

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

fn normalize(node: LayoutNode) -> LayoutNode {
    match node {
        LayoutNode::Pane(_) => node,
        LayoutNode::Split {
            direction,
            children,
        } => {
            let mut normalized_children = Vec::new();

            for child in children {
                let normalized_child = normalize(child.node);
                match normalized_child {
                    LayoutNode::Split {
                        direction: child_direction,
                        children: grand_children,
                    } if child_direction == direction => {
                        normalized_children.extend(grand_children);
                    }
                    other => normalized_children.push(SplitChild::new(child.weight, other)),
                }
            }

            match normalized_children.len() {
                0 => panic!("normalized split may not be empty"),
                1 => normalized_children.remove(0).node,
                _ => LayoutNode::Split {
                    direction,
                    children: normalized_children,
                },
            }
        }
    }
}

fn sibling_focus_after_removal(children: &[SplitChild], removed_index: usize) -> Option<PaneId> {
    if children.is_empty() {
        return None;
    }

    let preferred = removed_index.min(children.len().saturating_sub(1));
    children[preferred].node.first_pane_id()
}

fn find_resize_target(
    root: &LayoutNode,
    area: Rect,
    focused: PaneId,
    direction: ResizeDirection,
) -> Option<ResizeTarget> {
    let mut best = None;
    let mut path = Vec::new();
    let contains = collect_resize_target(root, area, focused, direction, &mut path, &mut best);
    contains.then_some(best).flatten()
}

fn collect_resize_target(
    node: &LayoutNode,
    area: Rect,
    focused: PaneId,
    direction: ResizeDirection,
    path: &mut Vec<usize>,
    best: &mut Option<ResizeTarget>,
) -> bool {
    match node {
        LayoutNode::Pane(id) => *id == focused,
        LayoutNode::Split {
            direction: split_direction,
            children,
        } => {
            let child_rects = split_rects(area, *split_direction, children);
            let Some(child_index) = children
                .iter()
                .position(|child| child.node.contains(focused))
            else {
                return false;
            };

            if split_matches_resize_direction(*split_direction, direction)
                && resolve_resize_operation(child_index, children.len(), direction).is_some()
            {
                *best = Some(ResizeTarget {
                    path_to_split: path.clone(),
                    child_index,
                    child_rects: child_rects.clone(),
                });
            }

            path.push(child_index);
            let contains = collect_resize_target(
                &children[child_index].node,
                child_rects[child_index],
                focused,
                direction,
                path,
                best,
            );
            path.pop();
            contains
        }
    }
}

fn split_matches_resize_direction(
    split_direction: SplitDirection,
    resize_direction: ResizeDirection,
) -> bool {
    match resize_direction {
        ResizeDirection::Left | ResizeDirection::Right => {
            split_direction == SplitDirection::Vertical
        }
        ResizeDirection::Up | ResizeDirection::Down => {
            split_direction == SplitDirection::Horizontal
        }
    }
}

fn resolve_resize_operation(
    child_index: usize,
    child_count: usize,
    direction: ResizeDirection,
) -> Option<(usize, usize, usize, usize)> {
    match direction {
        ResizeDirection::Left | ResizeDirection::Up => {
            if child_index > 0 {
                let left = child_index - 1;
                let right = child_index;
                Some((left, right, left, right))
            } else if child_index + 1 < child_count {
                let left = child_index;
                let right = child_index + 1;
                Some((left, right, left, right))
            } else {
                None
            }
        }
        ResizeDirection::Right | ResizeDirection::Down => {
            if child_index + 1 < child_count {
                let left = child_index;
                let right = child_index + 1;
                Some((left, right, right, left))
            } else if child_index > 0 {
                let left = child_index - 1;
                let right = child_index;
                Some((left, right, right, left))
            } else {
                None
            }
        }
    }
}

fn resize_axis_extent(rect: Rect, direction: ResizeDirection) -> u16 {
    match direction {
        ResizeDirection::Left | ResizeDirection::Right => rect.width,
        ResizeDirection::Up | ResizeDirection::Down => rect.height,
    }
}

fn split_rects(area: Rect, direction: SplitDirection, children: &[SplitChild]) -> Vec<Rect> {
    let weights: Vec<u16> = children.iter().map(|child| child.weight).collect();
    match direction {
        SplitDirection::Horizontal => {
            let heights = distribute_extent(area.height, &weights);
            let mut y = area.y;
            heights
                .into_iter()
                .map(|height| {
                    let rect = Rect {
                        x: area.x,
                        y,
                        width: area.width,
                        height,
                    };
                    y = y.saturating_add(height);
                    rect
                })
                .collect()
        }
        SplitDirection::Vertical => {
            let widths = distribute_extent(area.width, &weights);
            let mut x = area.x;
            widths
                .into_iter()
                .map(|width| {
                    let rect = Rect {
                        x,
                        y: area.y,
                        width,
                        height: area.height,
                    };
                    x = x.saturating_add(width);
                    rect
                })
                .collect()
        }
    }
}

fn distribute_extent(total: u16, weights: &[u16]) -> Vec<u16> {
    if weights.is_empty() {
        return Vec::new();
    }

    let total_u32 = u32::from(total);
    let sum_weights: u32 = weights
        .iter()
        .map(|weight| u32::from((*weight).max(DEFAULT_CHILD_WEIGHT)))
        .sum();
    let mut assigned = vec![0_u16; weights.len()];
    let mut consumed = 0_u32;

    for (index, weight) in weights.iter().enumerate() {
        if index + 1 == weights.len() {
            assigned[index] = total_u32.saturating_sub(consumed) as u16;
        } else {
            let share = total_u32 * u32::from((*weight).max(DEFAULT_CHILD_WEIGHT)) / sum_weights;
            assigned[index] = share as u16;
            consumed = consumed.saturating_add(share);
        }
    }

    assigned
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

fn default_child_weight() -> u16 {
    DEFAULT_CHILD_WEIGHT
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct LayoutNodeSnapshotRepr {
    #[serde(
        rename = "Pane",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pane: Option<LayoutPaneSnapshotRepr>,
    #[serde(
        rename = "Split",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    split: Option<LayoutSplitSnapshotRepr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LayoutPaneSnapshotRepr {
    pane_id: PaneId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
enum LayoutSplitSnapshotRepr {
    Nary {
        direction: SplitDirection,
        children: Vec<LayoutSplitChildSnapshotRepr>,
    },
    Binary {
        direction: SplitDirection,
        first: Box<LayoutNodeSnapshotRepr>,
        second: Box<LayoutNodeSnapshotRepr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LayoutSplitChildSnapshotRepr {
    #[serde(default = "default_child_weight")]
    weight: u16,
    node: LayoutNodeSnapshotRepr,
}

impl Serialize for LayoutNodeSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        LayoutNodeSnapshotRepr::from(self.clone()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for LayoutNodeSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let repr = LayoutNodeSnapshotRepr::deserialize(deserializer)?;
        Ok(LayoutNodeSnapshot::from(repr))
    }
}

impl From<LayoutNodeSnapshot> for LayoutNodeSnapshotRepr {
    fn from(snapshot: LayoutNodeSnapshot) -> Self {
        match snapshot {
            LayoutNodeSnapshot::Pane { pane_id } => Self {
                pane: Some(LayoutPaneSnapshotRepr { pane_id }),
                split: None,
            },
            LayoutNodeSnapshot::Split {
                direction,
                children,
            } => Self {
                pane: None,
                split: Some(LayoutSplitSnapshotRepr::Nary {
                    direction,
                    children: children.into_iter().map(Into::into).collect(),
                }),
            },
        }
    }
}

impl From<LayoutSplitChildSnapshot> for LayoutSplitChildSnapshotRepr {
    fn from(child: LayoutSplitChildSnapshot) -> Self {
        Self {
            weight: child.weight,
            node: child.node.into(),
        }
    }
}

impl From<LayoutNodeSnapshotRepr> for LayoutNodeSnapshot {
    fn from(repr: LayoutNodeSnapshotRepr) -> Self {
        normalize_snapshot(match repr {
            LayoutNodeSnapshotRepr {
                pane: Some(LayoutPaneSnapshotRepr { pane_id }),
                split: None,
            } => LayoutNodeSnapshot::Pane { pane_id },
            LayoutNodeSnapshotRepr {
                pane: None,
                split:
                    Some(LayoutSplitSnapshotRepr::Nary {
                        direction,
                        children,
                    }),
            } => LayoutNodeSnapshot::Split {
                direction,
                children: children.into_iter().map(Into::into).collect(),
            },
            LayoutNodeSnapshotRepr {
                pane: None,
                split:
                    Some(LayoutSplitSnapshotRepr::Binary {
                        direction,
                        first,
                        second,
                    }),
            } => LayoutNodeSnapshot::Split {
                direction,
                children: vec![
                    LayoutSplitChildSnapshot {
                        weight: DEFAULT_CHILD_WEIGHT,
                        node: (*first).into(),
                    },
                    LayoutSplitChildSnapshot {
                        weight: DEFAULT_CHILD_WEIGHT,
                        node: (*second).into(),
                    },
                ],
            },
            _ => panic!("layout snapshot node must contain exactly one of Pane or Split"),
        })
    }
}

impl From<LayoutSplitChildSnapshotRepr> for LayoutSplitChildSnapshot {
    fn from(child: LayoutSplitChildSnapshotRepr) -> Self {
        Self {
            weight: child.weight.max(DEFAULT_CHILD_WEIGHT),
            node: child.node.into(),
        }
    }
}

fn normalize_snapshot(node: LayoutNodeSnapshot) -> LayoutNodeSnapshot {
    match node {
        LayoutNodeSnapshot::Pane { .. } => node,
        LayoutNodeSnapshot::Split {
            direction,
            children,
        } => {
            let mut normalized = Vec::new();

            for child in children.into_iter().map(normalize_snapshot_child) {
                match child.node {
                    LayoutNodeSnapshot::Split {
                        direction: child_direction,
                        children: grand_children,
                    } if child_direction == direction => normalized.extend(grand_children),
                    other => normalized.push(LayoutSplitChildSnapshot {
                        weight: child.weight,
                        node: other,
                    }),
                }
            }

            match normalized.len() {
                0 => panic!("normalized snapshot split may not be empty"),
                1 => normalized.remove(0).node,
                _ => LayoutNodeSnapshot::Split {
                    direction,
                    children: normalized,
                },
            }
        }
    }
}

fn normalize_snapshot_child(child: LayoutSplitChildSnapshot) -> LayoutSplitChildSnapshot {
    LayoutSplitChildSnapshot {
        weight: child.weight.max(DEFAULT_CHILD_WEIGHT),
        node: normalize_snapshot(child.node),
    }
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
}
