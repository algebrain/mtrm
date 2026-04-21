#[test]
fn new_creates_single_tab_with_single_pane() {
    let temp = tempdir().unwrap();
    let manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

    assert_eq!(manager.tab_ids().len(), 1);
    assert_eq!(manager.active_tab_id(), TabId::new(0));
    assert_eq!(manager.active_pane_id(), PaneId::new(0));
}

#[test]
fn new_tab_adds_tab_and_activates_it() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

    let tab_id = manager
        .new_tab(&shell_config(temp.path().to_path_buf()))
        .unwrap();

    assert_eq!(manager.tab_ids(), vec![TabId::new(0), tab_id]);
    assert_eq!(manager.active_tab_id(), tab_id);
}

#[test]
fn activate_tab_switches_active_tab() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    let tab_id = manager
        .new_tab(&shell_config(temp.path().to_path_buf()))
        .unwrap();

    manager.activate_tab(TabId::new(0)).unwrap();
    assert_eq!(manager.active_tab_id(), TabId::new(0));

    manager.activate_tab(tab_id).unwrap();
    assert_eq!(manager.active_tab_id(), tab_id);
}

#[test]
fn split_active_pane_adds_new_pane() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

    let pane_id = manager
        .split_active_pane(
            SplitDirection::Vertical,
            &shell_config(temp.path().to_path_buf()),
        )
        .unwrap();

    let placements = manager
        .placements(Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 30,
        })
        .unwrap();
    assert_eq!(pane_id, PaneId::new(1));
    assert_eq!(placements.len(), 2);
    assert!(manager.pane_has_empty_screen(pane_id).unwrap());
}

#[test]
fn resize_active_pane_changes_layout_by_one_cell() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    manager
        .split_active_pane(
            SplitDirection::Vertical,
            &shell_config(temp.path().to_path_buf()),
        )
        .unwrap();
    manager.focus_pane(PaneId::new(0)).unwrap();

    let area = Rect {
        x: 0,
        y: 0,
        width: 40,
        height: 12,
    };
    let before = manager.placements(area).unwrap();

    let changed = manager
        .resize_active_pane(ResizeDirection::Right, area)
        .unwrap();
    let after = manager.placements(area).unwrap();

    assert!(changed);
    assert_eq!(after[0].1.width, before[0].1.width + 1);
    assert_eq!(after[1].1.width + 1, before[1].1.width);
    assert_eq!(manager.active_pane_id(), PaneId::new(0));
}

#[test]
fn close_active_pane_removes_it_and_keeps_tab_alive() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    manager
        .split_active_pane(
            SplitDirection::Vertical,
            &shell_config(temp.path().to_path_buf()),
        )
        .unwrap();

    let closed = manager.close_active_pane().unwrap();

    assert_eq!(closed, PaneId::new(1));
    assert_eq!(manager.active_pane_id(), PaneId::new(0));
    assert_eq!(
        manager
            .placements(Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 30,
            })
            .unwrap()
            .len(),
        1
    );
    assert!(matches!(
        manager.pane_text(closed),
        Err(TabsError::PaneNotFound(_))
    ));
}

#[test]
fn snapshot_reflects_current_state() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    let next_dir = temp.path().join("next");
    fs::create_dir(&next_dir).unwrap();
    let next_dir = fs::canonicalize(next_dir).unwrap();
    manager
        .write_to_active_pane(format!("cd '{}'\n", next_dir.display()).as_bytes())
        .unwrap();
    let changed = wait_until(Duration::from_secs(2), || {
        manager
            .active_pane_cwd()
            .map(|cwd| cwd == next_dir)
            .unwrap_or(false)
    });
    assert!(changed);

    let snapshot = manager.snapshot().unwrap();

    assert_eq!(snapshot.active_tab, TabId::new(0));
    assert_eq!(snapshot.tabs.len(), 1);
    assert_eq!(snapshot.tabs[0].panes[0].cwd, next_dir);
    assert_eq!(snapshot.tabs[0].panes[0].title, "pane-0");
}

