use super::music_workspace::MusicWorkspaceComponent;
use crate::app::components::inline_search::{InlineSearchHost, SearchPool};
use crate::app::components::msg::{AlbumCursorKind, ShellRequest};
use crate::app::components::Msg;
use crate::app::render::{LibraryListRenderCtx, MusicWideRenderCtx};
use crate::app::tests::make_item;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, KeyModifiers};
fn context(track_cursor: Option<usize>) -> MusicWideRenderCtx {
    let album = make_item("First Album", "MusicAlbum");
    let mut track = make_item("Track One", "Audio");
    track.index_number = 1;
    let mut second_track = make_item("Track Two", "Audio");
    second_track.index_number = 2;
    MusicWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![album.clone()], 0, 0),
        Some(album),
        "Artist".into(),
        vec![make_item("Artist", "MusicArtist")],
        0,
        vec![("Artist".into(), "2024".into(), "First Album".into())],
        vec![0],
        true,
        Some(vec![track, second_track]),
        false,
        track_cursor,
    )
}

fn grouped_context(
    cursor: usize,
    order: Vec<usize>,
    track_cursor: Option<usize>,
) -> MusicWideRenderCtx {
    let albums: Vec<_> = (0..4)
        .map(|index| make_item(&format!("Album {index}"), "MusicAlbum"))
        .collect();
    MusicWideRenderCtx::new(
        LibraryListRenderCtx::from_items(albums.clone(), cursor, 0),
        Some(albums[cursor].clone()),
        "Artist".into(),
        vec![make_item("Artist", "MusicArtist")],
        0,
        (0..4)
            .map(|index| ("Artist".into(), "2024".into(), format!("Album {index}")))
            .collect(),
        order,
        true,
        None,
        false,
        track_cursor,
    )
}

#[test]
fn music_workspace_keeps_track_cursor_local_between_syncs() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));
    component.set_inline_track_focus_enabled(true);
    // Enter inline track focus locally, then move within it.
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::NONE,
    }));
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Down,
        modifiers: KeyModifiers::NONE,
    }));
    // An ordinary content push (same album) never touches the local track
    // cursor.
    component.set_content(context(None));
    assert_eq!(component.track_cursor(), Some(1));
}

#[test]
fn music_workspace_vertical_move_follows_album_display_order() {
    let albums = vec![
        make_item("Album 0", "MusicAlbum"),
        make_item("Album 1", "MusicAlbum"),
        make_item("Album 2", "MusicAlbum"),
        make_item("Album 3", "MusicAlbum"),
    ];
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(MusicWideRenderCtx::new(
        LibraryListRenderCtx::from_items(albums.clone(), 2, 0),
        Some(albums[2].clone()),
        "Artist".into(),
        vec![make_item("Artist", "MusicArtist")],
        0,
        vec![
            ("Artist".into(), "2024".into(), "Album 0".into()),
            ("Artist".into(), "2023".into(), "Album 1".into()),
            ("Artist".into(), "2022".into(), "Album 2".into()),
            ("Artist".into(), "2021".into(), "Album 3".into()),
        ],
        vec![2, 0, 3, 1],
        true,
        None,
        false,
        None,
    ));
    // The shell re-anchors the album cursor at the navigation event; an
    // ordinary push no longer carries it.
    component.re_anchor(2, 0);
    component.set_album_columns(1);
    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Down,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(component.album_cursor(), 0);
    assert!(matches!(
        message,
        Some(Msg::Shell(ShellRequest::MusicAlbumCursor {
            target: 0,
            kind: AlbumCursorKind::Move,
        }))
    ));
}

#[test]
fn music_workspace_narrow_enter_requests_album_activation() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));
    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(component.track_cursor(), None);
    assert_eq!(message, Some(Msg::Shell(ShellRequest::MusicAlbumActivate)));
}

#[test]
fn music_workspace_enter_sets_track_cursor_when_inline_track_focus_enabled() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));
    component.set_inline_track_focus_enabled(true);
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(component.track_cursor(), Some(0));
    component.set_inline_track_focus_enabled(false);
    assert_eq!(component.track_cursor(), None);
}

