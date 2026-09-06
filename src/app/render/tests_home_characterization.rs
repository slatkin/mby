use super::test_helpers::{
    buffer_to_string, make_movie_app, render_app_to_terminal, render_home_shell_with,
};
use super::*;
use crate::app::components::{ComponentId, HomeComponent};
use crate::app::tests::make_app_stub;
use crate::app::{palette, PanelFocus, TabSelection};

fn home_app() -> App {
    let mut app = make_app_stub();
    app.tab = TabSelection::Home;
    app.mini_view_focus = PanelFocus::Library;
    app
}

/// The Continue Watching item the characterization seeds into Model-owned
/// `home_content` (task 5.3d).
fn emby_cw_item() -> mbv_core::api::EmbyItem {
    let movie_app = make_movie_app();
    movie_app.libs[0].nav_stack[0].items[0].clone()
}

/// Task 5.3d, Home legacy underpaint removal — regression: the legacy base
/// frame (`App::render`) no longer paints any Home content before the
/// mounted component view runs. It still reserves the full Home destination
/// area (`home_area`) as the placement handoff, but paints no Home rows,
/// pills, or hero there. Home content is Model-owned now (task 5.3d), so
/// the legacy frame never even holds a copy to (not) paint.
///
/// `remove-migrated-surface-underpaint` 3.1 (D4): the Home dispatch arm
/// (`render_library`, `src/app/render/components/widgets.rs:528`) is
/// `layout.home_area = area` with no width branch, so the geometry-only
/// hand-off holds at every breakpoint; the wide case is exercised here too.
#[test]
fn legacy_base_frame_does_not_paint_home_content_before_the_component() {
    for (width, height) in [(60, 20), (120, 40)] {
        let mut app = home_app();
        app.terminal_width = width;
        app.terminal_height = height;
        let terminal = render_app_to_terminal(&mut app, width, height);
        assert!(
            app.layout.main.home_area.height > 0,
            "legacy frame must still reserve home_area at {width}x{height}: {:?}",
            app.layout.main.home_area
        );
        let output = buffer_to_string(&terminal);
        assert!(
            !output.contains("Focused Movie"),
            "legacy frame must not paint Home rows/hero before the component \
             at {width}x{height}: {output:?}"
        );
    }
}

/// Task 5.3d, Home legacy underpaint removal: this characterization now
/// renders through the mounted `HomeComponent` (via the shell-equivalent
/// `render_home_shell` helper) instead of the legacy `App`-only frame, which
/// no longer paints Home content at all. The behavioral assertion — each
/// width/focused state still paints the selected movie's hero/list — is
/// unchanged.
#[test]
fn home_buffer_characterization_covers_wide_unfocused_narrow_and_selected_states() {
    let states = [
        (120, 40, true),
        (120, 40, false),
        (60, 40, true),
        (60, 12, true),
    ];
    for (width, height, focused) in states {
        let mut app = home_app();
        if !focused {
            app.panel_focus = PanelFocus::Queue;
        }
        let cw_item = emby_cw_item();
        let (_model, terminal) = render_home_shell_with(app, width, height, |m| {
            m.home_content.continue_items = vec![cw_item];
        });
        let output = buffer_to_string(&terminal);
        assert!(
            output.contains("Focused Movie"),
            "home hero/list missing in {width}x{height}: {output:?}"
        );
    }
}

/// `remove-migrated-surface-underpaint` D3 + the "Startup content" risk
/// bullet: task 2.4 routes the two startup `terminal.draw` sites in
/// `Model::run` (`src/app/shell_run.rs`) through `Model::draw_frame`, so the
/// first frame now paints the full base frame *and* the mounted component
/// views — not the old chrome-only flash. This characterizes that the startup
/// Home frame shows the mounted `HomeComponent`'s loading affordances (its
/// painted pill bar and empty-state placeholder while home_content.loading is
/// still set and no content has arrived) rather than blank panes.
#[test]
fn startup_frame_paints_loading_affordances_not_blank_panes() {
    let mut app = home_app();
    app.terminal_width = 100;
    app.terminal_height = 30;
    let mut model = crate::app::shell::Model::new(app);
    // The precondition `Model::run` sets before its first `terminal.draw`
    // (`src/app/shell_run.rs`): the Home destination is still loading.
    model.home_content.loading = true;
    model.push_home_content();

    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut term = ratatui::Terminal::new(backend).unwrap();
    term.draw(|f| model.draw_frame(f, false, false)).unwrap();
    let output = buffer_to_string(&term);

    assert!(
        output.split_whitespace().next().is_some(),
        "startup frame must not be an empty buffer"
    );
    assert!(
        output.contains("Continue"),
        "startup frame must paint the mounted HomeComponent's pill bar, not \
         just legacy chrome: {output:?}"
    );
    assert!(
        output.contains("(empty)"),
        "startup Home pane must paint its empty-state placeholder, not a \
         blank pane: {output:?}"
    );
}

