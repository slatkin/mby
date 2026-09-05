#![allow(dead_code, unused_imports)]

use super::*;
use crate::app::library_browse_actions::{
    build_album_index_with, full_library_fetch_limit, recursive_album_search_eligible,
};
use crate::app::tests::{make_app_stub, make_item, make_items};
use crate::app::{
    AlbumIndexState, AlbumPathPart, AlbumSearchEntry, BrowseLevel, ContextAction,
    FeedHomeVideoState, LibEvent, LibraryTab, QueueScope, TabSelection,
};
use mbv_core::api::TICKS_PER_SECOND;
use mbv_core::player::PlayerEvent;
use std::collections::HashMap;
use std::sync::mpsc;

fn folder(id: &str, name: &str) -> EmbyItem {
    let mut item = make_item(name, "Folder");
    item.id = id.into();
    item.is_folder = true;
    item
}

fn album(id: &str, name: &str) -> EmbyItem {
    let mut item = make_item(name, "MusicAlbum");
    item.id = id.into();
    item.is_folder = true;
    item.media_type = "Audio".into();
    item
}

fn recursive_music_app() -> App {
    let mut app = make_app_stub();
    app.music_levels = vec!["group".into(), "artist".into(), "album".into()];
    let mut library = make_item("Music", "CollectionFolder");
    library.id = "music-lib".into();
    library.collection_type = "music".into();
    library.is_folder = true;
    app.libs.push(LibraryTab::new(library));
    app
}
fn make_remote_session(audio_only: bool) -> mbv_core::api::SessionInfo {
    mbv_core::api::SessionInfo {
        media_info: mbv_core::api::SessionMediaInfo {
            audio_only,
            ..Default::default()
        },
        ..crate::app::tests::make_session("device", "Emby")
    }
}

#[test]
fn is_audio_item_reads_remote_session_audio_only_flag_when_true() {
    let mut app = crate::app::tests::make_app_stub();
    app.connected_session_id = Some("sess-1".into());
    app.connected_session_state = Some(make_remote_session(true));

    assert!(
        app.is_audio_item(),
        "a connected session's audio_only flag should decide is_audio_item(), \
         not local playlist/cursor state"
    );
}

#[test]
fn is_audio_item_reads_remote_session_audio_only_flag_when_false() {
    let mut app = crate::app::tests::make_app_stub();
    app.connected_session_id = Some("sess-1".into());
    app.connected_session_state = Some(make_remote_session(false));

    assert!(!app.is_audio_item());
}

#[test]
fn is_audio_item_falls_back_to_local_state_when_no_session() {
    let mut app = crate::app::tests::make_app_stub();
    assert!(app.connected_session_id.is_none());
    app.player_tab.set_items(
        vec![crate::app::tests::make_item("song", "Audio")],
        app.player_tab.queue_cursor,
    );
    app.player_tab.queue_cursor = 0;

    assert!(app.is_audio_item());
}

#[test]
fn toggle_mute_falls_back_to_cycle_audio_when_remote_session_connected() {
    // No session-level mute primitive exists (#88), so toggle_mute()
    // must hand off to cycle_audio()'s session-aware branch instead of
    // touching local ui_volume/pre_mute_volume state, which wouldn't
    // reflect a remote session's audio-only playback anyway.
    let mut app = crate::app::tests::make_app_stub();
    app.connected_session_id = Some("sess-1".into());
    app.connected_session_state = Some(make_remote_session(true));
    let ui_volume_before = app.ui_volume;

    app.toggle_mute();

    assert_eq!(
        app.ui_volume, ui_volume_before,
        "remote toggle_mute() must not touch local ui_volume state"
    );
    assert_eq!(
        app.connected_session_state.as_ref().unwrap().audio_index,
        2,
        "toggle_mute() should have delegated to cycle_audio()'s remote branch, \
         which advances the session's audio_index"
    );
}
fn fetch_album_tracks_is_a_no_op_when_already_cached() {
    let mut app = crate::app::tests::make_app_stub();
    app.album_tracks_cache.insert("album-1".into(), Vec::new());

    app.fetch_album_tracks("album-1".into());

    assert!(
        !app.album_tracks_loading.contains("album-1"),
        "a cache hit must return before marking the album as loading \
         (and before spawning a redundant network fetch)"
    );
}

#[test]
fn fetch_album_tracks_is_a_no_op_when_already_loading() {
    let mut app = crate::app::tests::make_app_stub();
    app.album_tracks_loading.insert("album-1".into());

    app.fetch_album_tracks("album-1".into());

    assert!(
        !app.album_tracks_cache.contains_key("album-1"),
        "a duplicate call while a fetch is already in flight must not \
         spawn a second fetch or fabricate a cache entry"
    );
}

