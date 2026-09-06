use super::*;

/// `/` opens the embedded Inline Search control locally (design.md D1/D4)
/// and still emits `OpenInlineSearch` for the shell-side full-library-load
/// work the control has no authority over.
#[test]
fn emby_browser_slash_opens_inline_search() {
    let mut browser = BrowserComponent::new();
    browser.set_content(BrowserContent::from_items(make_items(3)));
    browser.set_focused(true);
    assert!(!browser.inline_search().is_active());

    let message = browser.handle_tui_key(TuiKeyEvent {
        code: Key::Char('/'),
        modifiers: KeyModifiers::NONE,
    });

    assert!(browser.inline_search().is_active());
    assert_eq!(message, Some(Msg::Shell(ShellRequest::OpenInlineSearch)));
}

/// While search is open, a character that is otherwise a list shortcut (`r`
/// -> `BrowserRefresh`) is appended to the query instead of running the
/// shortcut, and the component returns immediately without an ordinary
/// `Msg` (design.md D4).
#[test]
fn emby_browser_search_open_shortcut_letter_becomes_query_text() {
    let mut browser = BrowserComponent::new();
    browser.set_content(BrowserContent::from_items(make_items(3)));
    browser.set_focused(true);
    browser.handle_tui_key(TuiKeyEvent {
        code: Key::Char('/'),
        modifiers: KeyModifiers::NONE,
    });

    let message = browser.handle_tui_key(TuiKeyEvent {
        code: Key::Char('r'),
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(browser.inline_search().query(), "r");
    assert_eq!(
        message, None,
        "shortcut letter must not reach the ordinary handler while search is open"
    );
}

/// Wide hero Wide paints the shared one-row Inline Search bar and results in
/// the right rail (design.md D3), without also painting the ordinary browser
/// pill presentation.
#[test]
fn emby_browser_wide_right_rail_paints_inline_search() {
    let mut browser = BrowserComponent::new_for_kind(BrowserKind::Movies);
    browser.configure_wide_movies(false, true);
    // The Hero pane's own selected-item title ("Focused Movie") is
    // deliberately distinct from the search result's name below, so a text
    // match can only come from an actual painted result row in the right
    // rail, never from the Hero pane sharing a screen row.
    browser.set_content(BrowserContent::from_items(vec![make_item(
        "Focused Movie",
        "Movie",
    )]));
    browser.set_focused(true);
    browser.handle_tui_key(TuiKeyEvent {
        code: Key::Char('/'),
        modifiers: KeyModifiers::NONE,
    });
    browser.inline_search_mut().set_pool(SearchPool::Items(vec![
        make_item("Search Result Alpha", "Movie"),
        make_item("Search Result Beta", "Movie"),
    ]));

    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal
        .draw(|frame| browser.view(frame, frame.area()))
        .unwrap();

    // The embedded control's own painted geometry (not the parent's `layout`)
    // is what its own mouse/page-size math reads.
    let list_area = browser.inline_search().layout().left_area;
    assert!(
        list_area.width > 0 && list_area.height > 0,
        "search result geometry must be published"
    );
    assert!(
        list_area.x > 0,
        "search paints in the right rail, not flush with the Hero pane at x=0: {list_area:?}"
    );

    let buffer = terminal.backend().buffer();
    let frame_text: String = (0..buffer.area().height)
        .flat_map(|y| (0..buffer.area().width).map(move |x| buffer.cell((x, y)).unwrap().symbol()))
        .collect();
    assert!(
        frame_text.contains("SEARCH:"),
        "shared one-row search bar painted"
    );
    assert!(
        !frame_text.contains("┌"),
        "ordinary bordered search input not painted"
    );
    assert!(
        !frame_text.contains("A–C"),
        "ordinary letter pills are replaced while search is active"
    );
    assert!(
        browser.test_layout().selector_tabs.is_empty(),
        "ordinary pill hit regions are unavailable while search is active"
    );

    let mut found = false;
    for y in list_area.y..list_area.y + list_area.height {
        // Scan only the right rail's own x-range: the Hero pane paints its
        // own title at x=0 on possibly-overlapping rows, so scanning the
        // full frame width would let that false-positive the assertion.
        let row: String = (list_area.x..list_area.x + list_area.width)
            .map(|x| buffer.cell((x, y)).unwrap().symbol())
            .collect();
        if row.contains("Search Result Alpha") {
            found = true;
        }
    }
    assert!(found, "matching result row painted in the right rail");

    // A click on the painted result row resolves through the embedded
    // control's own geometry (design.md D6), proving it (not the parent's
    // `layout`) is what mouse handling actually reads.
    let message = browser.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: list_area.x,
        row: list_area.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(message, None, "search mouse handling never emits a Msg");
    assert_eq!(browser.inline_search().cursor(), 0);

    // A press that begins in the Inline Search bar (above the result list)
    // and releases on the second result row still lands on that row, so its
    // ordinary Ctrl+P/S/A and context-menu actions act on it.
    browser.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: list_area.x,
        row: list_area.y.saturating_sub(1),
        modifiers: KeyModifiers::NONE,
    }));
    let message = browser.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: list_area.x,
        row: list_area.y + 1,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(message, None, "search mouse handling never emits a Msg");
    assert_eq!(
        browser.inline_search().cursor(),
        1,
        "search-bar-to-row gesture selects the row it ends on"
    );

    // A right click on the first result row moves the cursor there and asks
    // the host to open its ordinary item-based context menu for that result
    // (P1: context-menu actions stay available while search is open).
    let message = browser.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: list_area.x,
        row: list_area.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(browser.inline_search().cursor(), 0);
    assert!(
        matches!(
            message,
            Some(Msg::Shell(ShellRequest::BrowserContextMenu { ref item }))
                if item.name == "Search Result Alpha"
        ),
        "right click on a result row opens its context menu: {message:?}"
    );
}
