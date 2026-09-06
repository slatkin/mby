//! Characterization tests (task 1.1, standardize-Wide-hero-pane): pin the
//! *current* Wide browser-pane output of all seven Wide hero destinations
//! before any paint/primitive change lands. These intentionally capture
//! today's drifted behaviour (the Home clamp, ABS Podcasts' missing fill, ABS
//! Books' foreground-only `.style(Color)` bug, Feeds' conditional fill) as-is
//! -- they are a baseline to diff phases 2/3 against, not a statement of
//! correct behaviour. Must land in its own commit before any Wide hero paint
//! or primitive change (ledger migration flow).

use super::test_helpers::{buffer_to_string, make_audiobookshelf_book_app, make_music_group_app};
use crate::app::components::{
    AudiobookshelfBookComponent, AudiobookshelfPodcastComponent, FeedsComponent, HomeComponent,
    MusicWorkspaceComponent, TvWorkspaceComponent,
};
use crate::app::palette;
use crate::app::render::arrangements::library::wide_library_panes;
use crate::app::render::arrangements::wide_hero::{PANE_PAD_X, PANE_PAD_Y};
use crate::app::render::components::list_rows::LibraryListRenderCtx;
use crate::app::render::TvWideRenderCtx;
use crate::app::tests::make_item;
use crate::app::TWO_COLUMN_THRESHOLD;
use mbv_core::config::{FeedKind, FeedSubscription};
use mbv_core::playback_queue::{FeedEntry, QueueItem};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, KeyModifiers};

const WIDTH: u16 = 100;
const HEIGHT: u16 = 30;

fn wide_area() -> Rect {
    Rect::new(0, 0, WIDTH, HEIGHT)
}

fn direct_terminal(mut draw: impl FnMut(&mut ratatui::Frame)) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    terminal.draw(|f| draw(f)).unwrap();
    terminal
}

/// TV already routes through `wide_library_panes(area, PANE_PAD_X,
/// PANE_PAD_Y)` and `resolve_surface_focus` -- the one destination the
/// standardization leaves visually unchanged (task 3.2).
#[test]
fn tv_wide_left_pane_unconditional_fill_shared_inset() {
    let mut component = TvWorkspaceComponent::new();
    component.set_content(TvWideRenderCtx::new(
        LibraryListRenderCtx::from_items(vec![make_item("Focused Series", "Series")], 0, 0),
        None,
        None,
        0,
        None,
        false,
    ));
    let area = wide_area();
    let terminal = direct_terminal(|f| component.view(f, area));
    let buffer = terminal.backend().buffer();

    let panes = wide_library_panes(area, PANE_PAD_X, PANE_PAD_Y).expect("wide fits");
    let left_panel = panes.left_panel;

    assert_eq!(
        buffer[(left_panel.x, left_panel.y)].bg,
        palette::resolve_surface_focus(false)
    );
    assert_eq!(
        buffer[(left_panel.x, left_panel.bottom() - 1)].bg,
        palette::resolve_surface_focus(false)
    );
}

/// Music routes through `wide_library_panes(area, 0, PANE_PAD_Y)` --
/// no horizontal inset on the panel/pane split itself (task 3.3).
#[test]
fn music_wide_left_pane_unconditional_fill_no_horizontal_pad() {
    let app = make_music_group_app();
    let lib_idx = app.tab.emby_library_index().unwrap();
    let context = app.wide_music_render_ctx(lib_idx, None);
    let mut component = MusicWorkspaceComponent::new();
    component.set_content(context);
    let area = wide_area();
    let terminal = direct_terminal(|f| component.view(f, area));
    let buffer = terminal.backend().buffer();

    let panes = wide_library_panes(area, 0, PANE_PAD_Y).expect("wide fits");
    let left_panel = panes.left_panel;

    assert_eq!(
        buffer[(left_panel.x, left_panel.y)].bg,
        palette::resolve_surface_focus(false)
    );
    assert_eq!(
        buffer[(left_panel.x, left_panel.bottom() - 1)].bg,
        palette::resolve_surface_focus(false)
    );
}

