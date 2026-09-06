use super::inline_search::{InlineSearchHost, SearchPool};
use super::msg::{Msg, ShellRequest, TvHit};
use super::tv_workspace::TvWorkspaceComponent;
use crate::app::render::{LibraryListRenderCtx, TvWideRenderCtx};
use crate::app::tests::make_item;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{
    Event, Key, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

#[test]
fn tv_series_clicks_use_the_rendered_series_row_for_left_and_right_clicks() {
    let mut component = TvWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(
            vec![
                make_item("Series A", "Series"),
                make_item("Series B", "Series"),
            ],
            0,
            0,
        ),
        None,
        None,
        0,
        None,
        false,
    ));
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    terminal
        .draw(|frame| component.view(frame, frame.area()))
        .unwrap();
    let layout = component.test_layout();
    let row = layout.tv_wide_list_area.y + 1;
    let col = layout.tv_wide_list_area.x;

    let left = component.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(matches!(
        left,
        Some(Msg::Shell(ShellRequest::TvHitClick {
            hit: TvHit::SeriesRow(1),
        }))
    ));

    let right = component.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(matches!(
        right,
        Some(Msg::Shell(ShellRequest::TvHitContextMenu {
            hit: TvHit::SeriesRow(1),
            ..
        }))
    ));
}

#[test]
fn tv_right_selects_first_episode_for_activation() {
    let mut series = make_item("Series", "Series");
    series.id = "series-id".into();
    let mut season = make_item("Season 1", "Season");
    season.id = "season-id".into();
    let mut episode = make_item("Episode 1", "Episode");
    episode.id = "episode-id".into();
    let detail = crate::app::SeriesDetail {
        seasons: vec![season],
        episodes: [("season-id".into(), vec![episode])].into_iter().collect(),
    };
    let mut component = TvWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![series.clone()], 0, 0),
        Some(series),
        Some(detail),
        0,
        None,
        false,
    ));

    let key = |code| {
        Event::Keyboard(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
        })
    };
    assert!(matches!(
        component.on(&key(Key::Right)),
        Some(Msg::Shell(ShellRequest::TvMoveColumn { delta: 1 }))
    ));
    assert_eq!(
        component.episode_activation_selection(),
        Some(("series-id".into(), 0, 0))
    );
    assert!(matches!(
        component.on(&key(Key::Enter)),
        Some(Msg::Shell(ShellRequest::TvEpisodeActivate))
    ));
}

#[test]
fn tv_content_refresh_clamps_episode_cursor_and_handles_empty_season() {
    let mut series = make_item("Series", "Series");
    series.id = "series-id".into();
    let mut season = make_item("Season 1", "Season");
    season.id = "season-id".into();
    let episode = |name: &str, id: &str| {
        let mut item = make_item(name, "Episode");
        item.id = id.into();
        item
    };
    let detail = |episodes| crate::app::SeriesDetail {
        seasons: vec![season.clone()],
        episodes: [("season-id".into(), episodes)].into_iter().collect(),
    };
    let mut component = TvWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![series.clone()], 0, 0),
        Some(series.clone()),
        Some(detail(vec![
            episode("Episode 1", "episode-1"),
            episode("Episode 2", "episode-2"),
            episode("Episode 3", "episode-3"),
        ])),
        0,
        None,
        false,
    ));
    let key = |code| {
        Event::Keyboard(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
        })
    };
    component.on(&key(Key::Right));
    component.on(&key(Key::Down));
    component.on(&key(Key::Down));
    assert_eq!(
        component.episode_activation_selection(),
        Some(("series-id".into(), 0, 2))
    );

    // An unavailable detail refresh must not erase the mounted component's
    // local episode cursor while the data is loading.
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![series.clone()], 0, 0),
        Some(series.clone()),
        None,
        0,
        Some(2),
        false,
    ));
    assert_eq!(
        component.episode_activation_selection(),
        Some(("series-id".into(), 0, 2))
    );

    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![series.clone()], 0, 0),
        Some(series.clone()),
        Some(detail(vec![episode("Episode 1", "episode-1")])),
        0,
        Some(2),
        false,
    ));
    assert_eq!(
        component.episode_activation_selection(),
        Some(("series-id".into(), 0, 0))
    );
    assert!(matches!(
        component.on(&Event::Keyboard(KeyEvent {
            code: Key::Enter,
            modifiers: KeyModifiers::NONE
        })),
        Some(Msg::Shell(ShellRequest::TvEpisodeActivate))
    ));

    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![series.clone()], 0, 0),
        Some(series),
        Some(detail(Vec::new())),
        0,
        Some(0),
        false,
    ));
    assert_eq!(component.episode_activation_selection(), None);
}

