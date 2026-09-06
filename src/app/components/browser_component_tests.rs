use super::browser::BrowserComponent;
use super::browser_narrow::NarrowBrowseExtras;
use super::component_id::BrowserKind;
use crate::app::components::browser::{BrowserContent, BrowserIdentity};
use crate::app::components::inline_search::{InlineSearchHost, SearchPool};
use crate::app::components::msg::{Msg, ShellRequest};
use crate::app::library_column_width::{library_cell_width, LIBRARY_COLUMN_GAP};
use crate::app::render::LibraryListRenderCtx;
use crate::app::tests::{make_item, make_items};

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{
    Event, Key, KeyEvent as TuiKeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
/// Local keyboard navigation routes through typed `ShellRequest`s (task
/// 5.3d): while focused, the component moves its own cursor exactly the way
/// the legacy `App::move_lib_cursor_rows`/`jump_lib_cursor` bindings move
/// the App cursor, and returns the typed request in place of the raw
/// typed key request so the shell drives the App cursor through the same
/// App methods (never in addition — no double movement). A 40-item flat
/// list rendered 100 columns wide packs two items per row and pages
/// `(height - 1) * cols = 9 * 2 = 18` items — every case below lands on
/// the legacy stride, and the two clamp cases pin the ends.
#[test]
fn browser_local_navigation_mirrors_legacy_flat_movement() {
    let cases = [
        // (key, from, expected)
        (Key::Down, 0, 2),
        (Key::Char('j'), 0, 2),
        (Key::Up, 4, 2),
        (Key::Char('k'), 4, 2),
        (Key::Left, 4, 3),
        (Key::Char('h'), 4, 3),
        (Key::Right, 4, 5),
        (Key::Char('l'), 4, 5),
        (Key::Down, 39, 39),  // clamp at the last item
        (Key::Up, 1, 1),      // already at the first painted row
        (Key::Left, 0, 0),    // clamp at the left edge
        (Key::Right, 39, 39), // clamp at the right edge
        // PageDown/PageUp stride (height - 1) * cols — the page excludes
        // the count/search header line, not the full painted height.
        (Key::PageDown, 10, 28),
        (Key::PageUp, 28, 10),
        (Key::Home, 39, 0),
        (Key::End, 0, 39),
    ];
    for (key, from, expected) in cases {
        let mut browser = BrowserComponent::new();
        browser.set_content(BrowserContent::from_items(make_items(40)));
        browser.set_focused(true);
        browser.set_cursor_for_test(from);
        let mut terminal = Terminal::new(TestBackend::new(100, 10)).unwrap();
        terminal
            .draw(|frame| browser.view(frame, frame.area()))
            .unwrap();
        let message = browser.handle_tui_key(TuiKeyEvent {
            code: key,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            browser.cursor(),
            expected,
            "{key:?} from cursor {from} in a two-column flat list"
        );
        assert_eq!(
            message,
            Some(Msg::Shell(expected_movement_request(key, expected))),
            "{key:?} must return the typed movement request in place of the raw legacy key"
        );
    }

    // Unfocused (Queue/playback own panel focus): movement keys do not
    // mutate the component cursor and remain unclaimed by this component.
    let mut browser = BrowserComponent::new();
    browser.set_content(BrowserContent::from_items(make_items(40)));
    browser.set_focused(false);
    browser.set_cursor_for_test(7);
    let mut terminal = Terminal::new(TestBackend::new(100, 10)).unwrap();
    terminal
        .draw(|frame| browser.view(frame, frame.area()))
        .unwrap();
    for key in [
        Key::Up,
        Key::Down,
        Key::Left,
        Key::Right,
        Key::PageUp,
        Key::PageDown,
        Key::Home,
        Key::End,
        Key::Char('h'),
        Key::Char('j'),
        Key::Char('k'),
        Key::Char('l'),
    ] {
        let message = browser.handle_tui_key(TuiKeyEvent {
            code: key,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            browser.cursor(),
            7,
            "unfocused {key:?} must not move the cursor"
        );
        assert_eq!(message, None, "unfocused {key:?} must stay unclaimed");
    }
}

/// The typed movement request the focused two-column browser must return
/// for each navigation key (task 5.3d), asserted against the emitted
/// `Msg::Shell` payload by `browser_local_navigation_mirrors_legacy_flat_movement`.
/// The page payload is the painted display-row stride `(height - 1) = 9`
/// the 100-wide, 10-tall test list reports via `page_rows()` — the App
/// applies its own column count to that stride, exactly like the legacy arm.
fn expected_movement_request(_key: Key, index: usize) -> ShellRequest {
    ShellRequest::BrowserCursorIndex { index }
}

#[test]
fn browser_wheel_steps_one_painted_row_and_ignores_chrome() {
    let mut browser = BrowserComponent::new();
    browser.set_content(BrowserContent::from_items(make_items(8)));
    browser.set_focused(true);
    let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
    terminal
        .draw(|frame| browser.view(frame, frame.area()))
        .unwrap();
    let list = browser.test_layout().left_area;
    let wheel = |kind| {
        Event::Mouse(MouseEvent {
            kind,
            column: list.x + 1,
            row: list.y + 1,
            modifiers: KeyModifiers::NONE,
        })
    };

    assert_eq!(
        browser.on(&wheel(MouseEventKind::ScrollDown)),
        Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index: 1 }))
    );
    browser.reset_mouse_gestures_for_test();
    assert_eq!(
        browser.on(&wheel(MouseEventKind::ScrollUp)),
        Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index: 0 }))
    );
}

