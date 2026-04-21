use mtrm_core::{FocusMoveDirection, PaneId, ResizeDirection, SplitDirection};

use crate::{
    DEFAULT_CHILD_WEIGHT, LayoutNode, Rect, ResizeTarget, SplitChild,
};

pub(crate) fn normalize(node: LayoutNode) -> LayoutNode {
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

pub(crate) fn sibling_focus_after_removal(
    children: &[SplitChild],
    removed_index: usize,
) -> Option<PaneId> {
    if children.is_empty() {
        return None;
    }

    let preferred = removed_index.min(children.len().saturating_sub(1));
    children[preferred].node.first_pane_id()
}

pub(crate) fn find_resize_target(
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
            let Some(child_index) = children.iter().position(|child| child.node.contains(focused))
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

pub(crate) fn resolve_resize_operation(
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

pub(crate) fn resize_axis_extent(rect: Rect, direction: ResizeDirection) -> u16 {
    match direction {
        ResizeDirection::Left | ResizeDirection::Right => rect.width,
        ResizeDirection::Up | ResizeDirection::Down => rect.height,
    }
}

pub(crate) fn split_rects(area: Rect, direction: SplitDirection, children: &[SplitChild]) -> Vec<Rect> {
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

pub(crate) fn is_in_direction(current: Rect, candidate: Rect, direction: FocusMoveDirection) -> bool {
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

pub(crate) fn focus_score(
    current: Rect,
    candidate: Rect,
    direction: FocusMoveDirection,
) -> (i32, i32, u16) {
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
