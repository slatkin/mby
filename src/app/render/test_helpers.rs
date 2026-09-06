#![allow(dead_code, unused_imports)]

use super::*;
use crate::app::components::{BrowserComponent, MusicWorkspaceComponent, TvWorkspaceComponent};
use crate::app::layout::{AppLayout, LayoutPlayback};
use crate::app::render::components::widgets::render_right_scrollbar_with_viewport;
use crate::app::shell::Model;
use crate::app::tests::{make_app_stub, make_item};
use crate::app::types_audiobookshelf_browse::{
    build_surname_buckets, AudiobookshelfBookBrowseState,
};
use crate::app::{App, PanelFocus};
use crate::app::{BrowseLevel, LibraryTab, QueueScope, RemoteSlotState, TabSelection};
use crate::config::Config;
use mbv_core::api::EmbyClient;
use mbv_core::api::EmbyItem;
use mbv_core::audiobookshelf::{AudiobookshelfBook, AudiobookshelfChapter, AudiobookshelfLibrary};
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;

#[path = "test_helpers_mounted.rs"]
mod mounted;
pub use mounted::*;
#[path = "test_helpers_fixtures.rs"]
mod fixtures;
pub use fixtures::*;

pub fn buffer_to_string(term: &Terminal<TestBackend>) -> String {
    let buf = term.backend().buffer();
    let area = *buf.area();
    let mut out = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

pub fn render_sidebar_scrollbar_column(total: usize, visible: u16, scroll: usize) -> String {
    let backend = TestBackend::new(1, visible);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        super::components::chrome::render_sidebar_scrollbar(
            f,
            Rect::new(0, 0, 0, visible),
            total,
            scroll,
        );
    })
    .unwrap();
    buffer_to_string(&term)
}

pub fn render_scrollbar_column(height: u16, max_offset: usize, offset: usize) -> String {
    let backend = TestBackend::new(1, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_right_scrollbar(
            f,
            Rect::new(0, 0, 1, height),
            max_offset,
            offset,
            palette::TEXT_METADATA,
        );
    })
    .unwrap();
    buffer_to_string(&term)
}

pub fn render_scrollbar_column_with_viewport(
    height: u16,
    content_length: usize,
    viewport_content_length: usize,
    offset: usize,
) -> String {
    let backend = TestBackend::new(1, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_right_scrollbar_with_viewport(
            f,
            Rect::new(0, 0, 1, height),
            content_length,
            viewport_content_length,
            offset,
            palette::TEXT_METADATA,
        );
    })
    .unwrap();
    buffer_to_string(&term)
}

pub fn render_pill_bar_hitboxes(
    labels: &[String],
    ids: &[usize],
    selected_pos: usize,
    width: u16,
) -> Vec<(Rect, usize)> {
    let backend = TestBackend::new(width, 1);
    let mut term = Terminal::new(backend).unwrap();
    let mut tabs = Vec::new();
    term.draw(|f| {
        tabs = render_pill_bar(
            f,
            Rect::new(0, 0, width, 1),
            PillBar {
                labels,
                ids,
                selected_pos,
                prefix: None,
            },
        );
    })
    .unwrap();
    tabs
}