/// Letter-grouped lists (60 items render bucketed rows with a header row
/// between buckets and a ragged trailing row per bucket) striding one
/// PAINTED item row per Up/Down, using the component's `letter_vertical_delta`:
/// headers do not participate and a ragged target row falls back to its
/// last item. The painted (2-column) item rows are
///   A\u{2013}C: [0,1]..[26,27],[28]   (ragged: item 28 alone)
///   D\u{2013}F: [29,30]..[43,44]
///   G\u{2013}I: [45,46]..[57,58],[59] (ragged: item 59 alone)
/// Flat arithmetic (the pre-align +1 and the naive +2) lands on a
/// different item in every bracketed case, so each assertion is decisive.
#[test]
fn browser_local_navigation_skips_letter_headers_and_ragged_rows() {
    let mut items = Vec::new();
    for i in 0..15 {
        let mut item = make_item(&format!("Alpha {i}"), "Movie");
        item.id = format!("a{i}");
        items.push(item);
    }
    for i in 0..14 {
        let mut item = make_item(&format!("Beta {i}"), "Movie");
        item.id = format!("b{i}");
        items.push(item);
    }
    for i in 0..16 {
        let mut item = make_item(&format!("Delta {i}"), "Movie");
        item.id = format!("d{i}");
        items.push(item);
    }
    for i in 0..15 {
        let mut item = make_item(&format!("Gamma {i}"), "Movie");
        item.id = format!("g{i}");
        items.push(item);
    }
    assert_eq!(items.len(), 60);

    let cases = [
        // (key, from, expected) — letter-grouped 2-column layout
        (Key::Down, 27, 28), // ragged target row [28]: fall back to its last item
        (Key::Down, 28, 29), // across the D–F header: next *item* row is [29,30]
        (Key::Up, 29, 28),   // back across the header
        (Key::Down, 59, 59), // clamp at the very last item
        (Key::Up, 0, 0),     // clamp at the very first item
        (Key::Home, 59, 0),  // sorted order first
        (Key::End, 0, 59),   // sorted order last
        (Key::Left, 4, 3),   // sorted-order ±1 (column adjacency)
        (Key::Right, 4, 5),
    ];
    for (key, from, expected) in cases {
        let mut browser = BrowserComponent::new();
        browser.set_content(BrowserContent::from_items(items.clone()));
        browser.set_focused(true);
        browser.set_cursor_for_test(from);
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| browser.view(frame, frame.area()))
            .unwrap();
        browser.handle_tui_key(TuiKeyEvent {
            code: key,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            browser.cursor(),
            expected,
            "{key:?} from cursor {from} in the letter-grouped list"
        );
    }
}