#[test]
fn tv_keyboard_leaves_key_unclaimed_when_queue_is_focused() {
    let mut component = TvWorkspaceComponent::new();
    component.set_focused(false);
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(
            vec![
                make_item("Series A", "Series"),
                make_item("Series B", "Series"),
            ],
            0,
            0,
        ),
        None,
        None,
        0,
        None,
        true,
    ));

    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Down,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(message, None);
    assert_eq!(component.cursor(), 0);
}

#[test]
fn tv_episode_brackets_with_modifiers_are_unclaimed() {
    let mut component = TvWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![make_item("Series", "Series")], 0, 0),
        None,
        None,
        0,
        Some(0),
        false,
    ));

    for (code, modifiers) in [
        (Key::Char('['), KeyModifiers::CONTROL),
        (Key::Char(']'), KeyModifiers::ALT),
    ] {
        let message = component.on(&Event::Keyboard(KeyEvent { code, modifiers }));
        assert_eq!(message, None);
    }
    assert_eq!(
        component.on(&Event::Keyboard(KeyEvent {
            code: Key::Char(' '),
            modifiers: KeyModifiers::NONE,
        })),
        None
    );
}

#[test]
fn tv_grouped_cursor_mirrors_rendered_sorted_rows() {
    let mut items = vec![
        make_item("Zulu", "Series"),
        make_item("Alpha", "Series"),
        make_item("Beta", "Series"),
    ];
    items.extend((3..50).map(|index| make_item(&format!("Series {index}"), "Series")));

    let mut component = TvWorkspaceComponent::new();

    component.set_focused(true);
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(items, 1, 0),
        None,
        None,
        0,
        None,
        false,
    ));
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    terminal
        .draw(|frame| component.view(frame, frame.area()))
        .unwrap();
    assert_eq!(&component.test_layout().left_sorted_indices[..2], &[1, 2]);

    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Down,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(matches!(
        message,
        Some(Msg::Shell(ShellRequest::TvMoveRows { rows: 1 }))
    ));
    assert_eq!(component.cursor(), 2);
}

#[test]
fn tv_keyboard_uses_typed_requests_and_routes_brackets_by_pane() {
    let mut component = TvWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(
            vec![
                make_item("Series A", "Series"),
                make_item("Series B", "Series"),
            ],
            0,
            0,
        ),
        None,
        None,
        0,
        None,
        true,
    ));

    let key = |code| {
        Event::Keyboard(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
        })
    };
    assert!(matches!(
        component.on(&key(Key::Down)),
        Some(Msg::Shell(ShellRequest::TvMoveRows { rows: 1 }))
    ));
    assert!(matches!(
        component.on(&key(Key::Char('['))),
        Some(Msg::Shell(ShellRequest::TvCycleLetterPill { delta: -1 }))
    ));
    assert!(matches!(
        component.on(&key(Key::Enter)),
        Some(Msg::Shell(ShellRequest::TvActivate { item }))
            if item.name == "Series B" && item.item_type == "Series"
    ));
    assert!(matches!(
        component.on(&key(Key::Up)),
        Some(Msg::Shell(ShellRequest::TvEpisodeMove { delta: -1 }))
    ));
    assert!(matches!(
        component.on(&key(Key::Char(']'))),
        Some(Msg::Shell(ShellRequest::TvSeasonMove { delta: 1 }))
    ));
    assert!(matches!(
        component.on(&key(Key::Esc)),
        Some(Msg::Shell(ShellRequest::TvBack))
    ));

    component.on(&key(Key::Enter));
    assert!(matches!(
        component.on(&key(Key::Enter)),
        Some(Msg::Shell(ShellRequest::TvEpisodeActivate))
    ));
}

#[test]
fn dot_emits_library_context_menu() {
    let mut component = TvWorkspaceComponent::new();
    component.set_focused(true);
    let series = make_item("Series", "Series");
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![series], 0, 0),
        None,
        None,
        0,
        None,
        false,
    ));
    assert!(matches!(
        component.on(&Event::Keyboard(KeyEvent { code: Key::Char('.'), modifiers: KeyModifiers::NONE })),
        Some(Msg::Shell(ShellRequest::EmbyLibraryContextMenu { item })) if item.name == "Series"
    ));
}

#[test]
fn slash_emits_open_inline_search() {
    let mut component = TvWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![make_item("Series", "Series")], 0, 0),
        None,
        None,
        0,
        None,
        false,
    ));
    assert_eq!(
        component.on(&Event::Keyboard(KeyEvent {
            code: Key::Char('/'),
            modifiers: KeyModifiers::NONE
        })),
        Some(Msg::Shell(ShellRequest::OpenInlineSearch))
    );
}

