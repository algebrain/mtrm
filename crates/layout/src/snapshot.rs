use serde::{Deserialize, Deserializer, Serialize, Serializer};

use mtrm_core::{PaneId, SplitDirection};

use crate::{DEFAULT_CHILD_WEIGHT, LayoutNodeSnapshot, LayoutSplitChildSnapshot};

fn default_child_weight() -> u16 {
    DEFAULT_CHILD_WEIGHT
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct LayoutNodeSnapshotRepr {
    #[serde(rename = "Pane", default, skip_serializing_if = "Option::is_none")]
    pane: Option<LayoutPaneSnapshotRepr>,
    #[serde(rename = "Split", default, skip_serializing_if = "Option::is_none")]
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