#[test]
fn music_workspace_selection_follows_shared_hero_gate_boundaries() {
    for (width, height, wide) in [(81, 7, false), (82, 7, true), (82, 6, false)] {
        let mut component = MusicWorkspaceComponent::new();
        component.set_focused(true);
        component.set_content(context(Some(0)));
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| component.view(frame, Rect::new(0, 0, width, height)))
            .unwrap();
        assert_eq!(
            component.layout().wide_music_right_area.width > 0
                && component.layout().wide_music_right_area.height > 0,
            wide,
            "component layout branch at {width}x{height}"
        );
    }
}

#[test]
fn music_workspace_renders_without_app() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| component.view(frame, frame.area()))
        .unwrap();
    assert!(terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .any(|cell| cell.symbol() == "F"));
}

#[test]
fn music_workspace_horizontal_move_is_ignored_at_one_column() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(grouped_context(1, vec![0, 1, 2, 3], None));
    component.re_anchor(1, 0);
    component.set_album_columns(1);

    for key in [Key::Left, Key::Right, Key::Char('h'), Key::Char('l')] {
        let message = component.on(&Event::Keyboard(KeyEvent {
            code: key,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(message, None);
        assert_eq!(component.album_cursor(), 1);
    }
}

#[test]
fn music_workspace_page_moves_saturate_at_both_ends() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(grouped_context(0, vec![0, 1, 2, 3], None));
    component.set_album_columns(2);
    component.set_page_rows(2);

    for key in [Key::PageUp, Key::PageDown, Key::PageDown, Key::PageUp] {
        component.on(&Event::Keyboard(KeyEvent {
            code: key,
            modifiers: KeyModifiers::NONE,
        }));
    }
    assert_eq!(component.album_cursor(), 0);
}

#[test]
fn music_workspace_track_keys_are_consumed_locally_and_do_not_move_album_cursor() {
    // With a track focused (wide), Down moves the track cursor only: the
    // component consumes the key locally without emitting an album intent.
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    let albums: Vec<_> = (0..4)
        .map(|index| make_item(&format!("Album {index}"), "MusicAlbum"))
        .collect();
    let tracks: Vec<_> = (0..3)
        .map(|i| {
            let mut t = make_item(&format!("Track {i}"), "Audio");
            t.index_number = i + 1;
            t
        })
        .collect();
    component.set_content(MusicWideRenderCtx::new(
        LibraryListRenderCtx::from_items(albums.clone(), 1, 0),
        Some(albums[1].clone()),
        "Artist".into(),
        vec![make_item("Artist", "MusicArtist")],
        0,
        (0..4)
            .map(|index| ("Artist".into(), "2024".into(), format!("Album {index}")))
            .collect(),
        vec![0, 1, 2, 3],
        true,
        Some(tracks),
        false,
        None,
    ));
    component.re_anchor(1, 0);
    component.set_inline_track_focus_enabled(true);
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::NONE,
    }));
    component.set_album_columns(2);

    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Down,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(message, None);
    assert_eq!(component.track_cursor(), Some(1));
    assert_eq!(component.album_cursor(), 1);
}

#[test]
fn music_workspace_enter_on_focused_track_emits_activation() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));
    component.set_inline_track_focus_enabled(true);
    // Enter enters track mode; Enter again activates the focused track.
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::NONE,
    }));
    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(matches!(
        message,
        Some(Msg::Shell(ShellRequest::MusicTrackActivate))
    ));
}

#[test]
fn music_workspace_track_esc_exits_locally_without_forwarding() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));
    component.set_inline_track_focus_enabled(true);
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::NONE,
    }));
    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Esc,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(component.track_cursor(), None);
    assert_eq!(message, None);
}

#[test]
fn music_workspace_album_change_clears_track_focus() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));
    component.set_inline_track_focus_enabled(true);
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(component.track_cursor(), Some(0));

    // A different selected album (group switch / recursive activation)
    // resets the stale track cursor.
    let mut other = make_item("Other Album", "MusicAlbum");
    other.id = "album-2".into();
    component.set_content(MusicWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![other.clone()], 0, 0),
        Some(other),
        "Artist".into(),
        vec![make_item("Artist", "MusicArtist")],
        0,
        vec![("Artist".into(), "2024".into(), "Other Album".into())],
        vec![0],
        true,
        Some(vec![make_item("Other Track", "Audio")]),
        false,
        None,
    ));
    assert_eq!(component.track_cursor(), None);
}

