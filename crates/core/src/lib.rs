//! Базовые типы и общие контракты `mtrm`.

use serde::{Deserialize, Serialize};

macro_rules! define_id_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        pub struct $name(u64);

        impl $name {
            pub const fn new(raw: u64) -> Self {
                Self(raw)
            }

            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

define_id_type!(TabId);
define_id_type!(PaneId);
define_id_type!(SplitId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FocusMoveDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipboardCommand {
    CopySelection,
    PasteFromSystem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutCommand {
    SplitFocused(SplitDirection),
    CloseFocusedPane,
    MoveFocus(FocusMoveDirection),
    ScrollUpLines(u16),
    ScrollDownLines(u16),
    ScrollUpPages(u16),
    ScrollDownPages(u16),
    ScrollToBottom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabCommand {
    NewTab,
    CloseCurrentTab,
    NextTab,
    PreviousTab,
    Activate(TabId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppCommand {
    Clipboard(ClipboardCommand),
    Layout(LayoutCommand),
    Tabs(TabCommand),
    SendInterrupt,
    RequestSave,
    Quit,
}

#[derive(Debug, Default)]
pub struct IdAllocator {
    next_tab: u64,
    next_pane: u64,
    next_split: u64,
}

impl IdAllocator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_tab_id(&mut self) -> TabId {
        let id = TabId::new(self.next_tab);
        self.next_tab += 1;
        id
    }

    pub fn next_pane_id(&mut self) -> PaneId {
        let id = PaneId::new(self.next_pane);
        self.next_pane += 1;
        id
    }

    pub fn next_split_id(&mut self) -> SplitId {
        let id = SplitId::new(self.next_split);
        self.next_split += 1;
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_id_roundtrip() {
        let id = TabId::new(42);
        assert_eq!(id.get(), 42);
    }

    #[test]
    fn pane_id_roundtrip() {
        let id = PaneId::new(7);
        assert_eq!(id.get(), 7);
    }

    #[test]
    fn split_id_roundtrip() {
        let id = SplitId::new(3);
        assert_eq!(id.get(), 3);
    }

    #[test]
    fn ids_support_equality_and_inequality() {
        assert_eq!(TabId::new(1), TabId::new(1));
        assert_ne!(TabId::new(1), TabId::new(2));
        assert_eq!(PaneId::new(9), PaneId::new(9));
        assert_ne!(SplitId::new(4), SplitId::new(5));
    }

    #[test]
    fn allocator_generates_monotonic_ids_per_kind() {
        let mut allocator = IdAllocator::new();

        assert_eq!(allocator.next_tab_id(), TabId::new(0));
        assert_eq!(allocator.next_tab_id(), TabId::new(1));
        assert_eq!(allocator.next_pane_id(), PaneId::new(0));
        assert_eq!(allocator.next_pane_id(), PaneId::new(1));
        assert_eq!(allocator.next_split_id(), SplitId::new(0));
        assert_eq!(allocator.next_split_id(), SplitId::new(1));
    }

    #[test]
    fn serializes_and_deserializes_id_types() {
        let tab = TabId::new(11);
        let pane = PaneId::new(12);
        let split = SplitId::new(13);

        assert_eq!(
            serde_json::from_str::<TabId>(&serde_json::to_string(&tab).unwrap()).unwrap(),
            tab
        );
        assert_eq!(
            serde_json::from_str::<PaneId>(&serde_json::to_string(&pane).unwrap()).unwrap(),
            pane
        );
        assert_eq!(
            serde_json::from_str::<SplitId>(&serde_json::to_string(&split).unwrap()).unwrap(),
            split
        );
    }

    #[test]
    fn serializes_and_deserializes_enums() {
        let commands = [
            AppCommand::Clipboard(ClipboardCommand::CopySelection),
            AppCommand::Clipboard(ClipboardCommand::PasteFromSystem),
            AppCommand::Layout(LayoutCommand::SplitFocused(SplitDirection::Horizontal)),
            AppCommand::Layout(LayoutCommand::MoveFocus(FocusMoveDirection::Left)),
            AppCommand::Layout(LayoutCommand::CloseFocusedPane),
            AppCommand::Layout(LayoutCommand::ScrollUpLines(1)),
            AppCommand::Layout(LayoutCommand::ScrollDownLines(1)),
            AppCommand::Layout(LayoutCommand::ScrollUpPages(1)),
            AppCommand::Layout(LayoutCommand::ScrollDownPages(1)),
            AppCommand::Layout(LayoutCommand::ScrollToBottom),
            AppCommand::Tabs(TabCommand::NewTab),
            AppCommand::Tabs(TabCommand::Activate(TabId::new(99))),
            AppCommand::SendInterrupt,
            AppCommand::RequestSave,
            AppCommand::Quit,
        ];

        for command in commands {
            let json = serde_json::to_string(&command).unwrap();
            let decoded = serde_json::from_str::<AppCommand>(&json).unwrap();
            assert_eq!(decoded, command);
        }
    }
}