#[cfg(target_os = "macos")]
#[test]
fn snapshot_canonicalizes_alias_temp_path_on_macos() {
    let temp = tempdir().unwrap();
    let canonical_dir = fs::canonicalize(temp.path()).unwrap();
    let canonical_text = canonical_dir.to_string_lossy();
    let alias_text = canonical_text.replacen("/private/var/", "/var/", 1);

    assert_ne!(
        alias_text, canonical_text,
        "test requires a canonical /private/var/... path on macOS"
    );

    let alias_dir = PathBuf::from(alias_text);
    assert!(
        alias_dir.exists(),
        "alias temp path must exist: {:?}",
        alias_dir
    );

    let manager = TabManager::new(&shell_config(alias_dir)).unwrap();
    let snapshot = manager.snapshot().unwrap();

    assert_eq!(snapshot.tabs[0].panes[0].cwd, canonical_dir);
}

#[test]
fn from_snapshot_restores_tabs_and_panes() {
    let temp = tempdir().unwrap();
    let dir_a = temp.path().join("a");
    let dir_b = temp.path().join("b");
    fs::create_dir(&dir_a).unwrap();
    fs::create_dir(&dir_b).unwrap();

    let snapshot = SessionSnapshot {
        tabs: vec![
            TabSnapshot {
                id: TabId::new(5),
                title: "first".to_owned(),
                layout: LayoutTree::new(PaneId::new(10)).to_snapshot(),
                panes: vec![PaneSnapshot {
                    id: PaneId::new(10),
                    cwd: dir_a.clone(),
                    title: "left".to_owned(),
                }],
                active_pane: PaneId::new(10),
            },
            TabSnapshot {
                id: TabId::new(6),
                title: "second".to_owned(),
                layout: {
                    let mut layout = LayoutTree::new(PaneId::new(20));
                    layout.split_focused(SplitDirection::Vertical, PaneId::new(21));
                    layout.focus_pane(PaneId::new(21)).unwrap();
                    layout.to_snapshot()
                },
                panes: vec![
                    PaneSnapshot {
                        id: PaneId::new(20),
                        cwd: dir_a.clone(),
                        title: "".to_owned(),
                    },
                    PaneSnapshot {
                        id: PaneId::new(21),
                        cwd: dir_b.clone(),
                        title: "right".to_owned(),
                    },
                ],
                active_pane: PaneId::new(21),
            },
        ],
        active_tab: TabId::new(6),
    };

    let manager =
        TabManager::from_snapshot(snapshot, &shell_config(temp.path().to_path_buf())).unwrap();

    assert_eq!(manager.tab_ids(), vec![TabId::new(5), TabId::new(6)]);
    assert_eq!(manager.active_tab_id(), TabId::new(6));
    assert_eq!(manager.active_pane_id(), PaneId::new(21));
    assert_eq!(manager.pane_title(PaneId::new(10)).unwrap(), "left");
    assert_eq!(manager.pane_title(PaneId::new(20)).unwrap(), "pane-20");
    assert_eq!(manager.pane_title(PaneId::new(21)).unwrap(), "right");
}

#[test]
fn rename_tab_updates_runtime_title_and_snapshot() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

    manager
        .rename_tab(TabId::new(0), "renamed".to_owned())
        .unwrap();

    assert_eq!(manager.active_tab_title(), "renamed");
    let snapshot = manager.snapshot().unwrap();
    assert_eq!(snapshot.tabs[0].title, "renamed");
}

#[test]
fn active_pane_title_comes_from_runtime_data() {
    let temp = tempdir().unwrap();
    let manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

    assert_eq!(manager.active_pane_title().unwrap(), "pane-0");
}

#[test]
fn rename_pane_updates_runtime_title_and_snapshot() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

    manager
        .rename_pane(PaneId::new(0), "editor".to_owned())
        .unwrap();

    assert_eq!(manager.active_pane_title().unwrap(), "editor");
    let snapshot = manager.snapshot().unwrap();
    assert_eq!(snapshot.tabs[0].panes[0].title, "editor");
}

