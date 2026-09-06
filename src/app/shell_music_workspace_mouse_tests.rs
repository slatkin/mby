use super::tests::{make_item, make_music_group_app};
use super::*;
use crate::app::components::msg::{AlbumCursorKind, ShellRequest};
use crate::app::components::Msg;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tuirealm::event::{
    Event, Key, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

#[test]
fn narrow_short_wide_grouped_music_moves_one_album_per_down_and_page() {
    let mut model = Model::new(make_music_group_app());
    model.app.layout.main.left_area = ratatui::layout::Rect::new(0, 0, 100, 6);
    model.app.layout.main.wide_music_area = ratatui::layout::Rect::default();
    for index in 2..8 {
        let name = format!("Album {index}");
        let mut album = make_item(&name, "MusicAlbum");
        album.id = format!("album-{index}");
        model.app.libs[0].nav_stack[1].items.push(album);
    }
    model.sync_music_workspace();
    model.sync_active_destination();
    let id = model.music_workspace_id.clone().unwrap();
    let mut terminal = Terminal::new(TestBackend::new(100, 6)).unwrap();
    terminal
        .draw(|frame| model.render_music_workspace_component(frame))
        .unwrap();
    let message = model
        .application
        .get_component_mut(&id)
        .unwrap()
        .on(&Event::Keyboard(KeyEvent {
            code: Key::Down,
            modifiers: KeyModifiers::NONE,
        }));
    assert!(matches!(
        message,
        Some(Msg::Shell(ShellRequest::MusicAlbumCursor { target: 1, .. }))
    ));
    let message = model
        .application
        .get_component_mut(&id)
        .unwrap()
        .on(&Event::Keyboard(KeyEvent {
            code: Key::PageDown,
            modifiers: KeyModifiers::NONE,
        }));
    assert!(matches!(
        message,
        Some(Msg::Shell(ShellRequest::MusicAlbumCursor { target: 6, .. }))
    ));
}

#[test]
fn narrow_music_album_click_selects_and_requests_cursor_move() {
    let mut model = Model::new(make_music_group_app());
    model.app.layout.main.left_area = ratatui::layout::Rect::new(0, 0, 60, 9);
    model.app.layout.main.wide_music_area = ratatui::layout::Rect::default();
    for index in 0..4 {
        let mut album = make_item(&format!("Album {}", index + 2), "MusicAlbum");
        album.id = format!("album-{}", index + 2);
        album.artist = "Alpha".into();
        model.app.libs[0].nav_stack[1].items.push(album);
    }
    model.sync_music_workspace();
    model.sync_active_destination();
    let id = model.music_workspace_id.clone().unwrap();
    let mut terminal = Terminal::new(TestBackend::new(60, 9)).unwrap();
    terminal
        .draw(|frame| model.render_music_workspace_component(frame))
        .unwrap();
    let (col, heading_row, item_row) = {
        let c = model
            .application
            .get_component(&id)
            .unwrap()
            .as_any()
            .downcast_ref::<MusicWorkspaceComponent>()
            .unwrap();
        let area = c.test_narrow_list_area();
        (area.x + 1, area.y, area.y + 2)
    };

    // A click on the artist heading resolves to nothing.
    let heading = model
        .application
        .get_component_mut(&id)
        .unwrap()
        .on(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row: heading_row,
            modifiers: KeyModifiers::NONE,
        }));
    assert_eq!(heading, None);

    let message = model
        .application
        .get_component_mut(&id)
        .unwrap()
        .on(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row: item_row,
            modifiers: KeyModifiers::NONE,
        }));
    let Some(Msg::Shell(ShellRequest::MusicAlbumCursor { target, kind })) = message else {
        panic!("expected MusicAlbumCursor, got {message:?}");
    };
    assert_eq!(kind, AlbumCursorKind::Move);
    let component = model
        .application
        .get_component(&id)
        .unwrap()
        .as_any()
        .downcast_ref::<MusicWorkspaceComponent>()
        .unwrap();
    assert_eq!(component.album_cursor(), target);
}