/// Task 5.3d, Home legacy underpaint removal: the pill targets are now
/// characterized from the single painter — the mounted `HomeComponent`'s
/// own `pill_targets` — rather than `LayoutMain.selector_tabs`, which the
/// legacy frame no longer populates for Home. The assertions are preserved:
/// one Continue-Watching pill (id 0), the targets share one painted row, the
/// selected pill is highlighted, and exactly one pill bar row is painted.
#[test]
fn home_pill_row_and_targets_are_characterized_end_to_end() {
    let cw_item = emby_cw_item();
    let (model, terminal) = render_home_shell_with(home_app(), 60, 20, |m| {
        m.home_content.continue_items = vec![cw_item];
    });

    let home = model
        .application
        .get_component(&ComponentId::Home)
        .expect("Home component mounted")
        .as_any()
        .downcast_ref::<HomeComponent>()
        .expect("Home component type");
    let targets = home.test_pill_targets();
    assert_eq!(
        targets.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
        vec![0],
        "Home pill targets"
    );
    let first = targets.first().expect("Home should publish pill targets").0;
    assert!(
        targets
            .iter()
            .all(|(rect, _)| rect.y == first.y && rect.height == 1),
        "pill hitboxes must occupy one shared row: {targets:?}"
    );

    let buffer = terminal.backend().buffer();
    let selected = targets
        .iter()
        .find(|(_, id)| *id == 0)
        .expect("selected pill id should have a hitbox")
        .0;
    assert_eq!(
        buffer[(selected.x + 1, selected.y)].style().bg,
        Some(palette::PILL_SELECTED_BG),
        "selected pill appearance"
    );
    let row_text = (0..buffer.area().width)
        .map(|x| buffer[(x, first.y)].symbol())
        .collect::<String>();
    assert!(
        row_text.contains("⌘"),
        "pill row missing glyph: {row_text:?}"
    );
    assert!(
        row_text.contains("Continue"),
        "pill row missing label: {row_text:?}"
    );
    let pill_rows = (0..buffer.area().height)
        .filter(|y| buffer[(first.x, *y)].symbol() == "◢")
        .collect::<Vec<_>>();
    assert_eq!(
        pill_rows,
        vec![first.y],
        "Home must paint exactly one pill bar row"
    );
}

/// migrate-home-feeds 4.6 regression: after the full wide-Home arrangement
/// paint the focused selected row's background is the surface *containing*
/// the list panel (`SURFACE_RESTING`) — not the panel's focus-green fill and
/// not the old `SURFACE_BACKDROP` — and the rail-framing helper (now run
/// before the row flow) must not overpaint it. Unfocused: no bar.
#[test]
fn wide_home_selected_row_punches_through_to_the_resting_surface() {
    let bgs = |focused: bool| {
        let mut app = home_app();
        if !focused {
            app.panel_focus = PanelFocus::Queue;
        }
        let cw_item = emby_cw_item();
        let (model, terminal) = render_home_shell_with(app, 160, 40, |m| {
            m.home_content.continue_items = vec![cw_item];
        });
        let home = model
            .application
            .get_component(&ComponentId::Home)
            .expect("Home component mounted")
            .as_any()
            .downcast_ref::<HomeComponent>()
            .expect("Home component type");
        let (_, selected) = home.menu_placement_geometry();
        let selected = selected.expect("wide Home publishes a selected-row rect");
        let buffer = terminal.backend().buffer();
        (
            buffer[(selected.x, selected.y)].style().bg,
            buffer[(selected.x, selected.y + 1)].style().bg,
        )
    };

    let (selected, body) = bgs(true);
    assert_eq!(selected, Some(palette::SURFACE_RESTING));
    assert_eq!(body, Some(palette::resolve_surface_focus(true)));
    assert_ne!(selected, body);

    let (selected, body) = bgs(false);
    assert_eq!(selected, body, "unfocused wide Home shows no selection bar");
}