/// Home's non-Emby Latest selection fills the complete hero pane. The
/// Audiobookshelf cover and metadata are top-anchored within that pane.
#[test]
fn home_wide_non_emby_latest_fills_the_full_hero_area() {
    let source = crate::app::types_playback::HomeLatestSource::Audiobookshelf("books".into());
    let latest = vec![(
        "Books".into(),
        source.clone(),
        vec![QueueItem::AudiobookshelfBook(
            mbv_core::playback_queue::AudiobookshelfBookQueueItem {
                library_item_id: "book-1".into(),
                title: "Home Book".into(),
                author: Some("Author".into()),
                duration_ticks: None,
                position_ticks: 0,
                played: false,
                is_finished: false,
                cover_path: None,
            },
        )],
    )];
    let mut component = HomeComponent::new();
    component.set_content(Vec::new(), latest, false);
    component.set_focused(true);
    assert!(component.restore_section(&source), "Books pill must exist");
    let area = wide_area();
    let terminal = direct_terminal(|f| component.view(f, area));

    let hero = component.hero_area().expect("wide non-Emby hero pane");
    let buffer = terminal.backend().buffer();

    assert_eq!(buffer[(hero.x, hero.y)].bg, palette::SURFACE_RESTING);
    assert_eq!(
        buffer[(hero.x, hero.bottom() - 1)].bg,
        palette::SURFACE_RESTING,
        "the full reported hero area must be filled"
    );
}

fn feed_component_with_entries(entries: Vec<FeedEntry>) -> FeedsComponent {
    let subscriptions = vec![FeedSubscription {
        name: "Test Feed".into(),
        url: "https://example.test/feed".into(),
        kind: FeedKind::Audio,
    }];
    let grouped = vec![entries];
    let all_entries = grouped[0].clone();
    let mut component = FeedsComponent::new();
    component.set_content(&subscriptions, &grouped, &all_entries, false);
    component.set_focused(true);
    component
}

fn feed_entry(guid: &str, title: &str) -> FeedEntry {
    FeedEntry {
        guid: guid.into(),
        title: title.into(),
        enclosure_url: None,
        link: None,
        mime_type: None,
        duration_ticks: None,
        pub_date_secs: None,
        feed_kind: Some(FeedKind::Audio),
        feed_id: None,
        position_ticks: 0,
        played: false,
    }
}

/// Feeds with a selected entry: the fill and detail both paint (task 2.3's
/// starting point).
#[test]
fn feeds_wide_left_pane_fills_when_an_entry_is_selected() {
    let mut component = feed_component_with_entries(vec![feed_entry("entry-1", "Entry One")]);
    let area = wide_area();
    let terminal = direct_terminal(|f| component.view(f, area));
    let hero = component.layout().hero_area;
    assert!(hero.width > 0 && hero.height > 0);
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(hero.x, hero.y)].bg, palette::SURFACE_RESTING);
    assert_eq!(
        buffer[(hero.x, hero.bottom() - 1)].bg,
        palette::SURFACE_RESTING
    );
}