#[test]
fn narrow_music_album_double_click_requests_activation() {
    let mut model = Model::new(make_music_group_app());
    model.app.layout.main.left_area = ratatui::layout::Rect::new(0, 0, 60, 9);
    model.app.layout.main.wide_music_area = ratatui::layout::Rect::default();
    for index in 0..4 {
        let mut album = make_item(&format!("Album {}", index + 2), "MusicAlbum");
        album.id = format!("album-{}", index + 2);
        album.artist = "Alpha".into();
        model.app.libs[0].nav_stack[1].items.push(album);
    }
    model.sync_music_workspace();
    model.sync_active_destination();
    let id = model.music_workspace_id.clone().unwrap();
    let mut terminal = Terminal::new(TestBackend::new(60, 9)).unwrap();
    terminal
        .draw(|frame| model.render_music_workspace_component(frame))
        .unwrap();
    let (col, row) = {
        let c = model
            .application
            .get_component(&id)
            .unwrap()
            .as_any()
            .downcast_ref::<MusicWorkspaceComponent>()
            .unwrap();
        let area = c.test_narrow_list_area();
        (area.x + 1, area.y + 2)
    };
    let down = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    });
    let first = model.application.get_component_mut(&id).unwrap().on(&down);
    assert!(matches!(
        first,
        Some(Msg::Shell(ShellRequest::MusicAlbumCursor { .. }))
    ));
    let second = model.application.get_component_mut(&id).unwrap().on(&down);
    assert_eq!(second, Some(Msg::Shell(ShellRequest::MusicAlbumActivate)));
}

