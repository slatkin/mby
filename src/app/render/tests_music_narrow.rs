// §3.1 evidence for the narrow grouped-Music canonical migration.
//
// Narrow grouped Music now composes the persistent canonical
// `InlineMediaBrowser<String>` (`render_inline_media_browser`), exactly as the
// narrow TV series list does. These tests drive `MusicWorkspaceComponent::view`
// at the narrow breakpoint (and the full mounted `Model` for the breakpoint
// hand-off), asserting one-column geometry, non-selectable structural rows,
// selected-row replacement admission / ordinary-row fallback, the focused
// selection affordance, image-bearing fixtures, ordinary-refresh target
// retention, and the target/offset `ViewportAnchor` round trip across
// Wide -> Narrow -> Wide.

use super::test_helpers::{
    buffer_to_string, draw_mounted_frame, make_music_group_app, mounted_model_at,
    mounted_music_layout, mounted_music_scroll,
};
use super::*;
use crate::app::components::{ComponentId, MusicWorkspaceComponent};
use crate::app::shell::Model;
use crate::app::tests::make_item;
use crate::app::PanelFocus;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use tuirealm::component::Component;
use tuirealm::event::{Event, Key, KeyEvent, KeyModifiers};

const NW: u16 = 60;
const NH: u16 = 30;

/// A grouped-Music fixture with two artist groups (heading, inter-group
/// spacer, second heading) plus enough albums to overflow a narrow viewport.
fn multi_artist_app() -> App {
    let mut app = make_music_group_app();
    app.panel_focus = PanelFocus::Library;
    let level = app.libs[0].nav_stack.last_mut().unwrap();
    for i in 1..40 {
        let mut album = make_item(&format!("Alpha Album {i:02}"), "MusicAlbum");
        album.id = format!("alpha-{i}");
        album.artist = "Alpha".into();
        level.items.push(album);
    }
    for i in 0..6 {
        let mut album = make_item(&format!("Beta Album {i:02}"), "MusicAlbum");
        album.id = format!("beta-{i}");
        album.artist = "Beta".into();
        level.items.push(album);
    }
    level.total_count = level.items.len();
    app
}

fn render_narrow(
    app: &App,
    focused: bool,
    cursor: usize,
) -> (Terminal<TestBackend>, MusicWorkspaceComponent) {
    let lib_idx = app.tab.emby_library_index().unwrap();
    let mut context = app.wide_music_render_ctx(lib_idx, None);
    context.focused = focused;
    let mut component = MusicWorkspaceComponent::new();
    component.set_content(context);
    component.set_focused(focused);
    component.re_anchor(cursor, 0);
    let mut terminal = Terminal::new(TestBackend::new(NW, NH)).unwrap();
    terminal
        .draw(|f| component.view(f, Rect::new(0, 0, NW, NH)))
        .unwrap();
    (terminal, component)
}

fn press(model: &mut Model, code: Key) {
    let focused = model.application.focus().cloned();
    if let Some(id) = &focused {
        let msg = model
            .application
            .get_component_mut(id)
            .expect("focused component mounted")
            .on(&Event::Keyboard(KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
            }));
        if let Some(msg) = msg {
            let mut music_resize = false;
            let mut tv_resize = false;
            model.handle_terminal_message(msg, &mut music_resize, &mut tv_resize);
        }
    }
    model.sync_mounted_surfaces();
}

fn album_cursor(model: &Model, id: &ComponentId) -> usize {
    model
        .application
        .get_component(id)
        .and_then(|c| c.as_any().downcast_ref::<MusicWorkspaceComponent>())
        .map(MusicWorkspaceComponent::album_cursor)
        .expect("music workspace album cursor")
}