/// Feeds (task 2.3): the wide left pane fill is unconditional (D1) -- with
/// entries present but nothing selected, the pane still fills
/// `SURFACE_RESTING` (D3: read-only, never focus-green), with no hero
/// content painted. `render_feeds_content` is called directly with
/// `selected_entry: None` since the component's own cursor always resolves
/// to an entry once entries exist.
#[test]
fn feeds_wide_left_pane_fills_unconditionally_with_no_selection() {
    use crate::app::components::media_list::{InlineMediaBrowser, WideMediaList};
    use crate::app::layout::LayoutMain;
    use crate::app::render::{render_feeds_content, FeedsRenderModel};
    use crate::app::types_feed_tab::WatchedFilter;

    let subscriptions = vec![FeedSubscription {
        name: "Test Feed".into(),
        url: "https://example.test/feed".into(),
        kind: FeedKind::Audio,
    }];
    let entries = vec![feed_entry("entry-1", "Entry One")];
    let mut layout = LayoutMain::default();
    let mut canonical_list: WideMediaList<String> = WideMediaList::new();
    let inline_list: InlineMediaBrowser<String> = InlineMediaBrowser::new();
    let area = wide_area();
    let terminal = direct_terminal(|f| {
        render_feeds_content(
            f,
            area,
            false,
            &mut layout,
            FeedsRenderModel {
                subscriptions: &subscriptions,
                visible_entries: &entries,
                watched_filter: WatchedFilter::All,
                selected_group: 0,
                loading: false,
                selected_entry: None,
                images_enabled: true,
            },
            &mut canonical_list,
            &inline_list,
        );
    });
    let hero = layout.hero_area;
    assert!(hero.width > 0 && hero.height > 0, "hero={hero:?}");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(hero.x, hero.y)].bg, palette::SURFACE_RESTING);
    assert_eq!(
        buffer[(hero.x, hero.bottom() - 1)].bg,
        palette::SURFACE_RESTING
    );

    let mut focused_layout = LayoutMain::default();
    let mut focused_list: WideMediaList<String> = WideMediaList::new();
    let focused_terminal = direct_terminal(|f| {
        render_feeds_content(
            f,
            area,
            true,
            &mut focused_layout,
            FeedsRenderModel {
                subscriptions: &subscriptions,
                visible_entries: &entries,
                watched_filter: WatchedFilter::All,
                selected_group: 0,
                loading: false,
                selected_entry: None,
                images_enabled: true,
            },
            &mut focused_list,
            &inline_list,
        );
    });
    let focused_hero = focused_layout.hero_area;
    let focused_buffer = focused_terminal.backend().buffer();
    assert_eq!(
        focused_buffer[(focused_hero.x, focused_hero.y)].bg,
        palette::SURFACE_RESTING
    );
    assert_eq!(
        focused_buffer[(focused_hero.x, focused_hero.bottom() - 1)].bg,
        palette::SURFACE_RESTING
    );
}

/// Feeds with no entries to select: `feeds.rs:170-184` returns before the
/// Wide hero pane is ever reached (a placeholder message paints instead),
/// so no `hero_area` is published at all -- the broken empty-selection state
/// task 2.3 fixes (D1: an unconditional pane fill even with nothing
/// selected).
#[test]
fn feeds_wide_left_pane_unfilled_with_no_selected_entry() {
    let mut component = feed_component_with_entries(vec![]);
    let area = wide_area();
    let terminal = direct_terminal(|f| component.view(f, area));
    let hero = component.layout().hero_area;
    assert_eq!(
        hero,
        Rect::default(),
        "characterizes the pre-fix state: no hero pane is published with no entries"
    );
    let output = buffer_to_string(&terminal);
    assert!(
        output.contains("Press r to load feeds"),
        "output={output:?}"
    );
}

/// ABS Books (task 2.2): the `.style(Color)` foreground-only bug is fixed --
/// the wide left pane is filled via `wide_hero_hero_pane`, focus-green
/// (`LeftPaneFocus::Workspace`) only when a chapter is selected while
/// focused.
#[test]
fn abs_books_wide_left_pane_fills_via_shared_primitive() {
    let app = make_audiobookshelf_book_app();
    let mut component = AudiobookshelfBookComponent::new();
    if let Some(state) = app.audiobookshelf_book_browse.first() {
        component.set_content(state, app.images_enabled());
        component.set_focused(true);
    }
    let area = wide_area();
    let terminal = direct_terminal(|f| component.view(f, area));
    let geometry = component.geometry();
    assert!(geometry.wide);
    let panes = wide_library_panes(area, 0, PANE_PAD_Y).expect("wide fits");
    let left_panel = panes.left_panel;
    let buffer = terminal.backend().buffer();
    // No chapter is selected in this fixture, so the workspace is not held:
    // the pane stays resting, not focus-green.
    assert_eq!(
        buffer[(left_panel.x, left_panel.y)].bg,
        palette::SURFACE_RESTING
    );
    assert_eq!(
        buffer[(left_panel.x, left_panel.bottom() - 1)].bg,
        palette::SURFACE_RESTING
    );

    component.on(&Event::Keyboard(KeyEvent {
        code: Key::Left,
        modifiers: KeyModifiers::NONE,
    }));
    let focused_terminal = direct_terminal(|f| component.view(f, area));
    let focused_buffer = focused_terminal.backend().buffer();
    assert_eq!(
        focused_buffer[(left_panel.x, left_panel.y)].bg,
        palette::SURFACE_FOCUSED
    );
}