#[test]
fn music_workspace_re_anchor_overrides_prior_local_move() {
    // A shell re-anchor at a navigation event adopts the shell's cursor
    // unconditionally -- the outcome does not depend on whether the user
    // moved the cursor since the previous projection.
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(grouped_context(0, vec![0, 1, 2, 3], None));
    component.re_anchor(0, 0);
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Down,
        modifiers: KeyModifiers::NONE,
    }));
    assert_ne!(
        component.album_cursor(),
        0,
        "local move diverged the cursor"
    );

    component.set_content(grouped_context(2, vec![0, 1, 2, 3], None));
    component.re_anchor(2, 0);
    assert_eq!(component.album_cursor(), 2);
}

#[test]
fn music_workspace_ordinary_push_leaves_album_cursor_alone() {
    // Without a re-anchor, a content push never adopts the shell cursor,
    // and the component holds no stored copy of a previously pushed value.
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(grouped_context(0, vec![0, 1, 2, 3], None));
    component.re_anchor(0, 0);
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Down,
        modifiers: KeyModifiers::NONE,
    }));
    let moved = component.album_cursor();
    assert_ne!(moved, 0);

    component.set_content(grouped_context(3, vec![0, 1, 2, 3], None));
    assert_eq!(component.album_cursor(), moved);
}

#[test]
fn music_workspace_bracket_keys_request_group_switch() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(grouped_context(1, vec![0, 1, 2, 3], None));

    let prev = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('['),
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(
        prev,
        Some(Msg::Shell(ShellRequest::MusicGroupSwitch { delta: -1 }))
    );

    let next = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char(']'),
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(
        next,
        Some(Msg::Shell(ShellRequest::MusicGroupSwitch { delta: 1 }))
    );
}

#[test]
fn music_workspace_bracket_keys_ignored_with_focused_track() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));
    component.set_inline_track_focus_enabled(true);
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(component.track_cursor(), Some(0));

    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('['),
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(message, None);
}

#[test]
fn music_workspace_track_targeted_actions_emit_typed_messages() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));
    component.set_inline_track_focus_enabled(true);
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::NONE,
    }));

    let enqueue = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('a'),
        modifiers: KeyModifiers::CONTROL,
    }));
    assert!(matches!(
        enqueue,
        Some(Msg::Shell(ShellRequest::MusicTrackEnqueue))
    ));

    let menu = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('.'),
        modifiers: KeyModifiers::NONE,
    }));
    assert!(matches!(
        menu,
        Some(Msg::Shell(ShellRequest::MusicTrackContextMenu))
    ));
}

#[test]
fn ctrl_s_on_album_emits_library_shuffle() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));

    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('s'),
        modifiers: KeyModifiers::CONTROL,
    }));

    assert!(matches!(
        message,
        Some(Msg::Shell(ShellRequest::EmbyLibraryShuffle { item }))
            if item.id == "id" && item.item_type == "MusicAlbum"
    ));
}

#[test]
fn ctrl_s_with_track_focus_does_not_shuffle() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));
    component.set_inline_track_focus_enabled(true);
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::NONE,
    }));

    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('s'),
        modifiers: KeyModifiers::CONTROL,
    }));

    assert_eq!(message, None);
    assert_eq!(component.track_cursor(), Some(0));
}

#[test]
fn dot_on_album_emits_library_context_menu() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));
    assert!(matches!(
        component.on(&Event::Keyboard(KeyEvent { code: Key::Char('.'), modifiers: KeyModifiers::NONE })),
        Some(Msg::Shell(ShellRequest::EmbyLibraryContextMenu { item }))
            if item.name == "First Album"
    ));
}

#[test]
fn dot_with_track_focus_emits_track_context_menu() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));
    component.set_inline_track_focus_enabled(true);
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(
        component.on(&Event::Keyboard(KeyEvent {
            code: Key::Char('.'),
            modifiers: KeyModifiers::NONE
        })),
        Some(Msg::Shell(ShellRequest::MusicTrackContextMenu))
    );
}

#[test]
fn slash_on_album_emits_open_inline_search() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));
    assert_eq!(
        component.on(&Event::Keyboard(KeyEvent {
            code: Key::Char('/'),
            modifiers: KeyModifiers::NONE
        })),
        Some(Msg::Shell(ShellRequest::OpenInlineSearch))
    );
}