/// Wide hero Wide paints Inline Search in the right rail (design.md D3):
/// the shared bordered input/result painter lands in the series rail, not
/// the episode/Hero pane to its left, which remains visible.
#[test]
fn wide_tv_search_paints_in_right_rail_not_left_pane() {
    let mut component = TvWorkspaceComponent::new();
    component.set_focused(true);
    let mut series = make_item("Series", "Series");
    series.id = "series-id".into();
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![series], 0, 0),
        None,
        None,
        0,
        None,
        false,
    ));
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('/'),
        modifiers: KeyModifiers::NONE,
    }));
    assert!(component.inline_search().is_active());
    component
        .inline_search_mut()
        .set_pool(SearchPool::Items(vec![make_item(
            "Search Result Alpha",
            "Series",
        )]));

    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    terminal
        .draw(|frame| component.view(frame, frame.area()))
        .unwrap();

    let list_area = component.inline_search().layout().left_area;
    let left_pane = component.test_layout().tv_wide_left_area;
    assert!(list_area.width > 0 && list_area.height > 0);
    assert!(
        list_area.x >= left_pane.x + left_pane.width,
        "search paints in the right rail, after the episode/Hero pane: \
         list_area={list_area:?} left_pane={left_pane:?}"
    );

    let buffer = terminal.backend().buffer();
    let mut found_in_rail = false;
    let mut found_in_left_pane = false;
    for y in list_area.y..list_area.y + list_area.height {
        let rail_row: String = (list_area.x..list_area.x + list_area.width)
            .map(|x| buffer.cell((x, y)).unwrap().symbol())
            .collect();
        if rail_row.contains("Search Result Alpha") {
            found_in_rail = true;
        }
        let left_row: String = (left_pane.x..left_pane.x + left_pane.width)
            .map(|x| buffer.cell((x, y)).unwrap().symbol())
            .collect();
        if left_row.contains("Search Result Alpha") {
            found_in_left_pane = true;
        }
    }
    assert!(found_in_rail, "search result row painted in the right rail");
    assert!(
        !found_in_left_pane,
        "search result must not paint in the episode/Hero pane"
    );
}

#[test]
fn dot_with_episode_focus_targets_series() {
    let mut series = make_item("Series", "Series");
    series.id = "series-id".into();
    let mut season = make_item("Season", "Season");
    season.id = "season-id".into();
    let mut episode = make_item("Episode", "Episode");
    episode.id = "episode-id".into();
    episode.series_id = series.id.clone();
    let detail = crate::app::SeriesDetail {
        seasons: vec![season],
        episodes: [("season-id".into(), vec![episode])].into_iter().collect(),
    };
    let mut component = TvWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![series.clone()], 0, 0),
        Some(series),
        Some(detail),
        0,
        None,
        false,
    ));
    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Right,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(matches!(
        component.on(&Event::Keyboard(KeyEvent { code: Key::Char('.'), modifiers: KeyModifiers::NONE })),
        Some(Msg::Shell(ShellRequest::EmbyLibraryContextMenu { item }))
            if item.id == "series-id" && item.item_type == "Series"
    ));
}

#[test]
fn ctrl_r_emits_library_rescan() {
    let mut component = TvWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![make_item("Series", "Series")], 0, 0),
        None,
        None,
        0,
        None,
        false,
    ));

    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('r'),
        modifiers: KeyModifiers::CONTROL,
    }));

    assert_eq!(message, Some(Msg::Shell(ShellRequest::EmbyLibraryRescan)));
}

#[test]
fn ctrl_w_emits_library_toggle_watched() {
    let mut series = make_item("Series", "Series");
    series.id = "series-id".into();
    let mut season = make_item("Season 1", "Season");
    season.id = "season-id".into();
    let mut episode = make_item("Episode 1", "Episode");
    episode.id = "episode-id".into();
    episode.series_id = series.id.clone();
    let detail = crate::app::SeriesDetail {
        seasons: vec![season],
        episodes: [("season-id".into(), vec![episode])].into_iter().collect(),
    };
    let mut component = TvWorkspaceComponent::new();
    component.set_focused(true);
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![series.clone()], 0, 0),
        Some(series.clone()),
        Some(detail),
        0,
        None,
        false,
    ));

    // The local Episodes pane is focused, but legacy library actions target
    // the selected series-list item rather than the highlighted episode.
    assert!(matches!(
        component.on(&Event::Keyboard(KeyEvent {
            code: Key::Right,
            modifiers: KeyModifiers::NONE,
        })),
        Some(Msg::Shell(ShellRequest::TvMoveColumn { delta: 1 }))
    ));
    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('w'),
        modifiers: KeyModifiers::CONTROL,
    }));

    assert!(matches!(
        message,
        Some(Msg::Shell(ShellRequest::EmbyLibraryToggleWatched { item }))
            if item.id == "series-id" && item.item_type == "Series"
    ));
}