/// migrate-home-feeds 4.6 regression: narrow Home no longer floods the whole
/// list pane with `resolve_surface_focus(focused)` (reverts 14fb8435). The
/// focus-aware surface now lives only on the inline-hero shell. Narrow Home is
/// only reachable while its mini-view half holds focus, so there is no
/// unfocused narrow case to characterize — the shell just has to carry the
/// focused surface.
#[test]
fn narrow_home_hero_shell_carries_the_focus_surface() {
    let app = home_app();
    let (model, terminal) = render_home_shell_with(app, 60, 40, |m| {
        m.home_content.continue_items = vec![emby_cw_item()];
    });
    let home = model
        .application
        .get_component(&ComponentId::Home)
        .expect("Home component mounted")
        .as_any()
        .downcast_ref::<HomeComponent>()
        .expect("Home component type");
    let hero = home.hero_area().expect("narrow Home paints an inline hero");
    let expected = palette::resolve_surface_focus(true);
    let matches = (hero.left()..hero.right())
        .flat_map(|x| (hero.top()..hero.bottom()).map(move |y| (x, y)))
        .filter(|&(x, y)| terminal.backend().buffer()[(x, y)].style().bg == Some(expected))
        .count();
    assert!(matches > 0, "hero shell missing the focus surface");
}

/// migrate-home-feeds 4.6 regression: focused narrow Home with an inline hero
/// reads as a recessed card — the hero-shell background differs from the pane
/// backdrop showing behind non-selected rows (Movies narrow parity). Before
/// the 14fb8435 revert the pane flood made the two identical.
#[test]
fn narrow_home_inline_hero_contrasts_with_pane_backdrop() {
    let app = home_app();
    let (model, terminal) = render_home_shell_with(app, 60, 40, |m| {
        m.home_content.continue_items = vec![emby_cw_item()];
    });
    let home = model
        .application
        .get_component(&ComponentId::Home)
        .expect("Home component mounted")
        .as_any()
        .downcast_ref::<HomeComponent>()
        .expect("Home component type");
    let (area, _) = home.menu_placement_geometry();
    let hero = home.hero_area().expect("narrow Home paints an inline hero");
    let buffer = terminal.backend().buffer();

    let hero_bg = buffer[(hero.x + 1, hero.y + 1)].style().bg;
    assert_eq!(hero_bg, Some(palette::resolve_surface_focus(true)));

    // A row cell above the hero: pane backdrop from `chrome.rs`, never flooded.
    let backdrop_bg = buffer[(area.x, hero.y.saturating_sub(1))].style().bg;
    assert_eq!(backdrop_bg, Some(palette::SURFACE_BACKDROP));
    assert_ne!(hero_bg, backdrop_bg, "hero must read as a recessed card");
}

/// migrate-home-feeds 5.1: the shared Wide hero primitive owns the one-row
/// status-row reserve, so wide Home's hero panel and list panel must both
/// bottom out exactly one row above the destination area's bottom (the status
/// bar row) — no per-tab reserve on top of the shared one.
#[test]
fn home_images_off_collapses_artwork_and_uses_full_text_width() {
    let mut app = home_app();
    app.image_protocol_enabled = false;
    let (model, terminal) = render_home_shell_with(app, 200, 40, |m| {
        m.home_content.continue_items = vec![emby_cw_item()];
    });
    let home = model
        .application
        .get_component(&ComponentId::Home)
        .expect("Home component mounted")
        .as_any()
        .downcast_ref::<HomeComponent>()
        .expect("Home component type");
    let hero = home.hero_area().expect("wide Home paints a hero panel");
    assert!(buffer_to_string(&terminal).contains("Focused Movie"));
    let (list_area, _) = home.menu_placement_geometry();
    assert!(
        list_area.width > hero.width,
        "images-off Home text must expand beyond the artwork-width hero"
    );
}

#[test]
fn wide_home_panes_leave_exactly_one_row_above_the_status_bar() {
    let (width, height) = (200u16, 40u16);
    let app = home_app();
    let (model, terminal) = render_home_shell_with(app, width, height, |m| {
        m.home_content.continue_items = vec![emby_cw_item()];
    });
    let home = model
        .application
        .get_component(&ComponentId::Home)
        .expect("Home component mounted")
        .as_any()
        .downcast_ref::<HomeComponent>()
        .expect("Home component type");
    // The status row sits one row below the Home destination area, not one
    // row below the terminal (chrome owns the rows under `home_area`).
    let home_area = model.app.layout.main.home_area;
    let hero = home.hero_area().expect("wide Home paints a hero panel");
    let (list_area, _) = home.menu_placement_geometry();
    assert_eq!(
        hero.bottom(),
        home_area.bottom() - 1,
        "hero panel must bottom out one row above the status row"
    );
    // Positive buffer check: the framed list panel paints its `▁` bottom border
    // two rows above the status row, leaving exactly one blank row between the
    // panel and the status bar. A one-row vertical shift is caught here.
    crate::app::render::test_helpers::assert_list_pane_reserves_one_row_above_status(
        terminal.backend().buffer(),
        list_area,
        home_area.bottom(),
    );
}