pub fn assert_surface_pills(
    terminal: &Terminal<TestBackend>,
    layout: &LayoutMain,
    panel: Rect,
    expected_pill_rows: usize,
    spacer_bg: Color,
    expected_ids: &[usize],
    expected_labels: &[&str],
    selected_id: usize,
) {
    assert_eq!(
        layout
            .selector_tabs
            .iter()
            .map(|(_, id)| *id)
            .collect::<Vec<_>>(),
        expected_ids,
        "surface pill targets"
    );
    let first = layout
        .selector_tabs
        .first()
        .expect("surface should publish pill targets")
        .0;
    assert!(
        layout
            .selector_tabs
            .iter()
            .all(|(rect, _)| rect.y == first.y && rect.height == 1),
        "pill hitboxes must occupy one shared row: {:?}",
        layout.selector_tabs
    );
    let buffer = terminal.backend().buffer();
    let painted_rows = (panel.y..panel.bottom())
        .filter(|y| (panel.x..panel.right()).any(|x| matches!(buffer[(x, *y)].symbol(), "◢" | "◤")))
        .collect::<Vec<_>>();
    assert_eq!(
        painted_rows.len(),
        expected_pill_rows,
        "painted pill rows in designated panel: panel={panel:?} targets={:?}",
        layout.selector_tabs
    );
    assert!(
        painted_rows.contains(&first.y),
        "target row is not a painted pill row: targets={:?} rows={painted_rows:?}",
        layout.selector_tabs
    );
    let row_text = (0..buffer.area().width)
        .map(|x| buffer[(x, first.y)].symbol())
        .collect::<String>();
    for label in expected_labels {
        assert!(
            row_text.contains(label),
            "pill row missing {label:?}: {row_text:?}"
        );
    }
    assert_eq!(
        buffer[(first.x, first.y)].style().bg,
        Some(palette::PILL_ROW_BG),
        "pill row background"
    );
    for pill_y in &painted_rows {
        assert!(
            *pill_y + 1 < panel.bottom(),
            "reserved spacer must fit in panel"
        );
        for x in panel.x..panel.right() {
            assert_eq!(
                buffer[(x, *pill_y + 1)].style().bg,
                Some(spacer_bg),
                "reserved spacer background at x={x}, y={}",
                *pill_y + 1
            );
        }
    }
    let painted_spans = (first.x..panel.right())
        .filter(|x| buffer[(*x, first.y)].symbol() == "◢")
        .filter_map(|start| {
            (start + 1..panel.right())
                .find(|x| buffer[(*x, first.y)].symbol() == "◤")
                .map(|end| Rect::new(start, first.y, end - start + 1, 1))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        painted_spans,
        layout
            .selector_tabs
            .iter()
            .map(|(rect, _)| *rect)
            .collect::<Vec<_>>(),
        "pill hitboxes must match painted horizontal spans"
    );
    for rect in layout.selector_tabs.iter().map(|(rect, _)| *rect) {
        assert!(
            panel.contains((rect.x, rect.y).into())
                && panel.contains((rect.right() - 1, rect.bottom() - 1).into()),
            "pill target outside designated panel: {rect:?} panel={panel:?}"
        );
    }
    let selected = layout
        .selector_tabs
        .iter()
        .find(|(_, id)| *id == selected_id)
        .expect("selected pill id should have a hitbox")
        .0;
    assert_eq!(
        buffer[(selected.x + 1, selected.y)].style().bg,
        Some(palette::PILL_SELECTED_BG),
        "selected pill appearance"
    );
}

pub fn render_library_to_terminal(app: &mut App, layout: &mut LayoutMain) -> Terminal<TestBackend> {
    let backend = TestBackend::new(60, 20);
    let mut term = Terminal::new(backend).unwrap();
    let mut model = crate::app::shell::Model::new(std::mem::replace(app, make_app_stub()));
    model.sync_mounted_surfaces();
    term.draw(|f| {
        model
            .app
            .render_library(f, Rect::new(0, 0, 60, 20), layout, None);
        model.render_emby_browser_component(f);
        model.render_music_workspace_component(f);
    })
    .unwrap();
    *app = model.app;
    term
}

pub fn render_library_to_string(app: &mut App, layout: &mut LayoutMain) -> String {
    let term = render_library_to_terminal(app, layout);
    buffer_to_string(&term)
}

/// Like `render_library_to_string` but at an explicit terminal size, for
/// tests that need more rows than the default 60x20 (e.g. music-group views
/// whose hero panel reserves most of a short terminal).
pub fn render_library_to_string_sized(
    app: &mut App,
    layout: &mut LayoutMain,
    width: u16,
    height: u16,
) -> String {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    let mut model = crate::app::shell::Model::new(std::mem::replace(app, make_app_stub()));
    model.sync_mounted_surfaces();
    term.draw(|f| {
        model
            .app
            .render_library(f, Rect::new(0, 0, width, height), layout, None);
        model.render_emby_browser_component(f);
        model.render_music_workspace_component(f);
    })
    .unwrap();
    *app = model.app;
    buffer_to_string(&term)
}

pub fn render_view_to_terminal(
    app: &mut App,
    width: u16,
    height: u16,
) -> (Terminal<TestBackend>, LayoutMain) {
    // Mirror App::render(), which syncs terminal_width from the drawn Rect
    // before render_main runs -- without this, effective_panel_mode()/
    // effective_panel_focus() see whatever width the app was constructed
    // with instead of the width this call is actually rendering at. Only
    // terminal_width is touched here (the historical helper contract): the
    // terminal-normalization side effects of `compute_frame_layout` (image
    // cache clears, mini-view focus, queue-column clamping, terminal_height)
    // would change card reservation geometry for tests that render a view
    // at a different height than the stub default.
    app.terminal_width = width;
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    let mut layout = LayoutMain::default();
    term.draw(|f| {
        // Root/chrome geometry comes from the same authoritative paint-free
        // computation the live seam uses (task 2.1a).
        let chrome = app.compute_chrome_geometry(Rect::new(0, 0, width, height));
        layout.panel_area = chrome.panel_area;
        layout.panel_content_area = chrome.panel_content_area;
        app.render_main(
            f,
            Rect::new(0, 0, width, height),
            &chrome,
            &mut layout,
            &mut LayoutPlayback::default(),
            0,
            false,
            &None,
            None,
        );
    })
    .unwrap();
    (term, layout)
}

pub fn render_app_to_terminal(app: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| app.compose_base_frame(f, None)).unwrap();
    term
}

