use super::home::HomeComponent;
use super::media_list::{MediaListRow, MediaSemanticState};
use super::msg::{Msg, ShellRequest};
use mbv_core::playback_queue::QueueItem;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{
    Event, Key, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

#[test]
fn home_down_moves_the_component_cursor_without_app_state() {
    let mut home = HomeComponent::new();
    home.set_focused(true);
    home.set_content(
        vec![QueueItem::Emby(Box::new(crate::app::tests::make_item(
            "one", "Movie",
        )))],
        vec![(
            "Movies".into(),
            crate::app::types_playback::HomeLatestSource::Emby("movies".into()),
            vec![QueueItem::Emby(Box::new(crate::app::tests::make_item(
                "two", "Movie",
            )))],
        )],
        false,
    );

    let msg = home.on(&Event::Keyboard(KeyEvent {
        code: Key::Down,
        modifiers: KeyModifiers::NONE,
    }));

    assert_eq!(
        home.cursor(),
        0,
        "Home movement stays within the selected section"
    );
    assert_eq!(home.section(), 0);
    assert_eq!(msg, None);
}

fn two_section_home() -> HomeComponent {
    let mut home = HomeComponent::new();
    // Home keyboard ownership requires the Library panel to be focused; the
    // keyboard tests below exercise that focused state.
    home.set_focused(true);
    home.set_content(
        vec![
            QueueItem::Emby(Box::new(crate::app::tests::make_item("cw1", "Movie"))),
            QueueItem::Emby(Box::new(crate::app::tests::make_item("cw2", "Movie"))),
        ],
        vec![(
            "Movies".into(),
            crate::app::types_playback::HomeLatestSource::Emby("movies".into()),
            vec![QueueItem::Emby(Box::new(crate::app::tests::make_item(
                "latest1", "Movie",
            )))],
        )],
        false,
    );
    home
}

#[test]
fn home_wheel_steps_one_row_and_ignores_pills() {
    let mut home = two_section_home();
    let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
    terminal
        .draw(|frame| home.view(frame, frame.area()))
        .unwrap();
    let list = home.menu_placement_geometry().0;
    let event = |kind| {
        Event::Mouse(MouseEvent {
            kind,
            column: list.x + 1,
            row: list.y + 1,
            modifiers: KeyModifiers::NONE,
        })
    };

    assert_eq!(home.cursor(), 0);
    assert_eq!(home.on(&event(MouseEventKind::ScrollDown)), None);
    assert_eq!(home.cursor(), 1);
    home.reset_mouse_gestures_for_test();
    assert_eq!(home.on(&event(MouseEventKind::ScrollUp)), None);
    assert_eq!(home.cursor(), 0);

    home.reset_mouse_gestures_for_test();
    assert_eq!(
        home.on(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })),
        None
    );
    assert_eq!(home.cursor(), 0, "wheel over pills/chrome must be ignored");
}

fn key(code: Key) -> Event<crate::app::components::UserEvent> {
    Event::Keyboard(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn home_keys_stay_unclaimed_while_the_queue_panel_is_focused() {
    // Unfocused (Queue panel focused): Home must not claim or mutate
    // anything. Queue owns the focused event and the router owns globals.
    let mut home = two_section_home();
    home.set_focused(false);
    for code in [Key::Down, Key::Char(']'), Key::Enter] {
        let msg = home.on(&key(code));
        assert_eq!(msg, None, "unfocused {code:?} must stay unclaimed");
    }
    assert_eq!(
        home.cursor(),
        0,
        "queue-focused keys must not move Home's cursor"
    );
    assert_eq!(
        home.section(),
        0,
        "queue-focused keys must not move Home's pill"
    );
}

#[test]
fn home_alt_navigation_stays_unclaimed() {
    let mut home = two_section_home();

    let message = home.on(&Event::Keyboard(KeyEvent {
        code: Key::Up,
        modifiers: KeyModifiers::ALT,
    }));

    assert_eq!(home.cursor(), 0, "Alt+Up must not move the local cursor");
    assert_eq!(message, None, "global Alt navigation belongs to the router");
}

#[test]
fn enter_emits_typed_play_at_the_flat_cursor() {
    let mut home = two_section_home();
    home.on(&key(Key::Down));
    let msg = home.on(&key(Key::Enter));
    assert_eq!(msg, Some(Msg::Shell(ShellRequest::HomePlay(1))));
}

#[test]
fn home_alt_enter_stays_component_owned() {
    let mut home = two_section_home();
    let msg = home.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::ALT,
    }));
    assert_eq!(msg, Some(Msg::Shell(ShellRequest::HomePlay(0))));
}