#[test]
fn narrow_music_one_column_geometry_and_non_selectable_structural_rows() {
    let app = multi_artist_app();
    let (_terminal, component) = render_narrow(&app, true, 0);
    let layout = component.layout();

    // Every published item row carries exactly one album index (one column).
    assert!(
        layout.left_item_rows.iter().all(|row| row.len() <= 1),
        "narrow grouped Music is one column: {:?}",
        layout.left_item_rows
    );
    let album_rows = layout
        .left_row_targets
        .iter()
        .filter(|t| t.is_some())
        .count();
    let structural_rows = layout
        .left_row_targets
        .iter()
        .filter(|t| t.is_none())
        .count();
    assert!(album_rows >= 3, "album rows are selectable targets");
    assert!(
        structural_rows >= 2,
        "artist headings + inter-group spacer publish no target: {:?}",
        layout.left_row_targets
    );
    // The first painted row is the "Alpha" heading -> no target.
    assert_eq!(layout.left_row_targets.first(), Some(&None));
}

#[test]
fn narrow_music_admits_the_selected_album_detail_block() {
    let app = multi_artist_app();
    let (_terminal, component) = render_narrow(&app, true, 0);
    let layout = component.layout();

    assert!(
        layout.hero_area.height > 0,
        "the selected album's inline detail block is admitted at a tall viewport"
    );
    assert_eq!(layout.selected_item_rect, Some(layout.hero_area));
    // Exactly one selected-album parent target under the replacement.
    assert_eq!(
        layout
            .left_row_targets
            .iter()
            .filter(|t| matches!(t, Some(0)))
            .count(),
        1
    );
}

#[test]
fn narrow_music_falls_back_to_the_ordinary_selected_row_when_the_block_cannot_fit() {
    let app = multi_artist_app();
    let lib_idx = app.tab.emby_library_index().unwrap();
    let mut context = app.wide_music_render_ctx(lib_idx, None);
    context.focused = true;
    let mut component = MusicWorkspaceComponent::new();
    component.set_content(context);
    component.set_focused(true);
    component.re_anchor(0, 0);

    // A viewport barely taller than the pill row: the detail block cannot be
    // admitted, so the ordinary selected row is restored.
    let short = Rect::new(0, 0, NW, 8);
    let mut terminal = Terminal::new(TestBackend::new(NW, 8)).unwrap();
    terminal.draw(|f| component.view(f, short)).unwrap();
    let layout = component.layout();

    assert_eq!(
        layout.hero_area,
        Rect::default(),
        "no detail block admitted"
    );
    let selected = layout
        .selected_item_rect
        .expect("ordinary selected-row rect published on fallback");
    assert_eq!(selected.height, 1, "fallback restores a one-line row");
    assert!(buffer_to_string(&terminal).contains("First Album"));
}

#[test]
fn narrow_music_focused_selection_carries_the_canonical_highlight() {
    let app = multi_artist_app();
    // Fallback viewport so the ordinary selected row (not the hero) is painted.
    let lib_idx = app.tab.emby_library_index().unwrap();
    let mut context = app.wide_music_render_ctx(lib_idx, None);
    context.focused = true;
    let mut focused = MusicWorkspaceComponent::new();
    focused.set_content(context.clone());
    focused.set_focused(true);
    focused.re_anchor(0, 0);
    let mut unfocused = MusicWorkspaceComponent::new();
    context.focused = false;
    unfocused.set_content(context);
    unfocused.set_focused(false);
    unfocused.re_anchor(0, 0);

    let area = Rect::new(0, 0, NW, 8);
    let mut ft = Terminal::new(TestBackend::new(NW, 8)).unwrap();
    ft.draw(|f| focused.view(f, area)).unwrap();
    let mut ut = Terminal::new(TestBackend::new(NW, 8)).unwrap();
    ut.draw(|f| unfocused.view(f, area)).unwrap();

    let rect = focused.layout().selected_item_rect.unwrap();
    let fbg = ft.backend().buffer()[(rect.x + 4, rect.y)].bg;
    let ubg = ut.backend().buffer()[(rect.x + 4, rect.y)].bg;
    assert_ne!(
        fbg, ubg,
        "the focused selected row is highlighted; the unfocused one is not"
    );
}

#[test]
fn narrow_music_image_bearing_fixture_emits_the_selected_album_art() {
    let mut app = multi_artist_app();
    app.image_protocol_enabled = true;
    let mut model = mounted_model_at(app, NW, NH);
    let _ = draw_mounted_frame(&mut model, NW, NH);
    assert!(
        model
            .app
            .card_image_loading
            .iter()
            .any(|k| k.starts_with("album-1")),
        "the selected album's hero art is requested: {:?}",
        model.app.card_image_loading
    );
}