/// Render the Home destination exactly as the live shell does (task 5.3d,
/// Home legacy underpaint removal): draw the legacy `App::render` base frame
/// — which for Home now only reserves `home_area` — then paint the mounted
/// `HomeComponent` through the real `Model::render_home_component` shell
/// path (which sizes the component by `home_area` and paints the cover image
/// it returned). Returns the model, so tests can read the component's own
/// painted geometry and App state, together with the terminal. This is the
/// Home characterization path once the legacy underpaint is gone.
///
/// Home content is Model-owned (task 5.3d), so a test that needs seeded
/// Continue Watching rows/pills uses `render_home_shell_with` and seeds
/// `model.home_content` before the push.
pub fn render_queue_shell(
    mut app: App,
    width: u16,
    height: u16,
) -> (crate::app::shell::Model, Terminal<TestBackend>) {
    app.terminal_width = width;
    app.terminal_height = height;
    let mut model = crate::app::shell::Model::new(app);
    model.sync_queue();
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        model.app.compose_base_frame(f, None);
        model.render_queue_component(f);
    })
    .unwrap();
    (model, term)
}

pub fn render_home_shell(
    app: App,
    width: u16,
    height: u16,
) -> (crate::app::shell::Model, Terminal<TestBackend>) {
    render_home_shell_with(app, width, height, |_| {})
}

/// `render_home_shell` with a content-seeding callback: the test seeds
/// Model-owned `home_content` (task 5.3d) right after `Model::new` and
/// before `push_home_content` projects it into the mounted `HomeComponent`.
pub fn render_home_shell_with(
    mut app: App,
    width: u16,
    height: u16,
    seed: impl FnOnce(&mut crate::app::shell::Model),
) -> (crate::app::shell::Model, Terminal<TestBackend>) {
    app.terminal_width = width;
    app.terminal_height = height;
    let mut model = crate::app::shell::Model::new(app);
    seed(&mut model);
    model.push_home_content();
    model.sync_active_destination();
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        model.app.compose_base_frame(f, None);
        model.render_home_component(f);
    })
    .unwrap();
    (model, term)
}

pub fn render_view(app: &mut App, width: u16, height: u16) -> LayoutMain {
    render_view_to_terminal(app, width, height).1
}

/// Assert, against a *painted* buffer, that a Wide hero list pane leaves
/// exactly one blank row between its framed bottom border and the status-bar
/// row that `wide_hero_presentation` reserves (migrate-home-feeds slice 3.2
/// §5.1). This is the per-family §5 geometry check: re-derived layout rects
/// cannot catch a one-row vertical shift, so it reads the glyphs instead.
///
/// `pane` is the list pane's rect; `status_row_y` is the row the status bar
/// occupies (one below the pane's bottom edge). The framed panel paints its
/// `▁` bottom border on `status_row_y - 2`, and `status_row_y - 1` must be
/// blank. A one-row shift up moves the border off `status_row_y - 2`; a shift
/// down paints the reserve row — either way an assertion here fails.
pub fn assert_list_pane_reserves_one_row_above_status(
    buffer: &ratatui::buffer::Buffer,
    pane: ratatui::layout::Rect,
    status_row_y: u16,
) {
    let border_y = status_row_y - 2;
    let reserve_y = status_row_y - 1;
    assert_eq!(
        buffer[(pane.x, border_y)].symbol(),
        "▁",
        "framed list panel must paint its bottom border on row {border_y}"
    );
    for x in pane.x..pane.right() {
        assert_eq!(
            buffer[(x, reserve_y)].symbol(),
            " ",
            "the reserve row {reserve_y} between the list panel and the status bar must be blank (x={x})"
        );
    }
}