#[test]
fn enqueue_selected_rejects_item_from_a_different_route_than_active_queue() {
    let mut app = make_app_stub();
    app.library_routes
        .insert("music".to_string(), "living-room-pc".to_string());
    app.active_route = Some("music".to_string());
    let mut movies_item = make_item("Movies", "CollectionFolder");
    movies_item.id = "lib-movies".to_string();
    app.libs.push(LibraryTab::new(movies_item));
    app.tab = TabSelection::EmbyLibrary(0);

    // `enqueue_selected` was deleted in task 4.3, R1 (item is resolved at
    // the caller); the empty nav stack resolves to the library root, exactly
    // what the context-menu Enqueue arm now does.
    let item = app.libs[0].library.clone();
    app.enqueue_lib_item(0, item);

    // `PlayerTab`/`PlaybackQueue`/`EmbyItem` implement neither
    // `PartialEq` nor `Debug` in this codebase (confirmed: `EmbyItem`
    // derives only `Debug, Clone, Serialize, Deserialize`, and
    // `PlayerTab` derives only `Clone, Default`), so a whole-struct
    // `assert_eq!` against a captured "before" clone will not compile.
    // The established idiom elsewhere in this test module (e.g. the
    // rollback-path tests) is to assert on `.items` directly instead
    // -- here that's simplest as "still empty", since `make_app_stub`
    // starts with an empty queue and a rejected enqueue must leave it
    // that way.
    assert!(app
        .queue_for_scope(app.viewed_queue_scope())
        .emby_items()
        .is_empty());
    assert!(app.status.contains("Can't mix libraries in a routed queue"));
}

#[test]
fn enqueue_route_conflict_allows_matching_route() {
    let mut app = make_app_stub();
    app.active_route = Some("music".to_string());
    assert!(!app.enqueue_route_conflict(Some("music".to_string())));
}

#[test]
fn enqueue_route_conflict_allows_local_queue_local_item() {
    let mut app = make_app_stub();
    assert!(!app.enqueue_route_conflict(None));
}

#[test]
fn enqueue_route_conflict_rejects_mismatched_route() {
    let mut app = make_app_stub();
    app.active_route = Some("music".to_string());
    assert!(app.enqueue_route_conflict(Some("movies".to_string())));
    assert!(app.status.contains("Can't mix libraries in a routed queue"));
}

#[test]
fn enqueue_route_conflict_allows_enqueue_while_attached_to_a_session() {
    // A Sessions-panel attached session (`connected_session_id`) has
    // its own, separate queue-scope rules -- the library-routing
    // invariant must not fire a "Can't mix libraries" toast for a
    // reason unrelated to library routing.
    let mut app = make_app_stub();
    app.connected_session_id = Some("sess-1".to_string());
    assert!(!app.enqueue_route_conflict(Some("music".to_string())));
}

#[test]
fn enqueue_route_conflict_allows_enqueue_while_on_a_non_route_direct_remote() {
    let mut app = make_app_stub();
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(1), 0);
    app.player = mbv_core::player::PlayerProxy::remote(remote, false);
    app.player_rx = remote_rx;
    // active_route stays None: this is a Sessions-panel direct-remote
    // connection, not a library route.
    assert!(!app.enqueue_route_conflict(Some("music".to_string())));
}

#[test]
fn play_item_skips_library_routing_when_attached_to_a_session() {
    let mut app = make_app_stub();
    app.library_routes
        .insert("music".to_string(), "living-room-pc".to_string());
    app.connected_session_id = Some("sess-1".to_string());
    let mut lib_item = make_item("Music", "CollectionFolder");
    lib_item.id = "lib-music".to_string();
    app.libs.push(LibraryTab::new(lib_item));
    let mut item = make_item("Song", "Audio");
    item.id = "song-1".to_string();

    // No DAEMON_ROUTE_CONNECT_OVERRIDE set -- if library routing
    // engaged here it would attempt a real connection and this test
    // would hang/fail rather than reach the assertion below.
    app.play_item(item);

    assert!(app.active_route.is_none());
}

#[test]
fn play_item_skips_library_routing_when_already_direct_remote_via_sessions_panel() {
    // Regression guard for the gap `connected_session_id.is_none()`
    // alone misses: a Sessions-panel "Direct Remote" ctrl-socket
    // upgrade leaves `connected_session_id` as `None` but
    // `self.player.is_remote()` `true` and `active_route` `None`.
    // Library routing must not engage here either -- it would swap
    // `self.player` out from under the active direct-remote
    // connection without ever clearing `direct_remote_label`.
    let mut app = make_app_stub();
    app.library_routes
        .insert("music".to_string(), "living-room-pc".to_string());
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(1), 0);
    let sess = crate::app::tests::make_session("other-mbv", "mbv");
    app.switch_to_direct_remote(
        &sess,
        remote,
        remote_rx,
        &mbv_core::remote_player::DaemonEndpoint::Tcp("127.0.0.1:0".parse().unwrap()),
    );
    assert!(app.player.is_remote());
    assert!(app.active_route.is_none());

    let mut lib_item = make_item("Music", "CollectionFolder");
    lib_item.id = "lib-music".to_string();
    app.libs.push(LibraryTab::new(lib_item));
    let mut item = make_item("Song", "Audio");
    item.id = "song-1".to_string();

    // No DAEMON_ROUTE_CONNECT_OVERRIDE set -- if library routing
    // engaged here it would attempt a real connection and this test
    // would hang/fail rather than reach the assertion below.
    app.play_item(item);

    assert!(app.active_route.is_none());
}
