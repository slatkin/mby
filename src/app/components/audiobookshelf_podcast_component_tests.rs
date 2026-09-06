use super::audiobookshelf_podcast::AudiobookshelfPodcastComponent;
use super::msg::{Msg, PodcastEpisodeIntent, PodcastEpisodeTransition, ShellRequest};
use crate::app::images::audiobookshelf_cover_cache_key;
use crate::app::shell::Model;
use crate::app::tests_podcast::audiobookshelf_app;
use crate::app::types_audiobookshelf_browse::{
    AudiobookshelfBrowseState, AudiobookshelfEpisodeFilter,
};
use mbv_core::audiobookshelf::{AudiobookshelfLibrary, AudiobookshelfShow};
use mbv_core::config::{AudiobookshelfSetup, ServiceKind};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{
    Event, Key, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

/// split-audiobookshelf-cursor-ownership D4 / task 1.3 → 5.1: when a content
/// push drops the show the component had selected, the component resets its
/// own `episode_selection` / `episode_filter` / `scroll` to their defaults —
/// it never adopts the values carried in the shell's snapshot for those
/// fields.
#[test]
fn abs_podcast_component_drops_stale_episode_state_when_selection_vanishes() {
    let library = AudiobookshelfLibrary {
        id: "abs-podcasts".into(),
        name: "ABS Podcasts".into(),
        media_type: "podcast".into(),
    };
    let show = |id: &str, title: &str| AudiobookshelfShow {
        library_item_id: id.into(),
        title: title.into(),
        author: None,
        description: None,
        cover_path: None,
    };

    let mut first = AudiobookshelfBrowseState::new(library.clone());
    first.append_page(
        0,
        20,
        2,
        vec![show("show-a", "Show A"), show("show-b", "Show B")],
    );
    first.select(0);

    let mut component = AudiobookshelfPodcastComponent::new();
    component.set_content(&first, false);
    component.set_focused(true);
    component.set_episode_selection(Some(1));
    component.set_episode_filter(AudiobookshelfEpisodeFilter::Unplayed);

    // New content: show-a is gone. The projected content type no longer
    // carries episode filter / selection / scroll, so the component's own
    // interaction state is all there is -- and it must reset.
    let mut second = AudiobookshelfBrowseState::new(library);
    second.append_page(0, 20, 1, vec![show("show-b", "Show B")]);

    component.set_content(&second, false);
    component.set_focused(true);

    assert_eq!(
        component.episode_selection(),
        None,
        "stale episode selection must reset, not adopt the snapshot's Some(5)"
    );
    assert_eq!(
        component.episode_filter(),
        AudiobookshelfEpisodeFilter::All,
        "stale episode filter must reset to All, not adopt the snapshot's Played"
    );
}

#[test]
fn abs_podcast_component_keeps_local_show_cursor_and_renders_without_app_state() {
    let app = crate::app::tests_podcast::audiobookshelf_app();
    let state = &app.audiobookshelf_browse[0];
    let mut component = AudiobookshelfPodcastComponent::new();
    component.set_content(state, false);
    component.set_focused(true);

    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Down,
        modifiers: KeyModifiers::NONE,
    }));
    let Some(Msg::Shell(ShellRequest::AudiobookshelfPodcastShowMove { index })) = message else {
        panic!("show movement should carry the resolved show index");
    };
    assert_eq!(index, 0, "single show clamps the resolved cursor to 0");
    assert_eq!(component.cursor(), 0);

    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    terminal
        .draw(|frame| component.view(frame, frame.area()))
        .unwrap();
    let output: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol().to_owned())
        .collect();
    assert!(output.contains("Show A"), "output: {output:?}");
}