#[test]
fn narrow_music_ordinary_refresh_retains_the_selected_album_target() {
    let mut model = mounted_model_at(multi_artist_app(), NW, NH);
    let _ = draw_mounted_frame(&mut model, NW, NH);
    let id = model
        .music_workspace_id
        .clone()
        .expect("music workspace mounted");

    // Move the component's own cursor, then push an ordinary content refresh:
    // the selected album survives without a shell cursor mirror.
    press(&mut model, Key::Char('j'));
    let _ = draw_mounted_frame(&mut model, NW, NH);
    let moved = album_cursor(&model, &id);
    assert!(moved > 0, "j moved the album cursor");

    model.push_music_workspace_content();
    let _ = draw_mounted_frame(&mut model, NW, NH);
    assert_eq!(
        album_cursor(&model, &id),
        moved,
        "an ordinary refresh keeps the component's divergent cursor"
    );
    assert_eq!(
        mounted_music_layout(&model)
            .left_row_targets
            .iter()
            .filter(|t| matches!(t, Some(idx) if *idx == moved))
            .count(),
        1
    );
}

/// Draw one frame after syncing `app.terminal_width/height` to the new size,
/// mirroring the production resize-event path (`apply_terminal_observer`) that
/// `draw_mounted_frame` alone does not emulate.
fn resize_draw(model: &mut Model, width: u16, height: u16) -> String {
    model.app.terminal_width = width;
    model.app.terminal_height = height;
    draw_mounted_frame(model, width, height)
}

#[test]
fn narrow_music_viewport_anchor_round_trips_across_wide_narrow_wide() {
    let mut app = multi_artist_app();
    // Select the last album so the wide rail must scroll: the anchor has a
    // non-trivial target and screen-row offset to preserve.
    let last = app.libs[0].nav_stack.last().unwrap().items.len() - 1;
    app.libs[0]
        .nav_stack
        .last_mut()
        .unwrap()
        .set_resting_cursor(last);

    let mut model = mounted_model_at(app, 160, 40);
    let _ = resize_draw(&mut model, 160, 40);
    let id = model
        .music_workspace_id
        .clone()
        .expect("music workspace mounted");
    assert!(
        model.app.layout.main.wide_music_right_area.width > 0
            && model.app.layout.main.wide_music_right_area.height > 0
    );
    assert_eq!(album_cursor(&model, &id), last);
    let wide_scroll = mounted_music_scroll(&model);
    assert!(wide_scroll > 0, "the bottom album scrolls the wide rail");
    let wide_offset = wide_anchor_offset(&model, &id);

    // Wide -> Narrow: the selected album and its screen-row offset carry over.
    let narrow = resize_draw(&mut model, 60, 30);
    assert!(
        !(model.app.layout.main.wide_music_right_area.width > 0
            && model.app.layout.main.wide_music_right_area.height > 0)
    );
    assert_eq!(
        album_cursor(&model, &id),
        last,
        "selection survives the flip:\n{narrow}"
    );
    assert!(
        narrow.contains("Beta Album"),
        "the narrow hero paints the carried-over selected album:\n{narrow}"
    );

    // Narrow -> Wide: the album, cursor, scroll offset, and screen-row offset
    // all return to the wide arrangement.
    let _ = resize_draw(&mut model, 160, 40);
    assert!(
        model.app.layout.main.wide_music_right_area.width > 0
            && model.app.layout.main.wide_music_right_area.height > 0
    );
    assert_eq!(album_cursor(&model, &id), last);
    assert_eq!(
        mounted_music_scroll(&model),
        wide_scroll,
        "wide album_scroll recomputes to the identical bottom-anchored offset"
    );
    assert_eq!(
        wide_anchor_offset(&model, &id),
        wide_offset,
        "the selected-row screen offset is preserved across the round trip"
    );
}