#[test]
fn ctrl_enter_and_ctrl_a_enqueue_at_the_flat_cursor() {
    // Task 5.3d, Home typed-effect keyboard ownership: both the Ctrl+Enter
    // and Ctrl+A chords enqueue the component's flat cursor target via the
    // typed `ShellRequest::HomeEnqueue`, mirroring the two legacy
    // `handle_cw_key` enqueue arms they replace.
    let mut home = two_section_home();
    home.on(&key(Key::Down));
    let msg = home.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::CONTROL,
    }));
    assert_eq!(msg, Some(Msg::Shell(ShellRequest::HomeEnqueue(1))));

    let msg = home.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('a'),
        modifiers: KeyModifiers::CONTROL,
    }));
    assert_eq!(msg, Some(Msg::Shell(ShellRequest::HomeEnqueue(1))));
}

#[test]
fn delete_emits_typed_remove_at_the_flat_cursor() {
    let mut home = two_section_home();
    let msg = home.on(&key(Key::Delete));
    assert_eq!(msg, Some(Msg::Shell(ShellRequest::HomeDelete(0))));
}

#[test]
fn section_bracket_moves_into_the_next_section_and_persists() {
    let mut home = two_section_home();
    let msg = home.on(&key(Key::Char(']')));
    assert_eq!(home.section(), 1);
    assert_eq!(home.cursor(), 2, "cursor lands in the new section's range");
    assert_eq!(msg, Some(Msg::Shell(ShellRequest::HomeSectionSelected(1))));
}

/// Task 5.3d, numeric Home section deletion: an empty latest pill is still a
/// selectable section (the component is the sole owner of the numeric
/// section). An empty pill yields a valid selected section (so it remains
/// discoverable) while its (empty) range leaves the flat cursor clamped to 0.
#[test]
fn empty_latest_pill_is_a_selectable_section() {
    let mut home = HomeComponent::new();
    home.set_focused(true);
    home.set_content(
        vec![],
        vec![(
            "Podcasts".into(),
            crate::app::types_playback::HomeLatestSource::Audiobookshelf("abs-pod".into()),
            vec![],
        )],
        false,
    );

    let msg = home.on(&key(Key::Char(']')));
    assert_eq!(home.section(), 1, "empty pill must be selectable");
    assert_eq!(msg, Some(Msg::Shell(ShellRequest::HomeSectionSelected(1))));
    assert_eq!(
        home.cursor(),
        0,
        "an empty section leaves the cursor clamped"
    );
}

/// Task 5.3d, numeric Home section deletion: `source_for_section` keeps the
/// off-by-one rule in the component — section 0 (Continue Watching) is `None`
/// (the empty persistence sentinel), section 1 maps to `latest[0]`, and an
/// out-of-range index is `None`.
#[test]
fn source_for_section_maps_numeric_to_semantic_source() {
    let home = two_section_home();
    assert_eq!(
        home.source_for_section(0),
        None,
        "Continue Watching resolves to None"
    );
    assert_eq!(
        home.source_for_section(1),
        Some(crate::app::types_playback::HomeLatestSource::Emby(
            "movies".into()
        )),
        "section 1 resolves to latest[0]'s source"
    );
    assert_eq!(
        home.source_for_section(2),
        None,
        "out-of-range section is None"
    );
}

#[test]
fn unmatched_key_stays_unclaimed() {
    let mut home = two_section_home();
    let msg = home.on(&key(Key::Char('v')));
    assert_eq!(msg, None);
}
#[test]
fn ctrl_w_emits_toggle_watched_without_a_cursor_payload() {
    let mut home = two_section_home();
    let msg = home.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('w'),
        modifiers: KeyModifiers::CONTROL,
    }));
    assert_eq!(msg, Some(Msg::Shell(ShellRequest::HomeToggleWatched)));
}

#[test]
fn dot_emits_home_context_menu_with_component_target() {
    let mut home = two_section_home();
    let target = crate::app::tests::make_item("cw-target", "Movie");
    home.set_continue_watching_item(Some(target.clone()));
    let msg = home.on(&key(Key::Char('.')));
    assert_eq!(
        msg,
        Some(Msg::Shell(ShellRequest::HomeContextMenu {
            home_cw_selected: true,
            cw_item: Some(target),
        }))
    );
}

fn emby_queue_item(id: &str) -> QueueItem {
    let mut item = crate::app::tests::make_item(id, "Movie");
    item.id = id.to_owned();
    QueueItem::Emby(Box::new(item))
}

fn cw_home(count: usize) -> HomeComponent {
    let mut home = HomeComponent::new();
    home.set_focused(true);
    home.set_content(
        (0..count)
            .map(|i| emby_queue_item(&format!("cw{i}")))
            .collect(),
        vec![],
        false,
    );
    home
}