#[test]
fn abs_podcast_component_emits_typed_episode_transitions_in_episode_mode() {
    let app = crate::app::tests_podcast::audiobookshelf_app();
    let state = &app.audiobookshelf_browse[0];
    let mut component = AudiobookshelfPodcastComponent::new();
    component.set_content(state, false);
    component.set_focused(true);
    component.set_episode_selection(Some(0));

    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Down,
        modifiers: KeyModifiers::NONE,
    }));
    let Some(Msg::Shell(ShellRequest::AudiobookshelfPodcastEpisodeTransition(transition))) =
        message
    else {
        panic!("episode movement should be a typed episode-transition request, got {message:?}");
    };
    assert_eq!(transition, PodcastEpisodeTransition::NextEpisode);

    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char(']'),
        modifiers: KeyModifiers::NONE,
    }));
    let Some(Msg::Shell(ShellRequest::AudiobookshelfPodcastEpisodeTransition(transition))) =
        message
    else {
        panic!("filter cycling should be a typed episode-transition request, got {message:?}");
    };
    assert_eq!(transition, PodcastEpisodeTransition::NextFilter);

    let message = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Esc,
        modifiers: KeyModifiers::NONE,
    }));
    let Some(Msg::Shell(ShellRequest::AudiobookshelfPodcastEpisodeTransition(transition))) =
        message
    else {
        panic!("episode exit should be a typed episode-transition request, got {message:?}");
    };
    assert_eq!(transition, PodcastEpisodeTransition::Exit);
}

#[test]
fn abs_podcast_component_cycles_show_title_buckets_with_brackets() {
    let library = AudiobookshelfLibrary {
        id: "abs-podcasts".into(),
        name: "ABS Podcasts".into(),
        media_type: "podcast".into(),
    };
    let mut state = AudiobookshelfBrowseState::new(library);
    state.append_page(
        0,
        20,
        2,
        vec![
            AudiobookshelfShow {
                library_item_id: "alpha".into(),
                title: "Alpha".into(),
                author: None,
                description: None,
                cover_path: None,
            },
            AudiobookshelfShow {
                library_item_id: "zulu".into(),
                title: "Zulu".into(),
                author: None,
                description: None,
                cover_path: None,
            },
        ],
    );

    let mut component = AudiobookshelfPodcastComponent::new();
    component.set_content(&state, false);
    component.set_focused(true);

    for (key, index) in [(Key::Char('['), 1), (Key::Char(']'), 0)] {
        assert_eq!(
            component.on(&Event::Keyboard(KeyEvent {
                code: key,
                modifiers: KeyModifiers::NONE,
            })),
            Some(Msg::Shell(ShellRequest::AudiobookshelfPodcastShowMove {
                index
            }))
        );
    }
}

#[test]
fn abs_podcast_component_emits_typed_action_intents_without_raw_key_replay() {
    let state = &crate::app::tests_podcast::audiobookshelf_app().audiobookshelf_browse[0];
    let mut component = AudiobookshelfPodcastComponent::new();
    component.set_content(state, false);
    component.set_focused(true);

    // One representative action key per intent: the component reports only the
    // matched intent (task 5.3d.7); the shell resolves conditions at the Model
    // boundary.
    let space = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char(' '),
        modifiers: KeyModifiers::NONE,
    }));
    assert!(matches!(
        space,
        Some(Msg::Shell(
            ShellRequest::AudiobookshelfPodcastEpisodeIntent(PodcastEpisodeIntent::FocusOrPlay)
        ))
    ));

    let enter = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(matches!(
        enter,
        Some(Msg::Shell(
            ShellRequest::AudiobookshelfPodcastEpisodeIntent(PodcastEpisodeIntent::OpenOrPlay)
        ))
    ));

    let ctrl_a = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('a'),
        modifiers: KeyModifiers::CONTROL,
    }));
    assert!(matches!(
        ctrl_a,
        Some(Msg::Shell(
            ShellRequest::AudiobookshelfPodcastEpisodeIntent(PodcastEpisodeIntent::Enqueue)
        ))
    ));

    let unrelated = component.on(&Event::Keyboard(KeyEvent {
        code: Key::Char('z'),
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(unrelated, None);
}

fn narrow_grid_component_state() -> AudiobookshelfBrowseState {
    let library = AudiobookshelfLibrary {
        id: "lib".into(),
        name: "Podcasts".into(),
        media_type: "podcast".into(),
    };
    let mut state = AudiobookshelfBrowseState::new(library);
    state.append_page(
        0,
        20,
        12,
        (0..12)
            .map(|i| AudiobookshelfShow {
                library_item_id: format!("show-{i}"),
                title: format!("Show {i}"),
                author: None,
                description: None,
                cover_path: None,
            })
            .collect(),
    );
    state.select(2);
    state
}

fn view_narrow(component: &mut AudiobookshelfPodcastComponent, width: u16, height: u16) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| component.view(frame, frame.area()))
        .unwrap();
}