/// ABS Podcasts (task 2.1): the wide left pane fills via `wide_hero_hero_pane`.
/// D8's gain: this surface goes focus-green when the episode workspace holds
/// focus (mirroring TV), not a bare `focused`.
#[test]
fn abs_podcasts_wide_left_pane_fills_via_shared_primitive() {
    let app = crate::app::tests_podcast::audiobookshelf_app();
    let mut component = AudiobookshelfPodcastComponent::new();
    if let Some(state) = app.audiobookshelf_browse.first() {
        component.set_content(state, app.images_enabled());
        component.set_focused(true);
    }
    let area = wide_area();
    let terminal = direct_terminal(|f| component.view(f, area));
    let geometry = component.geometry();
    let hero = geometry.hero_area;
    assert!(hero.width > 0 && hero.height > 0, "hero={hero:?}");
    let buffer = terminal.backend().buffer();
    // No episode is selected in this fixture: the show list holds focus, so
    // the pane stays resting even though the surface is focused overall
    // (D8/D3: never a bare `focused`).
    assert_eq!(buffer[(hero.x, hero.y)].bg, palette::SURFACE_RESTING);
    assert_ne!(
        buffer[(hero.x, hero.y)].bg,
        palette::resolve_surface_focus(true)
    );

    component.set_episode_selection(Some(0));
    let focused_terminal = direct_terminal(|f| component.view(f, area));
    let focused_buffer = focused_terminal.backend().buffer();
    assert_eq!(
        focused_buffer[(hero.x, hero.y)].bg,
        palette::SURFACE_FOCUSED
    );
}

#[test]
fn abs_book_wide_hero_keeps_text_with_images_on_or_off() {
    let app = make_audiobookshelf_book_app();
    for images_enabled in [true, false] {
        let mut component = AudiobookshelfBookComponent::new();
        component.set_content(
            app.audiobookshelf_book_browse.first().expect("book state"),
            images_enabled,
        );
        component.set_focused(true);
        let terminal = direct_terminal(|f| component.view(f, wide_area()));
        assert!(buffer_to_string(&terminal).contains("Alpha Tales"));
    }
}

#[test]
fn abs_podcast_wide_hero_keeps_text_with_images_on_or_off() {
    let app = crate::app::tests_podcast::audiobookshelf_app();
    for images_enabled in [true, false] {
        let mut component = AudiobookshelfPodcastComponent::new();
        component.set_content(
            app.audiobookshelf_browse.first().expect("podcast state"),
            images_enabled,
        );
        component.set_focused(true);
        let terminal = direct_terminal(|f| component.view(f, wide_area()));
        assert!(buffer_to_string(&terminal).contains("Show A"));
    }
}

/// Sanity: the fixture width used throughout this module clears the shared
/// two-column breakpoint, so every characterization above exercises the Wide
/// Wide hero presentation rather than falling back to narrow.
#[test]
fn fixture_width_is_wide() {
    const { assert!(WIDTH >= TWO_COLUMN_THRESHOLD) };
    let _ = buffer_to_string; // keep the shared helper import exercised
}