#[test]
fn narrow_music_group_pill_click_requests_relative_group_switch() {
    let mut model = Model::new(make_music_group_app());
    model.app.layout.main.left_area = ratatui::layout::Rect::new(0, 0, 100, 6);
    model.app.layout.main.wide_music_area = ratatui::layout::Rect::default();
    model.sync_music_workspace();
    model.sync_active_destination();
    let id = model.music_workspace_id.clone().unwrap();
    let mut terminal = Terminal::new(TestBackend::new(100, 6)).unwrap();
    terminal
        .draw(|frame| model.render_music_workspace_component(frame))
        .unwrap();
    let (col, row, target_group) = {
        let c = model
            .application
            .get_component(&id)
            .unwrap()
            .as_any()
            .downcast_ref::<MusicWorkspaceComponent>()
            .unwrap();
        let (rect, group) = c
            .test_pill_regions()
            .iter()
            .find(|(_, group)| *group == 2)
            .copied()
            .expect("third group pill painted");
        (rect.x + 1, rect.y, group)
    };
    assert_eq!(target_group, 2);
    let message = model
        .application
        .get_component_mut(&id)
        .unwrap()
        .on(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    assert_eq!(
        message,
        Some(Msg::Shell(ShellRequest::MusicGroupSwitch { delta: 2 }))
    );
}

#[test]
fn narrow_music_album_right_click_carries_pointer_anchor() {
    let mut model = Model::new(make_music_group_app());
    model.app.layout.main.left_area = ratatui::layout::Rect::new(0, 0, 60, 9);
    model.app.layout.main.wide_music_area = ratatui::layout::Rect::default();
    for index in 0..4 {
        let mut album = make_item(&format!("Album {}", index + 2), "MusicAlbum");
        album.id = format!("album-{}", index + 2);
        album.artist = "Alpha".into();
        model.app.libs[0].nav_stack[1].items.push(album);
    }
    model.sync_music_workspace();
    model.sync_active_destination();
    let id = model.music_workspace_id.clone().unwrap();
    let mut terminal = Terminal::new(TestBackend::new(60, 9)).unwrap();
    terminal
        .draw(|frame| model.render_music_workspace_component(frame))
        .unwrap();
    let (col, row) = {
        let c = model
            .application
            .get_component(&id)
            .unwrap()
            .as_any()
            .downcast_ref::<MusicWorkspaceComponent>()
            .unwrap();
        let area = c.test_narrow_list_area();
        (area.x + 1, area.y + 2)
    };
    let message = model
        .application
        .get_component_mut(&id)
        .unwrap()
        .on(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    assert_eq!(
        message,
        Some(Msg::Shell(ShellRequest::MusicAlbumContextMenu {
            anchor: (col, row)
        }))
    );
}

#[test]
fn wide_music_album_rail_click_still_requests_cursor_move() {
    let mut model = Model::new(make_music_group_app());
    model.app.layout.main.wide_music_area = ratatui::layout::Rect::new(0, 0, 100, 30);
    model.app.layout.main.wide_music_right_area = ratatui::layout::Rect::new(50, 0, 50, 30);
    model.sync_music_workspace();
    model.sync_active_destination();
    let id = model.music_workspace_id.clone().unwrap();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| model.render_music_workspace_component(frame))
        .unwrap();
    let (col, row) = {
        let c = model
            .application
            .get_component(&id)
            .unwrap()
            .as_any()
            .downcast_ref::<MusicWorkspaceComponent>()
            .unwrap();
        let area = c.layout().wide_music_browser_area;
        // Row 0 is the artist heading; row 1 is the sole album row.
        (area.x + 1, area.y + 1)
    };
    let message = model
        .application
        .get_component_mut(&id)
        .unwrap()
        .on(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    assert!(matches!(
        message,
        Some(Msg::Shell(ShellRequest::MusicAlbumCursor {
            target: 0,
            kind: AlbumCursorKind::Move,
        }))
    ));
}

#[test]
fn wide_music_rail_wheel_pages_albums() {
    let mut model = Model::new(make_music_group_app());
    for index in 2..=32 {
        let mut album = make_item(&format!("Album {index}"), "MusicAlbum");
        album.id = format!("album-{index}");
        album.artist = "Alpha".into();
        model.app.libs[0].nav_stack[1].items.push(album);
    }
    model.app.layout.main.left_area = ratatui::layout::Rect::new(0, 0, 100, 30);
    model.app.layout.main.wide_music_area = ratatui::layout::Rect::new(0, 0, 100, 30);
    model.app.layout.main.wide_music_right_area = ratatui::layout::Rect::new(50, 0, 50, 30);
    model.sync_music_workspace();
    model.sync_active_destination();
    let id = model.music_workspace_id.clone().unwrap();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| model.render_music_workspace_component(frame))
        .unwrap();
    let (column, row) = {
        let component = model
            .application
            .get_component(&id)
            .unwrap()
            .as_any()
            .downcast_ref::<MusicWorkspaceComponent>()
            .unwrap();
        let area = component.layout().wide_music_browser_area;
        (area.x + 1, area.y + 1)
    };
    let wheel = |kind| {
        Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    };
    let down = model
        .application
        .get_component_mut(&id)
        .unwrap()
        .on(&wheel(MouseEventKind::ScrollDown));
    let Some(Msg::Shell(ShellRequest::MusicAlbumCursor { target, kind })) = down else {
        panic!("expected MusicAlbumCursor, got {down:?}");
    };
    assert_eq!(kind, AlbumCursorKind::Page);
    assert_eq!(target, 30);
    model
        .application
        .get_component_mut(&id)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<MusicWorkspaceComponent>()
        .unwrap()
        .reset_mouse_gestures_for_test();
    let up = model
        .application
        .get_component_mut(&id)
        .unwrap()
        .on(&wheel(MouseEventKind::ScrollUp));
    assert!(matches!(
        up,
        Some(Msg::Shell(ShellRequest::MusicAlbumCursor {
            target: 0,
            kind: AlbumCursorKind::Page,
        }))
    ));
}

#[test]
fn wide_music_track_wheel_steps_track() {
    let mut model = Model::new(make_music_group_app());
    let mut first = make_item("Track One", "Audio");
    first.id = "track-1".into();
    let mut second = make_item("Track Two", "Audio");
    second.id = "track-2".into();
    model
        .app
        .album_tracks_cache
        .insert("album-1".into(), vec![first, second]);
    model.app.layout.main.wide_music_area = ratatui::layout::Rect::new(0, 0, 100, 30);
    model.app.layout.main.wide_music_right_area = ratatui::layout::Rect::new(50, 0, 50, 30);
    model.sync_music_workspace();
    model.sync_active_destination();
    let id = model.music_workspace_id.clone().unwrap();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| model.render_music_workspace_component(frame))
        .unwrap();
    let (column, row) = {
        let component = model
            .application
            .get_component_mut(&id)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<MusicWorkspaceComponent>()
            .unwrap();
        component.set_inline_track_focus_enabled(true);
        component.enter_track_focus();
        let (rect, _) = component.layout().wide_music_track_hitmap[0];
        (rect.x + 1, rect.y)
    };
    let message = model
        .application
        .get_component_mut(&id)
        .unwrap()
        .on(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    assert_eq!(message, None);
    let component = model
        .application
        .get_component(&id)
        .unwrap()
        .as_any()
        .downcast_ref::<MusicWorkspaceComponent>()
        .unwrap();
    assert_eq!(component.track_cursor(), Some(1));
}

#[test]
fn narrow_music_list_wheel_pages_albums() {
    let mut model = Model::new(make_music_group_app());
    for index in 2..=12 {
        let mut album = make_item(&format!("Album {index}"), "MusicAlbum");
        album.id = format!("album-{index}");
        album.artist = "Alpha".into();
        model.app.libs[0].nav_stack[1].items.push(album);
    }
    model.app.layout.main.left_area = ratatui::layout::Rect::new(0, 0, 60, 9);
    model.app.layout.main.wide_music_area = ratatui::layout::Rect::default();
    model.sync_music_workspace();
    model.sync_active_destination();
    let id = model.music_workspace_id.clone().unwrap();
    let mut terminal = Terminal::new(TestBackend::new(60, 9)).unwrap();
    terminal
        .draw(|frame| model.render_music_workspace_component(frame))
        .unwrap();
    let (column, row) = {
        let component = model
            .application
            .get_component(&id)
            .unwrap()
            .as_any()
            .downcast_ref::<MusicWorkspaceComponent>()
            .unwrap();
        let area = component.test_narrow_list_area();
        (area.x + 1, area.y + 2)
    };
    let message = model
        .application
        .get_component_mut(&id)
        .unwrap()
        .on(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    assert!(matches!(
        message,
        Some(Msg::Shell(ShellRequest::MusicAlbumCursor {
            target: 9,
            kind: AlbumCursorKind::Page,
        }))
    ));
}

#[test]
fn music_wheel_over_chrome_is_ignored() {
    for (wide, width, height) in [(true, 100, 30), (false, 60, 9)] {
        let mut model = Model::new(make_music_group_app());
        if wide {
            model.app.layout.main.wide_music_area = ratatui::layout::Rect::new(0, 0, width, height);
            model.app.layout.main.wide_music_right_area =
                ratatui::layout::Rect::new(50, 0, 50, height);
        } else {
            model.app.layout.main.left_area = ratatui::layout::Rect::new(0, 0, width, height);
            model.app.layout.main.wide_music_area = ratatui::layout::Rect::default();
        }
        model.sync_music_workspace();
        model.sync_active_destination();
        let id = model.music_workspace_id.clone().unwrap();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| model.render_music_workspace_component(frame))
            .unwrap();
        let (column, row) = {
            let component = model
                .application
                .get_component(&id)
                .unwrap()
                .as_any()
                .downcast_ref::<MusicWorkspaceComponent>()
                .unwrap();
            let (rect, _) = component
                .test_pill_regions()
                .first()
                .copied()
                .expect("painted group pill");
            (rect.x + 1, rect.y)
        };
        let message = model
            .application
            .get_component_mut(&id)
            .unwrap()
            .on(&Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            }));
        assert_eq!(message, None);
    }
}