#[test]
fn abs_podcast_narrow_one_column_navigation_uses_page_rows() {
    let state = narrow_grid_component_state();
    let mut component = AudiobookshelfPodcastComponent::new();
    component.set_content(&state, false);
    component.set_focused(true);
    view_narrow(&mut component, 100, 6);
    assert_eq!(component.geometry().columns, 1);
    assert!(matches!(
        component.on(&Event::Keyboard(KeyEvent {
            code: Key::Down,
            modifiers: KeyModifiers::NONE
        })),
        Some(Msg::Shell(ShellRequest::AudiobookshelfPodcastShowMove {
            index: 3
        }))
    ));
    assert!(matches!(
        component.on(&Event::Keyboard(KeyEvent {
            code: Key::Right,
            modifiers: KeyModifiers::NONE
        })),
        Some(Msg::Shell(ShellRequest::AudiobookshelfPodcastShowMove {
            index: 4
        }))
    ));
    let mut page_component = AudiobookshelfPodcastComponent::new();
    page_component.set_content(&state, false);
    page_component.set_focused(true);
    view_narrow(&mut page_component, 100, 6);
    let page_rows = page_component
        .geometry()
        .list_area
        .height
        .saturating_sub(1)
        .max(1) as usize;
    page_component.on(&Event::Keyboard(KeyEvent {
        code: Key::PageDown,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(page_component.cursor(), 2 + page_rows);
}

#[test]
fn abs_podcast_wheel_moves_three_rows_and_ignores_outside_list() {
    let state = narrow_grid_component_state();
    let mut component = AudiobookshelfPodcastComponent::new();
    component.set_content(&state, false);
    component.set_focused(true);
    view_narrow(&mut component, 100, 6);
    let list = component.geometry().list_area;
    let inside = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: list.x,
        row: list.y,
        modifiers: KeyModifiers::NONE,
    };
    assert!(matches!(
        component.on(&Event::Mouse(inside)),
        Some(Msg::Shell(ShellRequest::AudiobookshelfPodcastShowMove {
            index: 5
        }))
    ));
    // The wheel throttle lives in the private gesture state (ADR 0024, D3);
    // reset it so the synchronous test loop's second wheel step is recognized.
    component.reset_mouse_gestures_for_test();
    assert!(matches!(
        component.on(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: list.x,
            row: list.y,
            modifiers: KeyModifiers::NONE,
        })),
        Some(Msg::Shell(ShellRequest::AudiobookshelfPodcastShowMove {
            index: 2
        }))
    ));
    assert_eq!(component.cursor(), 2);
    assert_eq!(
        component.on(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })),
        None
    );
    assert_eq!(component.cursor(), 2);
}

#[test]
fn abs_podcast_row_mouse_selects_the_clicked_show_and_bucket_start() {
    let state = narrow_grid_component_state();
    let mut component = AudiobookshelfPodcastComponent::new();
    component.set_content(&state, false);
    component.set_focused(true);
    view_narrow(&mut component, 100, 6);
    let rects = component.geometry().show_rows.clone();
    let (rect, clicked) = rects
        .iter()
        .copied()
        .find(|(_, i)| *i != component.cursor())
        .expect("a non-selected show row is painted");
    let msg = component.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x + rect.width / 2,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(
        msg,
        Some(Msg::Shell(ShellRequest::AudiobookshelfPodcastShowMove {
            index: clicked
        }))
    );
    let bucket = component.geometry().selector_tabs[0].0;
    let msg = component.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: bucket.x,
        row: bucket.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(matches!(
        msg,
        Some(Msg::Shell(ShellRequest::AudiobookshelfPodcastShowMove {
            index: 0
        }))
    ));
}