#[test]
fn browser_control_transition_preserves_the_selected_viewport_offset() {
    let mut browser = BrowserComponent::new_for_kind(BrowserKind::Movies);
    browser.set_content(BrowserContent::from_items(make_items(40)));
    browser.set_focused(true);
    browser.set_narrow_extras(NarrowBrowseExtras {
        hero_placeholder: true,
        ..NarrowBrowseExtras::default()
    });
    // Seed a nontrivial resting viewport before the first breakpoint paint.
    browser.apply_position(20, 8);

    let mut terminal = Terminal::new(TestBackend::new(60, 30)).unwrap();
    terminal
        .draw(|frame| browser.view(frame, frame.area()))
        .unwrap();
    let narrow_anchor = browser
        .viewport_anchor(browser.painted_viewport_height())
        .expect("narrow inline control has a selected item");

    let mut wide_terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    wide_terminal
        .draw(|frame| browser.view(frame, frame.area()))
        .unwrap();
    let wide_anchor = browser
        .viewport_anchor(browser.painted_viewport_height())
        .expect("wide control has a selected item");

    assert_eq!(wide_anchor.selected_target, narrow_anchor.selected_target);
    assert_eq!(
        wide_anchor.selected_row_offset, narrow_anchor.selected_row_offset,
        "Movies breakpoint handoff must preserve the outgoing inline offset"
    );

    let mut narrow_terminal = Terminal::new(TestBackend::new(60, 30)).unwrap();
    narrow_terminal
        .draw(|frame| browser.view(frame, frame.area()))
        .unwrap();
    let narrow_again = browser
        .viewport_anchor(browser.painted_viewport_height())
        .expect("narrow control has a selected item after returning");
    assert_eq!(
        narrow_again.selected_target, wide_anchor.selected_target,
        "Movies breakpoint handoff must preserve the selected item in both directions"
    );
    assert_eq!(
        narrow_again.selected_row_offset, wide_anchor.selected_row_offset,
        "Movies breakpoint handoff must preserve the outgoing wide offset"
    );
}