#[test]
fn music_mouse_track_click_stays_component_local() {
    let mut model = Model::new(make_music_group_app());
    let mut track = make_item("Track One", "Audio");
    track.id = "track-1".into();
    let mut second_track = make_item("Track Two", "Audio");
    second_track.id = "track-2".into();
    model
        .app
        .album_tracks_cache
        .insert("album-1".into(), vec![track, second_track]);
    model.app.layout.main.wide_music_area = ratatui::layout::Rect::new(0, 0, 100, 30);
    model.app.layout.main.wide_music_right_area = ratatui::layout::Rect::new(50, 0, 50, 30);
    model.sync_music_workspace();
    model.sync_active_destination();
    let id = model
        .music_workspace_id
        .clone()
        .expect("wide Music workspace mounted");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| model.render_music_workspace_component(frame))
        .unwrap();
    let (column, row) = {
        let component = model
            .application
            .get_component(&id)
            .unwrap()
            .as_any()
            .downcast_ref::<MusicWorkspaceComponent>()
            .unwrap();
        assert_eq!(component.track_cursor(), None);
        let (rect, _) = component
            .layout()
            .wide_music_track_hitmap
            .get(1)
            .copied()
            .expect("painted second track hitmap");
        (rect.x + 1, rect.y)
    };
    let message = model
        .application
        .get_component_mut(&id)
        .unwrap()
        .on(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    assert_eq!(message, None);
    let component = model
        .application
        .get_component(&id)
        .unwrap()
        .as_any()
        .downcast_ref::<MusicWorkspaceComponent>()
        .unwrap();
    assert_eq!(component.track_cursor(), Some(1));
    assert_eq!(component.track_selected_row(), Some(1));
}