#[test]
fn narrow_music_reused_model_paints_after_a_wide_to_narrow_resize() {
    // Unit-1 investigation: a single `Model` resized Wide -> Narrow was
    // reported painting the narrow grouped-Music buffer blank. The cause is
    // the test harness, not the painter -- `draw_mounted_frame` does not sync
    // `app.terminal_width/height`, so `compose_base_frame` collapses
    // `left_area` to 0x0 and the workspace `view` early-returns. With the
    // dimensions synced (as the real resize-event path does), the canonical
    // persistent `InlineMediaBrowser` recomputes its flow and paints the
    // album rows on the resized frame.
    let mut model = mounted_model_at(multi_artist_app(), 160, 40);
    let _ = resize_draw(&mut model, 160, 40);
    assert!(
        model.app.layout.main.wide_music_right_area.width > 0
            && model.app.layout.main.wide_music_right_area.height > 0
    );

    let narrow = resize_draw(&mut model, 60, 30);
    assert!(
        !(model.app.layout.main.wide_music_right_area.width > 0
            && model.app.layout.main.wide_music_right_area.height > 0)
    );
    assert!(
        narrow.contains("First Album") || narrow.contains("Alpha Album"),
        "narrow grouped Music must paint album rows after the resize:\n{narrow}"
    );

    let wide = resize_draw(&mut model, 160, 40);
    assert!(
        model.app.layout.main.wide_music_right_area.width > 0
            && model.app.layout.main.wide_music_right_area.height > 0
    );
    assert!(wide.contains("First Album") || wide.contains("Alpha Album"));
}

#[test]
fn narrow_music_applies_the_flip_anchor_at_the_content_viewport_height() {
    // The write side (`render_narrow_music_group_with_ctx` applying a pending
    // `ViewportAnchor`) must use the *content* viewport height -- the same
    // height the read side (`viewport_anchor`) measures its offset against --
    // not the full `area.height`. Geometry chosen so the two heights clamp the
    // resting scroll to different values, and the painter's downstream
    // re-clamp does not mask the difference.
    use crate::app::components::media_list::{InlineMediaBrowser, ViewportAnchor};
    use crate::app::layout::LayoutMain;
    use crate::app::render::arrangements::wide_hero::pill_bar_areas;

    let app = multi_artist_app();
    let lib_idx = app.tab.emby_library_index().unwrap();
    let ctx = app
        .wide_music_render_ctx(lib_idx, None)
        .with_local_state(27, 0, None);

    let rows = ctx.grouped_rows();
    let target = ctx.list.items[27].id.clone();
    let display_row = rows
        .iter()
        .position(|row| row.selectable_target() == Some(&target))
        .expect("selected album is in the flow");

    let mut browser: InlineMediaBrowser<String> = InlineMediaBrowser::new();
    browser.set_content(rows);
    browser.select_target(&target);

    let area = Rect::new(0, 0, 60, 26);
    let content_h = pill_bar_areas(area).content_area.height as usize;
    let want_offset = 4usize;
    let anchor = ViewportAnchor {
        selected_target: target,
        selected_row_offset: want_offset,
    };
    // Sanity: the two candidate heights really do clamp differently here.
    let n_rows = browser.rows().len();
    assert!(display_row - want_offset > n_rows - area.height as usize);
    assert!(display_row - want_offset <= n_rows - content_h);

    let mut layout = LayoutMain::default();
    let mut terminal = Terminal::new(TestBackend::new(60, 26)).unwrap();
    let mut output = None;
    terminal
        .draw(|f| {
            output = Some(render_narrow_music_group_with_ctx(
                f,
                area,
                &ctx,
                &mut layout,
                &mut browser,
                Some(&anchor),
            ));
        })
        .unwrap();

    assert_eq!(
        display_row - output.unwrap().final_scroll,
        want_offset,
        "the anchor landed the selected row at its requested content-viewport offset"
    );
}

/// The wide selected-row screen offset the component published this frame
/// (index of the selected album row below the artist header + earlier albums).
fn wide_anchor_offset(model: &Model, id: &ComponentId) -> usize {
    let component = model
        .application
        .get_component(id)
        .and_then(|c| c.as_any().downcast_ref::<MusicWorkspaceComponent>())
        .expect("music workspace");
    let layout = component.layout();
    let rect = layout.selected_item_rect.expect("wide selected-row rect");
    (rect.y - layout.wide_music_browser_area.y) as usize
}