/// Task 2.1: only the active section is projected, as `Item` rows (Home has
/// no `Heading`/`Spacer`), so structural-row index == selectable index; each
/// row keeps the legacy Home row's display name, runtime, and resume badge,
/// and carries no played/active marker.
#[test]
fn active_section_projects_item_rows_with_content_and_parallel_indices() {
    use crate::app::ui_util::fmt_duration_short;
    use mbv_core::api::TICKS_PER_SECOND;

    let mut episode = crate::app::tests::make_item("Chapter One", "Episode");
    episode.id = "ep-1".into();
    episode.series_name = "The Series".into();
    episode.runtime_ticks = 90 * TICKS_PER_SECOND;
    episode.playback_position_ticks = 45 * TICKS_PER_SECOND;

    let mut home = HomeComponent::new();
    home.set_content(
        vec![QueueItem::Emby(Box::new(episode)), emby_queue_item("cw1")],
        vec![(
            "Movies".into(),
            crate::app::types_playback::HomeLatestSource::Emby("movies".into()),
            vec![emby_queue_item("latest0"), emby_queue_item("latest1")],
        )],
        false,
    );

    let rows = home.test_active_rows();
    assert_eq!(rows.len(), 2, "only the active Continue Watching section");
    assert_eq!(
        rows.iter()
            .filter_map(MediaListRow::selectable_target)
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["ep-1", "cw1"],
        "target is the stable item id; row N is selectable row N",
    );
    let MediaListRow::Item {
        primary,
        duration,
        trailing,
        semantic_state,
        ..
    } = &rows[0]
    else {
        unreachable!("projected Home rows are Item rows")
    };
    assert_eq!(primary, "The Series Chapter One", "episode display name");
    assert_eq!(
        duration.as_deref(),
        Some(fmt_duration_short(90).as_str()),
        "runtime surfaces as the row duration",
    );
    assert_eq!(trailing.as_deref(), Some("50%"), "resume badge surfaces");
    assert_eq!(
        *semantic_state,
        MediaSemanticState::Ordinary,
        "Home rows carry no played/active marker",
    );
}

/// Task 2.2: an ordinary content refresh keeps the selected target and
/// locally clamps; it never adopts a parent cursor/scroll.
#[test]
fn ordinary_refresh_preserves_target_and_locally_clamps() {
    let mut home = cw_home(8);
    for _ in 0..3 {
        home.on(&key(Key::Down));
    }
    assert_eq!(home.cursor(), 3);

    home.set_content(
        (0..8).map(|i| emby_queue_item(&format!("cw{i}"))).collect(),
        vec![],
        false,
    );
    assert_eq!(home.cursor(), 3, "same content: selected target retained");
    assert_eq!(
        home.test_active_scroll(),
        0,
        "refresh adopts no parent scroll"
    );

    home.set_content(
        (0..2).map(|i| emby_queue_item(&format!("cw{i}"))).collect(),
        vec![],
        false,
    );
    assert_eq!(home.cursor(), 1, "shrunk content clamps to the last row");
}

/// Task 2.2: a breakpoint transition performs exactly one `ViewportAnchor`
/// handoff — the incoming control keeps the selected target and is seeded
/// with the outgoing control's screen-row offset (a fresh mount would rest at
/// the top).
#[test]
fn breakpoint_transition_hands_off_one_viewport_anchor() {
    let mut home = cw_home(40);
    for _ in 0..35 {
        home.on(&key(Key::Down));
    }
    assert_eq!(home.cursor(), 35);

    let mut wide = Terminal::new(TestBackend::new(200, 30)).unwrap();
    wide.draw(|frame| home.view(frame, frame.area())).unwrap();

    let mut narrow = Terminal::new(TestBackend::new(60, 30)).unwrap();
    narrow.draw(|frame| home.view(frame, frame.area())).unwrap();

    assert_eq!(home.cursor(), 35, "selected target survives the transition");
    assert!(
        home.test_active_scroll() > 0,
        "the handoff seeded the incoming control's resting offset from the anchor",
    );
}

#[test]
fn home_renders_content_without_app_state() {
    let mut home = two_section_home();
    let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();

    terminal
        .draw(|frame| home.view(frame, frame.area()))
        .unwrap();

    let output: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol().to_owned())
        .collect();
    assert!(output.contains("cw1"));
}

#[test]
fn home_right_click_uses_the_rendered_row_target() {
    let mut home = two_section_home();
    let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
    terminal
        .draw(|frame| home.view(frame, frame.area()))
        .unwrap();

    // Right-click on a rendered row resolves the painted target and moves
    // the component-local cursor to it, so the emitted `ContextMenu` region
    // and `home.cursor()` agree on the row under the click.
    let (rect, target) = home.test_hitmap()[1];
    let row_message = home.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(
        home.cursor(),
        target,
        "right-click moves the local cursor to the painted row"
    );
    assert!(matches!(
        row_message,
        Some(Msg::Shell(ShellRequest::HomeRowContextMenu { .. }))
    ));

    // A right-click on rendered blank space inside the list (the rows below
    // the last painted hitmap row) opens the menu at the current cursor and
    // leaves the cursor unchanged.
    let cursor_before = home.cursor();
    let blank_y = home
        .test_hitmap()
        .iter()
        .map(|(r, _)| r.bottom())
        .max()
        .unwrap();
    let blank_message = home.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: 0,
        row: blank_y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(
        home.cursor(),
        cursor_before,
        "blank-space right-click leaves the cursor unchanged"
    );
    assert!(matches!(
        blank_message,
        Some(Msg::Shell(ShellRequest::HomeRowContextMenu { .. }))
    ));
}