#[test]
fn send_interrupt_is_forwarded_to_active_process() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

    manager.write_to_active_pane(b"sleep 5\n").unwrap();
    thread::sleep(Duration::from_millis(150));
    manager.send_interrupt_to_active_pane().unwrap();
    manager
        .write_to_active_pane(b"printf '__TABS_INTERRUPT__\\n'\n")
        .unwrap();

    let output =
        read_until_contains(&mut manager, "__TABS_INTERRUPT__", Duration::from_secs(3)).unwrap();
    assert!(output.contains("__TABS_INTERRUPT__"));
}

#[test]
fn active_pane_cwd_returns_live_process_directory() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    let next_dir = temp.path().join("cwd-next");
    fs::create_dir(&next_dir).unwrap();
    let next_dir = fs::canonicalize(next_dir).unwrap();

    manager
        .write_to_active_pane(format!("cd '{}'\n", next_dir.display()).as_bytes())
        .unwrap();

    let changed = wait_until(Duration::from_secs(2), || {
        manager
            .active_pane_cwd()
            .map(|cwd| cwd == next_dir)
            .unwrap_or(false)
    });
    assert!(changed);
}

#[test]
fn refresh_all_panes_preserves_output_of_inactive_pane() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    manager
        .split_active_pane(
            SplitDirection::Vertical,
            &shell_config(temp.path().to_path_buf()),
        )
        .unwrap();
    let right_pane = manager.active_pane_id();

    manager
        .write_to_active_pane(b"printf '__INACTIVE_SCREEN__\\n'\n")
        .unwrap();
    manager.move_focus(FocusMoveDirection::Left).unwrap();

    let captured = wait_until(Duration::from_secs(2), || {
        manager.refresh_all_panes().unwrap_or(false)
            && manager
                .pane_text(right_pane)
                .map(|text| text.contains("__INACTIVE_SCREEN__"))
                .unwrap_or(false)
    });

    assert!(
        captured,
        "inactive pane output must stay in pane screen state"
    );
}

#[test]
fn scrolling_active_pane_changes_visible_text() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    manager
        .resize_active_tab(Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 6,
        })
        .unwrap();
    manager
        .write_to_active_pane(
            b"i=1; while [ \"$i\" -le 20 ]; do printf 'line%s\\n' \"$i\"; i=$((i+1)); done\n",
        )
        .unwrap();

    let loaded = wait_until(Duration::from_secs(2), || {
        manager.refresh_all_panes().unwrap_or(false)
            && manager
                .active_pane_text()
                .map(|text| text.contains("line20"))
                .unwrap_or(false)
    });
    assert!(loaded);

    let before = manager.active_pane_text().unwrap();
    assert!(before.contains("line20"));
    assert!(manager.scroll_active_pane_up_lines(2).is_ok());
    let after = manager.active_pane_text().unwrap();

    assert_ne!(before, after);
}

#[test]
fn input_returns_scrolled_pane_to_bottom() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    manager
        .resize_active_tab(Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 6,
        })
        .unwrap();
    manager
        .write_to_active_pane(
            b"i=1; while [ \"$i\" -le 20 ]; do printf 'line%s\\n' \"$i\"; i=$((i+1)); done\n",
        )
        .unwrap();

    let loaded = wait_until(Duration::from_secs(2), || {
        manager.refresh_all_panes().unwrap_or(false)
            && manager
                .active_pane_text()
                .map(|text| text.contains("line20"))
                .unwrap_or(false)
    });
    assert!(loaded);

    manager.scroll_active_pane_up_lines(3).unwrap();
    assert!(manager.active_pane_text().unwrap().contains("line17"));
    manager
        .write_to_active_pane(b"printf '__BOTTOM__\\n'\n")
        .unwrap();

    let reset = wait_until(Duration::from_secs(2), || {
        manager.refresh_all_panes().unwrap_or(false)
            && manager
                .active_pane_text()
                .map(|text| text.contains("__BOTTOM__"))
                .unwrap_or(false)
    });
    assert!(reset);
}