/// Wide-Movies exact parity: a Movies-keyed component on a >=82-wide
/// rendered list uses its own kind and painted geometry, and the right
/// rail strides ONE item per row, matching its painted one-column geometry.
/// Down from 0 lands at 1, not 2, and returns the typed rows request;
/// Left/Right/h/l stay unbound locally.
#[test]
fn browser_local_navigation_strides_one_column_for_wide_movies() {
    let mut browser = BrowserComponent::new_for_kind(BrowserKind::Movies);
    browser.set_content(BrowserContent::from_items(make_items(12)));
    browser.set_focused(true);
    browser.configure_wide_movies(false, false);
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal
        .draw(|frame| browser.view(frame, frame.area()))
        .unwrap();

    let message = browser.handle_tui_key(TuiKeyEvent {
        code: Key::Down,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        browser.cursor(),
        1,
        "wide-Movies rail Down must stride one item, not two"
    );
    assert_eq!(
        message,
        Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index: 1 })),
        "wide-Movies Down must return the typed rows request"
    );

    browser.handle_tui_key(TuiKeyEvent {
        code: Key::Down,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(browser.cursor(), 2);
    browser.handle_tui_key(TuiKeyEvent {
        code: Key::Up,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(browser.cursor(), 1);

    for key in [Key::Left, Key::Right, Key::Char('h'), Key::Char('l')] {
        let message = browser.handle_tui_key(TuiKeyEvent {
            code: key,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            browser.cursor(),
            1,
            "wide-Movies rail {key:?} must stay unbound locally"
        );
        assert_eq!(message, None, "wide-Movies {key:?} must stay unclaimed");
    }
}

#[test]
fn browser_alt_navigation_stays_unclaimed() {
    let mut browser = BrowserComponent::new();
    browser.set_content(BrowserContent::from_items(make_items(2)));
    browser.set_focused(true);

    for code in [Key::Left, Key::Right, Key::Up, Key::Down] {
        let message = browser.on(&Event::Keyboard(TuiKeyEvent {
            code,
            modifiers: KeyModifiers::ALT,
        }));
        assert_eq!(message, None, "Alt+{code:?} belongs to the router");
    }
    assert_eq!(
        browser.cursor(),
        0,
        "Alt navigation must not move the local cursor"
    );
}
#[test]
fn browser_alt_refresh_stays_component_owned() {
    let mut browser = BrowserComponent::new();
    browser.set_content(BrowserContent::from_items(make_items(1)));
    browser.set_focused(true);

    let message = browser.on(&Event::Keyboard(TuiKeyEvent {
        code: Key::Char('r'),
        modifiers: KeyModifiers::ALT,
    }));

    assert_eq!(
        message,
        Some(Msg::Shell(ShellRequest::BrowserRefresh)),
        "Alt+r must remain the browser's local refresh effect"
    );
}

#[test]
fn browser_context_menu_requires_bare_dot() {
    let mut browser = BrowserComponent::new();
    browser.set_content(BrowserContent::from_items(make_items(1)));
    browser.set_focused(true);

    let modified = browser.on(&Event::Keyboard(TuiKeyEvent {
        code: Key::Char('.'),
        modifiers: KeyModifiers::CONTROL,
    }));
    assert_eq!(modified, None, "Ctrl+. must not open the context menu");

    let bare = browser.on(&Event::Keyboard(TuiKeyEvent {
        code: Key::Char('.'),
        modifiers: KeyModifiers::NONE,
    }));
    assert!(
        matches!(
            bare,
            Some(Msg::Shell(ShellRequest::BrowserContextMenu { .. }))
        ),
        "bare `.` must open the context menu, got {bare:?}"
    );
}

#[test]
fn set_content_keeps_the_control_cursor_and_apply_position_moves_it() {
    let mut browser = BrowserComponent::new();
    let items = || make_items(4);
    browser.set_content(BrowserContent::from_items(items()));
    browser.set_focused(true);

    browser.apply_position(1, 2);
    assert_eq!(browser.cursor(), 1);
    assert_eq!(browser.scroll(), 2);

    // Position in the render context is deliberately stripped before content
    // reaches the control, so a nonzero incoming cursor/scroll cannot replace
    // its current position.
    browser.set_content(BrowserContent::from_render_ctx(
        LibraryListRenderCtx::from_items(items(), 3, 7),
    ));
    browser.set_focused(true);
    assert_eq!(browser.cursor(), 1);
    assert_eq!(browser.scroll(), 2);

    let identity = BrowserIdentity::default();
    assert!(browser.note_browse_identity(identity.clone()));
    let content = |ctx| BrowserContent::from_render_ctx(ctx);
    for (label, content) in [
        (
            "pagination",
            content(LibraryListRenderCtx::from_items(make_items(3), 2, 6)),
        ),
        (
            "loading",
            content(LibraryListRenderCtx::from_items(items(), 3, 7).with_loading(true)),
        ),
        (
            "refresh",
            content(LibraryListRenderCtx::from_items(items(), 0, 0)),
        ),
    ] {
        // Unchanged browse identity means pagination, loading completion, and
        // refresh content pushes cannot re-seed the component's position.
        assert!(!browser.note_browse_identity(identity.clone()), "{label}");
        browser.set_content(content);
        browser.set_focused(true);
        assert_eq!(browser.cursor(), 1, "{label} cursor");
        assert_eq!(browser.scroll(), 2, "{label} scroll");
    }

    let changed_identities = [
        (
            "depth",
            BrowserIdentity {
                depth: 1,
                ..identity.clone()
            },
        ),
        (
            "parent",
            BrowserIdentity {
                parent_id: "parent".into(),
                ..identity.clone()
            },
        ),
        ("back restore", identity.clone()),
        (
            "letter reset",
            BrowserIdentity {
                letter_filter: Some(1),
                ..identity.clone()
            },
        ),
        (
            "sort",
            BrowserIdentity {
                sort_by: "DateCreated".into(),
                ..identity.clone()
            },
        ),
        (
            "saved restore",
            BrowserIdentity {
                unplayed_only: true,
                ..identity.clone()
            },
        ),
        (
            "feed group",
            BrowserIdentity {
                feed_group: Some(1),
                ..identity.clone()
            },
        ),
    ];
    for (label, changed_identity) in changed_identities {
        // Start every changed-identity case from a known position. The
        // identity change itself does not move the control; the explicit
        // apply_position push is the only position re-seed.
        browser.apply_position(1, 2);
        assert!(
            browser.note_browse_identity(changed_identity),
            "{label} must be a new browse identity"
        );
        browser.set_content(content(LibraryListRenderCtx::from_items(items(), 0, 0)));
        browser.set_focused(true);
        assert_eq!(browser.cursor(), 1, "{label} before explicit position");
        assert_eq!(browser.scroll(), 2, "{label} before explicit position");
        browser.apply_position(2, 3);
        assert_eq!(browser.cursor(), 2, "{label} cursor");
        assert_eq!(browser.scroll(), 3, "{label} scroll");
    }
}

#[test]
fn browser_renders_the_shared_generic_rows() {
    let mut browser = BrowserComponent::new();
    browser.set_content(BrowserContent::from_items(vec![make_item(
        "Movie one",
        "Movie",
    )]));
    browser.set_focused(true);
    let mut terminal = Terminal::new(TestBackend::new(40, 4)).unwrap();
    terminal
        .draw(|frame| browser.view(frame, frame.area()))
        .unwrap();

    let rendered: String = (0..40)
        .map(|x| terminal.backend().buffer().cell((x, 0)).unwrap().symbol())
        .collect();
    assert!(rendered.contains("Movie one"));
}

#[test]
fn browser_mouse_uses_the_painted_two_column_cell_for_left_and_right_clicks() {
    let mut browser = BrowserComponent::new();
    browser.set_content(BrowserContent::from_items(vec![
        make_item("first", "Movie"),
        make_item("second", "Movie"),
    ]));
    browser.set_focused(true);
    let mut terminal = Terminal::new(TestBackend::new(100, 6)).unwrap();
    terminal
        .draw(|frame| browser.view(frame, frame.area()))
        .unwrap();
    let layout = browser.test_layout();
    let area = layout.left_area;
    let cell_width = library_cell_width(area, 2);
    let position = (area.x + cell_width + LIBRARY_COLUMN_GAP, area.y);

    let left = browser.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: position.0,
        row: position.1,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(matches!(
        left,
        Some(crate::app::components::msg::Msg::Shell(
            crate::app::components::msg::ShellRequest::BrowserRowClick { target: 1 }
        ))
    ));

    let right = browser.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: position.0,
        row: position.1,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(matches!(
        right,
        Some(crate::app::components::msg::Msg::Shell(
            crate::app::components::msg::ShellRequest::BrowserRowContextMenu { target: 1, .. }
        ))
    ));
}

/// Narrow canonical-list path (task 6.2): with `wide_movies` false and the
/// hero-capable browse surface reserving an inline hero block
/// (`hero_placeholder`), the active control is the embedded
/// `InlineMediaBrowser`, not the generic two-column grid — `left_item_rows`
/// stays empty and row identity comes from `inline_browser.resolve_point`
/// (design.md D6). A left click below the reserved hero block must move the
/// cursor to the clicked row and emit the resolved target, exactly like the
/// wide rail's `browser_mouse_uses_the_painted_two_column_cell_for_left_and_right_clicks`.
#[test]
fn narrow_canonical_list_click_moves_cursor_and_emits_row_click() {
    let mut browser = BrowserComponent::new_for_kind(BrowserKind::Movies);
    browser.set_content(BrowserContent::from_items(make_items(6)));
    browser.set_focused(true);
    browser.set_narrow_extras(NarrowBrowseExtras {
        hero_placeholder: true,
        ..NarrowBrowseExtras::default()
    });
    let mut terminal = Terminal::new(TestBackend::new(60, 30)).unwrap();
    terminal
        .draw(|frame| browser.view(frame, frame.area()))
        .unwrap();
    assert!(
        browser.test_layout().left_item_rows.is_empty(),
        "narrow canonical path must not populate the generic-grid row map"
    );

    let (area, row_map) = {
        let layout = browser.test_layout();
        (layout.left_area, layout.left_row_map.clone())
    };
    let target_row = row_map
        .iter()
        .position(|target| matches!(target, Some(idx) if *idx != browser.cursor()))
        .expect("a non-selected row is painted below the inline hero");
    let target = row_map[target_row].unwrap();
    let position = (area.x, area.y + target_row as u16);

    let message = browser.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: position.0,
        row: position.1,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(
        message,
        Some(Msg::Shell(ShellRequest::BrowserRowClick { target })),
        "narrow canonical row click must resolve via inline_browser.resolve_point"
    );
    assert_eq!(browser.cursor(), target);
}

/// A double-click on the same narrow canonical row emits activation instead
/// of a plain cursor move (mirrors the accepted Music narrow precedent,
/// task 6.1).
#[test]
fn narrow_canonical_list_double_click_emits_row_activate() {
    let mut browser = BrowserComponent::new_for_kind(BrowserKind::Movies);
    browser.set_content(BrowserContent::from_items(make_items(6)));
    browser.set_focused(true);
    browser.set_narrow_extras(NarrowBrowseExtras {
        hero_placeholder: true,
        ..NarrowBrowseExtras::default()
    });
    let mut terminal = Terminal::new(TestBackend::new(60, 30)).unwrap();
    terminal
        .draw(|frame| browser.view(frame, frame.area()))
        .unwrap();

    let (area, row_map) = {
        let layout = browser.test_layout();
        (layout.left_area, layout.left_row_map.clone())
    };
    let target_row = row_map
        .iter()
        .position(|target| matches!(target, Some(idx) if *idx != browser.cursor()))
        .expect("a non-selected row is painted below the inline hero");
    let target = row_map[target_row].unwrap();
    let down = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: area.x,
        row: area.y + target_row as u16,
        modifiers: KeyModifiers::NONE,
    });

    let first = browser.on(&down);
    assert_eq!(
        first,
        Some(Msg::Shell(ShellRequest::BrowserRowClick { target }))
    );
    let second = browser.on(&down);
    assert_eq!(
        second,
        Some(Msg::Shell(ShellRequest::BrowserRowActivate { target }))
    );
}

/// A right click on a narrow canonical row emits the context menu request
/// with the raw pointer position as the anchor (design.md D4).
#[test]
fn narrow_canonical_list_right_click_emits_row_context_menu() {
    let mut browser = BrowserComponent::new_for_kind(BrowserKind::Movies);
    browser.set_content(BrowserContent::from_items(make_items(6)));
    browser.set_focused(true);
    browser.set_narrow_extras(NarrowBrowseExtras {
        hero_placeholder: true,
        ..NarrowBrowseExtras::default()
    });
    let mut terminal = Terminal::new(TestBackend::new(60, 30)).unwrap();
    terminal
        .draw(|frame| browser.view(frame, frame.area()))
        .unwrap();

    let (area, row_map) = {
        let layout = browser.test_layout();
        (layout.left_area, layout.left_row_map.clone())
    };
    let target_row = row_map
        .iter()
        .position(|target| matches!(target, Some(idx) if *idx != browser.cursor()))
        .expect("a non-selected row is painted below the inline hero");
    let target = row_map[target_row].unwrap();
    let position = (area.x, area.y + target_row as u16);

    let message = browser.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: position.0,
        row: position.1,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(
        message,
        Some(Msg::Shell(ShellRequest::BrowserRowContextMenu {
            target,
            anchor: position,
        }))
    );
}

/// A pill click in narrow mode (letter pills painted above the canonical
/// inline list) resolves via the same `pill_regions` map the wide rail uses
/// — `handle_mouse` never branches on breakpoint for pill hits.
#[test]
fn narrow_canonical_list_pill_click_emits_pill_click() {
    let mut browser = BrowserComponent::new_for_kind(BrowserKind::Movies);
    browser.set_content(BrowserContent::from_items(make_items(6)));
    browser.set_focused(true);
    browser.set_narrow_extras(NarrowBrowseExtras {
        hero_placeholder: true,
        show_letter_pills: true,
        ..NarrowBrowseExtras::default()
    });
    let mut terminal = Terminal::new(TestBackend::new(60, 30)).unwrap();
    terminal
        .draw(|frame| browser.view(frame, frame.area()))
        .unwrap();

    let (rect, target) = browser
        .test_layout()
        .selector_tabs
        .iter()
        .find(|(_, target)| *target == 2)
        .copied()
        .expect("third letter pill painted");

    let message = browser.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(
        message,
        Some(Msg::Shell(ShellRequest::BrowserPillClick { target }))
    );
}

#[path = "browser_inline_search_tests.rs"]
mod inline_search_tests;