#[test]
fn abs_podcast_component_returns_none_when_unfocused() {
    let state = &crate::app::tests_podcast::audiobookshelf_app().audiobookshelf_browse[0];
    let mut component = AudiobookshelfPodcastComponent::new();
    component.set_content(state, false);
    component.set_focused(false);
    let cursor = component.cursor();

    for code in [Key::Down, Key::Enter, Key::Char('z')] {
        assert_eq!(
            component.on(&Event::Keyboard(KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
            })),
            None
        );
        assert_eq!(component.cursor(), cursor);
    }
}

#[test]
fn abs_podcast_cover_fetch_bridged_to_content_push_and_gated_by_images() {
    // Image-disabled: the fresh-mount content push must not schedule any cover
    // fetch.
    let mut model = Model::new(audiobookshelf_app());
    model.sync_audiobookshelf_podcast();
    assert!(
        model.app.card_image_loading.is_empty(),
        "image-disabled content push must not schedule a cover fetch"
    );

    // Image-enabled with a configured server and secret: the selected show's
    // cover is scheduled through the bridge by the fresh-mount content push.
    let mut app = audiobookshelf_app();
    app.image_protocol_enabled = true;
    app.config.lock().unwrap().audiobookshelf_setup =
        Some(AudiobookshelfSetup::new("https://abs.example"));
    mbv_core::config::save_service_secret(ServiceKind::Audiobookshelf, "test-secret").unwrap();
    let mut model = Model::new(app);
    model.sync_audiobookshelf_podcast();

    let server = model
        .app
        .config
        .lock()
        .unwrap()
        .audiobookshelf_setup
        .as_ref()
        .unwrap()
        .server_url
        .clone();
    let expected_key =
        audiobookshelf_cover_cache_key(&server, "show-a", model.app.current_protocol_suffix());
    assert!(
        model.app.card_image_loading.contains(&expected_key),
        "image-enabled content push should schedule the selected show's cover fetch"
    );
}