#[test]
fn slash_with_track_focus_is_unclaimed() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(context(None));
    component.set_inline_track_focus_enabled(true);
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(
        component.on(&Event::Keyboard(KeyEvent {
            code: Key::Char('/'),
            modifiers: KeyModifiers::NONE
        })),
        None
    );
}

#[test]
fn dot_empty_list_is_unclaimed() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(MusicWideRenderCtx::new(
        LibraryListRenderCtx::from_items(Vec::new(), 0, 0),
        None,
        "Artist".into(),
        Vec::new(),
        0,
        Vec::new(),
        Vec::new(),
        true,
        None,
        false,
        None,
    ));
    assert_eq!(
        component.on(&Event::Keyboard(KeyEvent {
            code: Key::Char('.'),
            modifiers: KeyModifiers::NONE
        })),
        None
    );
}

#[test]
fn ctrl_p_empty_list_is_unclaimed() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(MusicWideRenderCtx::new(
        LibraryListRenderCtx::from_items(Vec::new(), 0, 0),
        None,
        "Artist".into(),
        Vec::new(),
        0,
        Vec::new(),
        Vec::new(),
        true,
        None,
        false,
        None,
    ));

    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('p'),
        modifiers: KeyModifiers::CONTROL,
    }));

    assert_eq!(message, None);
}

/// Wide Inline Search suppresses the grouped album rail's artist headers
/// (design.md D3) and paints flat scored results instead, while the Hero
/// pane painted alongside it remains visible.
#[test]
fn music_workspace_wide_search_hides_grouped_rows_and_paints_flat_results() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    // `grouped_context` groups all four albums under a single "Artist"
    // heading; the ordinary wide rail would paint that heading above them.
    component.set_content(grouped_context(0, vec![0, 1, 2, 3], None));

    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('/'),
        modifiers: KeyModifiers::NONE,
    }));
    assert!(component.inline_search().is_active());
    component
        .inline_search_mut()
        .set_pool(SearchPool::Items(vec![make_item(
            "Zeta Album Match",
            "MusicAlbum",
        )]));

    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal
        .draw(|frame| component.view(frame, frame.area()))
        .unwrap();

    let list_area = component.inline_search().layout().left_area;
    assert!(list_area.width > 0 && list_area.height > 0);
    assert!(
        list_area.x > 0,
        "search paints in the browser pane, not over the Hero pane: {list_area:?}"
    );

    let buffer = terminal.backend().buffer();
    let mut found_result = false;
    let mut found_heading = false;
    for y in list_area.y..list_area.y + list_area.height {
        let row: String = (list_area.x..list_area.x + list_area.width)
            .map(|x| buffer.cell((x, y)).unwrap().symbol())
            .collect();
        if row.contains("Zeta Album Match") {
            found_result = true;
        }
        if row.contains("Artist") {
            found_heading = true;
        }
    }
    assert!(
        found_result,
        "flat search result painted in the browser pane"
    );
    assert!(
        !found_heading,
        "no artist heading painted in the search list area"
    );

    // The Hero pane (right of the browser pane) remains painted.
    let mut hero_painted = false;
    for y in 0..30 {
        let row: String = (list_area.x + list_area.width..120)
            .map(|x| buffer.cell((x, y)).unwrap().symbol())
            .collect();
        if row.contains("Album 0") {
            hero_painted = true;
        }
    }
    assert!(hero_painted, "Hero pane remains visible during search");
}

/// Dismissing search restores the prior album position (design.md D4):
/// Inline Search's cursor is local to the control, so the component's own
/// `album_cursor` -- never touched while search is open -- is unchanged.
#[test]
fn music_workspace_dismiss_restores_prior_album_position() {
    let mut component = MusicWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(grouped_context(0, vec![0, 1, 2, 3], None));
    component.re_anchor(2, 0);
    assert_eq!(component.album_cursor(), 2);

    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('/'),
        modifiers: KeyModifiers::NONE,
    }));
    assert!(component.inline_search().is_active());

    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Esc,
        modifiers: KeyModifiers::NONE,
    }));

    assert_eq!(
        message, None,
        "Escape dismisses locally with no shell effect"
    );
    assert!(!component.inline_search().is_active());
    assert_eq!(
        component.album_cursor(),
        2,
        "the prior album position is unchanged by the local search session"
    );
}