/// Task 5.3d.10c: the component owns its painted geometry (list/right/hero/
/// inline-hero/selected-item rects), so the shell can read it after render
/// ownership moved off `App`. The same mounted component is rendered wide then
/// narrow; the wide right panel must be coherent, and a narrow re-render must
/// not leak the wide `right_area`. A no-show narrow render resets every hero
/// field.
#[test]
fn abs_podcast_component_geometry_is_wide_coherent_and_narrow_resets_wide() {
    let mut state = AudiobookshelfBrowseState::new(AudiobookshelfLibrary {
        id: "abs-podcasts".into(),
        name: "ABS Podcasts".into(),
        media_type: "podcast".into(),
    });
    state.append_page(
        0,
        10,
        10,
        vec![AudiobookshelfShow {
            library_item_id: "show-a".into(),
            title: "Show A".into(),
            author: Some("Author".into()),
            description: Some("An audacious podcast about everything worth hearing.".into()),
            cover_path: None,
        }],
    );
    state.select(0);

    let mut component = AudiobookshelfPodcastComponent::new();
    component.set_content(&state, false);
    component.set_focused(true);

    let wide = Rect::new(0, 0, 100, 40);
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    terminal.draw(|frame| component.view(frame, wide)).unwrap();
    let geometry = component.geometry();
    assert!(
        geometry.hero_area.width > 0 && geometry.hero_area.height > 0,
        "wide hero must be painted"
    );
    assert!(
        geometry.right_area.width > 0 && geometry.right_area.height > 0,
        "wide right panel must be painted"
    );
    assert_eq!(
        geometry.list_area, geometry.right_area,
        "wide list == right panel"
    );
    assert_eq!(
        geometry.right_area.x, wide.x,
        "wide browser/list is the left pane"
    );
    assert!(
        geometry.right_area.right() <= geometry.hero_area.x,
        "wide hero sits right of the browser/list pane"
    );
    assert!(geometry.hero_area.bottom() <= wide.bottom());
    assert!(geometry.right_area.right() <= wide.right());
    assert_eq!(
        geometry.inline_hero_area,
        Rect::default(),
        "wide layout has no inline hero"
    );
    assert!(
        geometry.selected_item_rect.is_none(),
        "wide layout has no selected-item shell"
    );

    // Re-render the same mounted component narrow: the wide `right_area` must
    // not survive, and the admitted inline hero must agree across fields.
    let narrow = Rect::new(0, 0, 60, 40);
    terminal
        .draw(|frame| component.view(frame, narrow))
        .unwrap();
    let geometry = component.geometry();
    assert_eq!(
        geometry.right_area,
        Rect::default(),
        "narrow render must reset the wide right_area"
    );
    assert!(
        geometry.list_area.width > 0 && geometry.list_area.height > 0,
        "narrow list area must be nonzero"
    );
    assert!(
        geometry.list_area.y >= narrow.y && geometry.list_area.bottom() <= narrow.bottom(),
        "narrow list sits within the area"
    );
    assert!(
        geometry.hero_area.width > 0 && geometry.hero_area.height > 0,
        "narrow inline hero must be admitted for a short selected show"
    );
    assert_eq!(
        geometry.inline_hero_area, geometry.hero_area,
        "narrow inline hero must equal the painted hero"
    );
    assert_eq!(
        geometry.selected_item_rect,
        Some(geometry.hero_area),
        "narrow selected-item rect must equal the painted hero"
    );
    assert!(geometry.hero_area.right() <= narrow.right());
    assert!(geometry.hero_area.bottom() <= narrow.bottom());

    // No-show narrow render: every hero/right/selected field resets.
    let empty = AudiobookshelfBrowseState::new(AudiobookshelfLibrary {
        id: "abs-podcasts".into(),
        name: "ABS Podcasts".into(),
        media_type: "podcast".into(),
    });
    let mut empty_component = AudiobookshelfPodcastComponent::new();
    empty_component.set_content(&empty, false);
    empty_component.set_focused(true);
    let mut empty_terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();

    empty_terminal
        .draw(|frame| empty_component.view(frame, wide))
        .unwrap();
    let empty_wide_geometry = empty_component.geometry();
    assert!(
        empty_wide_geometry.right_area.width > 0,
        "no-show wide layout still paints its right placeholder panel"
    );
    assert_eq!(
        empty_wide_geometry.list_area,
        empty_wide_geometry.right_area
    );
    assert_eq!(
        empty_wide_geometry.hero_area,
        Rect::default(),
        "no-show wide layout must not report an unpainted hero"
    );
    assert!(empty_wide_geometry.selected_item_rect.is_none());

    empty_terminal
        .draw(|frame| empty_component.view(frame, narrow))
        .unwrap();
    let empty_narrow_geometry = empty_component.geometry();
    assert_eq!(
        empty_narrow_geometry.list_area, narrow,
        "no-show narrow list_area is the whole area"
    );
    assert_eq!(empty_narrow_geometry.right_area, Rect::default());
    assert_eq!(empty_narrow_geometry.hero_area, Rect::default());
    assert_eq!(empty_narrow_geometry.inline_hero_area, Rect::default());
    assert!(empty_narrow_geometry.selected_item_rect.is_none());
}

/// Task 4.1/4.5: a double-click on a painted show row selects it and emits
/// the existing OpenOrPlay episode intent; a right-click is ignored
/// (task 4.6: no keyboard context-menu equivalent).
#[test]
fn abs_podcast_mouse_double_click_emits_open_or_play_and_right_click_ignored() {
    let state = narrow_grid_component_state();
    let mut component = AudiobookshelfPodcastComponent::new();
    component.set_content(&state, false);
    component.set_focused(true);
    view_narrow(&mut component, 100, 6);
    let (rect, clicked) = component
        .geometry()
        .show_rows
        .iter()
        .copied()
        .find(|(_, i)| *i != component.cursor())
        .expect("a non-selected show row is painted");
    // Two quick Downs at the same point = DoubleClick on the second.
    component.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }));
    let msg = component.on(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(
        msg,
        Some(Msg::Shell(
            ShellRequest::AudiobookshelfPodcastEpisodeIntent(PodcastEpisodeIntent::OpenOrPlay)
        ))
    );
    assert_eq!(component.cursor(), clicked);
    assert_eq!(
        component.on(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        })),
        None,
        "task 4.6: right-click must be ignored on this surface"
    );
}
