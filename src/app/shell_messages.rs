use super::*;
use crate::app::components::MusicWorkspaceComponent;
use std::time::Instant;

impl Model {
    pub(crate) fn handle_terminal_message(
        &mut self,
        msg: Msg,
        music_resize: &mut bool,
        tv_resize: &mut bool,
    ) -> bool {
        let mut quit = false;
        match msg {
            Msg::TerminalEvent(event) => {
                apply_terminal_observer(self, event, music_resize, tv_resize)
            }
            Msg::Shell(request) => match request {
                ShellRequest::MusicAlbumActivate => {
                    if self.app.tab.emby_library_index().is_some() {
                        // Outcome 3 reader: get the album from the component, which
                        // owns the selection cursor.
                        let album = self
                            .music_workspace_id
                            .as_ref()
                            .and_then(|id| self.application.get_component(id))
                            .and_then(|comp| {
                                comp.as_any().downcast_ref::<MusicWorkspaceComponent>()
                            })
                            .and_then(|comp| comp.selected_item());
                        if let Some(album) = album {
                            if !self.app.is_right_panel_wide() {
                                self.app.open_album_selection_modal(&album);
                            }
                        }
                    }
                    self.push_music_workspace_content();
                }
                ShellRequest::MusicAlbumCursor { target, kind } => {
                    // Click-to-focus: a pointer-driven album-cursor move pulls
                    // panel focus to the Library. Keyboard moves only reach
                    // this arm while the Library is already focused, so this is
                    // idempotent there.
                    self.app.set_panel_focus(crate::app::PanelFocus::Library);
                    if let Some(lib_idx) = self.app.tab.emby_library_index() {
                        match kind {
                            AlbumCursorKind::Move => {
                                let idle = self.app.list_image_fetches_allowed();
                                let now = Instant::now();
                                self.app.last_nav_at = now;
                                self.app.mark_library_navigation(now);
                                if self.app.move_music_group_display_cursor(lib_idx, target) {
                                    self.app.save_default_library_position(lib_idx);
                                    if idle {
                                        self.app.maybe_fetch_next_page(lib_idx, target);
                                    }
                                }
                            }
                            AlbumCursorKind::Jump => {
                                if self.app.jump_music_group_display_cursor(lib_idx, target) {
                                    self.app.save_default_library_position(lib_idx);
                                    self.app.maybe_fetch_next_page(lib_idx, target);
                                }
                            }
                            AlbumCursorKind::Page => {
                                self.app.page_grouped_album_cursor(lib_idx, target);
                            }
                        }
                    }
                    self.push_music_workspace_content();
                }
                // Inline album-track activation/enqueue/context-menu
                // target resolution: the component owns the cursor,
                // the shell resolves it to the cached track and runs
                // the App effect (task 5.3d, Album track focus).
                ShellRequest::MusicTrackActivate => {
                    if self.app.tab.emby_library_index().is_some() {
                        if let Some((album_id, track)) = self.focused_music_track() {
                            self.app.play_album_track(&album_id, &track);
                        }
                    }
                    self.push_music_workspace_content();
                }
                ShellRequest::MusicTrackEnqueue => {
                    if let Some(lib_idx) = self.app.tab.emby_library_index() {
                        if let Some((_, track)) = self.focused_music_track() {
                            self.app.enqueue_lib_item(lib_idx, track);
                        }
                    }
                    self.push_music_workspace_content();
                }
                ShellRequest::MusicTrackContextMenu => {
                    if let Some((_, track)) = self
                        .app
                        .tab
                        .emby_library_index()
                        .and_then(|_| self.focused_music_track())
                    {
                        self.app.open_context_menu_for(track);
                    }
                    self.push_music_workspace_content();
                }
                ShellRequest::MusicAlbumContextMenu { anchor } => {
                    self.app.set_panel_focus(crate::app::PanelFocus::Library);
                    let album = self
                        .music_workspace_id
                        .as_ref()
                        .and_then(|id| self.application.get_component(id))
                        .and_then(|comp| comp.as_any().downcast_ref::<MusicWorkspaceComponent>())
                        .and_then(|comp| comp.selected_item());
                    if let Some(album) = album {
                        self.app.open_context_menu_for_at(album, anchor.0, anchor.1);
                    }
                    self.push_music_workspace_content();
                }
                ShellRequest::MusicGroupSwitch { delta } => {
                    if let Some(lib_idx) = self.app.tab.emby_library_index() {
                        self.app.switch_music_group(lib_idx, delta);
                    }
                    // A group switch replaces the album level; re-anchor the
                    // workspace cursor at this nav event (mirrors the pill
                    // click path in `ShellRequest::BrowserPillClick`).
                    self.music_workspace_reanchor = true;
                    self.push_music_workspace_content();
                }
                // Help overlay cross-boundary requests (design D4).
                ShellRequest::Quit => quit = true,
                ShellRequest::DismissHelp => self.umount_help(),
                ShellRequest::OpenSettings => {
                    self.umount_help();
                    self.mount_sidebar(super::super::SidebarId::Settings);
                }
                ShellRequest::OpenSessions => {
                    self.umount_help();
                    self.mount_sidebar(super::super::SidebarId::Sessions);
                }
                ShellRequest::OpenPlaylists => {
                    self.umount_help();
                    self.mount_sidebar(super::super::SidebarId::Playlists);
                    self.app.open_playlists_panel();
                }
                ShellRequest::ConfirmIntent(intent) => {
                    self.handle_confirm_intent(intent);
                    // Confirmations rewrite Home content/focus; re-project (5.3d).
                    self.push_home_content();
                    // Emby browser content may have changed (5.3d.15/M2).
                    self.push_emby_browser_content();
                }
                ShellRequest::DaemonLostIntent(intent) => {
                    if self.handle_daemon_lost_intent(intent) {
                        quit = true;
                    }
                }
                ShellRequest::RemoteReanchorIntent(intent) => {
                    self.handle_remote_reanchor_intent(intent);
                }
                // Context menu: the shell owns cursor navigation and
                // action execution; the component owns key interpretation
                // (task 5.1).
                ShellRequest::ContextMenuIntent(intent) => {
                    self.handle_context_menu_intent(intent);
                    // Enter executes the action, which can refetch Home; re-project (5.3d).
                    self.push_home_content();
                    // Emby browser content may have changed (5.3d.15/M2).
                    self.push_emby_browser_content();
                }
                ShellRequest::ContextMenuSelect(idx) => {
                    self.handle_context_menu_select(idx);
                    // A selected action can refetch Home; re-project (5.3d).
                    self.push_home_content();
                    // Emby browser content may have changed (5.3d.15/M2).
                    self.push_emby_browser_content();
                }
                ShellRequest::ContextMenuDismiss => {
                    self.app.pending_overlay =
                        Some(super::super::types_overlay::OverlayRequest::DismissContextMenu);
                }
                // Search sidebar: dismiss (Esc/Backspace-on-empty).
                // The component owns the state; the shell unmounts it.
                ShellRequest::DismissSearch => {
                    self.dismiss_sidebar(super::super::SidebarId::Search);
                }
                // Search sidebar: activate result (Enter). The
                // component owns the cursor/results; the shell owns
                // the library tabs and navigation spawn (task 3.2).
                ShellRequest::SearchActivate { id, item_type } => {
                    self.app.activate_search_result(id, item_type);
                }
                ShellRequest::OpenInlineSearch => {
                    self.open_inline_search();
                }
                ShellRequest::InlineSearchActivate { id, item_type } => {
                    self.activate_inline_search_item(id, item_type);
                }
                ShellRequest::DismissSessions => {
                    self.dismiss_sidebar(super::super::SidebarId::Sessions);
                }
                ShellRequest::RefreshSessions => {
                    self.app.spawn_sessions_load();
                    self.app.spawn_cast_discovery();
                }
                ShellRequest::SelectSession(index) => {
                    if let Some(target) = self.app.panel_targets.get(index).cloned() {
                        self.app.select_panel_target(target);
                    }
                }
                ShellRequest::DetachSessions => {
                    let cast_attached = self.app.is_cast_attached();
                    // Skip disconnect_remote's "No session selected" toast when
                    // only a cast target is attached; the cast detach below is
                    // the real action in that case.
                    if self.app.can_disconnect_remote() || !cast_attached {
                        self.app.disconnect_remote();
                    }
                    if cast_attached {
                        self.app.detach_cast();
                        self.app.flash(
                            "Detached from cast target".to_string(),
                            ToastSeverity::Success,
                        );
                    }
                    self.dismiss_sidebar(super::super::SidebarId::Sessions);
                }
                ShellRequest::RefreshFeeds => {
                    self.app.refresh_feeds();
                }
                ShellRequest::FeedsPlay(entry) => {
                    if let Some(entry) = entry {
                        self.app.play_feed_entry(entry);
                    } else {
                        self.app
                            .flash("No feed entry selected".into(), ToastSeverity::Neutral);
                    }
                }
                ShellRequest::FeedsRowClick => {
                    // A Feeds list row the user clicked: the component already
                    // resolved and selected the row; the shell pulls panel
                    // focus to the Library (task 4.5, mirrors `HomeRowClick`).
                    self.app.set_panel_focus(crate::app::PanelFocus::Library);
                    self.sync_feeds();
                }
                ShellRequest::FeedsEnqueue(entry) => {
                    if let Some(entry) = entry {
                        self.app.enqueue_feed_entry(entry);
                    } else {
                        self.app
                            .flash("No feed entry selected".into(), ToastSeverity::Neutral);
                    }
                }
                request @ ShellRequest::DismissSelectionModal
                | request @ ShellRequest::SelectionModalFilterSelected
                | request @ ShellRequest::SelectionModalActivate(_) => {
                    self.handle_selection_modal_request(request);
                    // Selection-modal changes to the ABS episode filter
                    // must reach the mounted component (5.3d.11 U6).
                    self.push_audiobookshelf_podcast_content();
                }
                ShellRequest::MultiselectCommit { .. } => {
                    self.handle_multiselect_commit();
                    // Hiding libraries/pills refetches Home inside the commit; re-project (5.3d).
                    self.push_home_content();
                    // Emby browser content may have changed (5.3d.15/M2).
                    self.push_emby_browser_content();
                }
                request @ ShellRequest::LibraryRoutesEnter
                | request @ ShellRequest::LibraryRoutesEsc => {
                    self.handle_library_routes_request(request);
                }
                ShellRequest::FeedsManageIntent(intent) => {
                    self.handle_feeds_manage_intent(intent);
                }
                ShellRequest::AudiobookshelfPodcastEpisodeIntent(intent) => {
                    // Typed podcast episode action intent (task 5.3d.7).
                    // The shell resolves the episode-selection and
                    // wide/narrow conditions from App state/layout and
                    // runs the existing App play/enter/modal/enqueue
                    // effects (D17); re-project after the effect.
                    self.handle_audiobookshelf_podcast_episode_intent(intent);
                    self.push_audiobookshelf_podcast_content();
                }
                ShellRequest::AudiobookshelfPodcastShowMove { index } => {
                    // Resolved podcast show-list cursor
                    // (split-audiobookshelf-cursor-ownership D1). The
                    // component already resolved its own movement and
                    // carries the landed index; apply it directly through
                    // the index-taking entry point (clamp + `state.select`
                    // + detail-fetch), never recomputing from a delta. The
                    // episode-selection guard lives only on the component
                    // now (D2).
                    // Click-to-focus (task 4.5): a mouse-driven (or already
                    // focused keyboard) show move pulls panel focus to the
                    // Library.
                    self.app.set_panel_focus(crate::app::PanelFocus::Library);
                    self.app.select_audiobookshelf_show(index);
                    // The component owns the painted cursor; persist the
                    // active tab's slot once after the movement lands so
                    // the saved position tracks the moved cursor (B3).
                    if let Some(index) = self.app.tab.audiobookshelf_index() {
                        self.app.save_audiobookshelf_position(index);
                    }
                    // `select_audiobookshelf_show` rewrote the active browse
                    // state (cursor/selection); re-project (5.3d.11 U6).
                    self.push_audiobookshelf_podcast_content();
                }
                request @ (ShellRequest::AudiobookshelfBookMove(_)
                | ShellRequest::AudiobookshelfBookIntent(_)) => {
                    self.handle_audiobookshelf_book_request(request);
                }
                // Browser (generic Emby) mouse gestures are recognized by
                // `BrowserComponent`'s private `MouseGestureState` (ADR 0024,
                // design.md D3/D4): it owns the wheel throttle and resolves the
                // row target itself, so the shell only applies the effect.
                ShellRequest::BrowserScroll { offset } => {
                    if let Some(lib_idx) = self.app.tab.emby_library_index() {
                        if self.app.is_feed_home_video_group_view(lib_idx) {
                            if let Some(state) = self.app.libs[lib_idx].feed_home_video.as_mut() {
                                state.video_scroll = offset;
                                self.app.save_default_library_position(lib_idx);
                            }
                        } else {
                            self.app.persist_library_scroll(lib_idx, offset);
                        }
                    }
                }
                // Browser selected-item typed effects (task 5.3d, Emby
                // browser effect decoupling): the component reports the
                // explicit `EmbyItem` target; the shell forwards it
                // straight to the App effect (no App-cursor re-read).
                request @ (ShellRequest::BrowserActivate { .. }
                | ShellRequest::BrowserPlay { .. }
                | ShellRequest::BrowserEnqueue { .. }
                | ShellRequest::BrowserToggleWatched { .. }
                | ShellRequest::BrowserContextMenu { .. }
                | ShellRequest::EmbyLibraryContextMenu { .. }
                | ShellRequest::BrowserShuffle { .. }
                | ShellRequest::BrowserRefresh
                | ShellRequest::BrowserRescan
                | ShellRequest::BrowserBack
                | ShellRequest::BrowserCycleLetterPill { .. }
                | ShellRequest::BrowserCycleGroup { .. }
                | ShellRequest::EmbyLibraryPlay { .. }
                | ShellRequest::EmbyLibraryEnqueue { .. }
                | ShellRequest::EmbyLibraryToggleWatched { .. }
                | ShellRequest::EmbyLibraryShuffle { .. }
                | ShellRequest::EmbyLibraryRefresh
                | ShellRequest::EmbyLibraryRescan) => {
                    // Keep the existing Browser projection for generic Emby
                    // libraries, and also refresh the separately-mounted
                    // Music/TV workspace when one issued an EmbyLibrary*
                    // effect request.
                    let reproject_workspace = matches!(
                        &request,
                        ShellRequest::EmbyLibraryPlay { .. }
                            | ShellRequest::EmbyLibraryEnqueue { .. }
                            | ShellRequest::EmbyLibraryToggleWatched { .. }
                            | ShellRequest::EmbyLibraryShuffle { .. }
                            | ShellRequest::EmbyLibraryRefresh
                            | ShellRequest::EmbyLibraryRescan
                    );
                    self.handle_browser_request(request);
                    // Browser navigation/effects change library content; re-project (5.3d.15/M2).
                    self.push_emby_browser_content();
                    if reproject_workspace {
                        self.push_music_workspace_content();
                        self.push_tv_workspace_content();
                    }
                }
                // Pure cursor movement: the component already resolved its own
                // index, so apply the App-side nav effects but skip the content
                // re-projection the effect requests above need.
                request @ ShellRequest::BrowserCursorIndex { .. } => {
                    self.handle_browser_request(request);
                }
                ShellRequest::BrowserPillClick { target } => {
                    if let Some(lib_idx) = self.app.tab.emby_library_index() {
                        self.app.handle_mouse_selector_click_emby(lib_idx, target);
                    }
                    // A music-group pill switch replaces the album level;
                    // re-anchor the workspace cursor at this nav event.
                    self.music_workspace_reanchor = true;
                    self.push_emby_browser_content();
                }
                ShellRequest::BrowserRowClick { target } => {
                    if let Some(lib_idx) = self.app.tab.emby_library_index() {
                        self.app.handle_mouse_single_click_emby(lib_idx, target);
                    }
                    self.push_emby_browser_content();
                }
                ShellRequest::BrowserRowActivate { target } => {
                    if let Some(lib_idx) = self.app.tab.emby_library_index() {
                        self.app.handle_mouse_double_click_emby(lib_idx, target);
                    }
                    self.push_emby_browser_content();
                }
                ShellRequest::BrowserRowContextMenu { target, anchor } => {
                    if let Some(lib_idx) = self.app.tab.emby_library_index() {
                        self.app
                            .handle_mouse_right_click_emby(lib_idx, target, anchor.0, anchor.1);
                    }
                    self.push_emby_browser_content();
                }
                // Home (cross-Service) mouse gestures are recognized by
                // `HomeComponent`'s private `MouseGestureState` (ADR 0024,
                // design.md D3/D4): it owns the double-click window and wheel
                // throttle and resolves the row target itself. The wheel
                // effect still runs through `handle_home_scroll` (its App gate
                // plus the Continue Watching `cw_move_cursor` quirk) until
                // task 4.3.
                ShellRequest::HomeScroll { delta } => {
                    self.handle_home_scroll(delta);
                }
                ShellRequest::HomeRowClick => {
                    self.app.set_panel_focus(crate::app::PanelFocus::Library);
                    self.push_home_content();
                }
                ShellRequest::HomeRowActivate { target } => {
                    self.app.set_panel_focus(crate::app::PanelFocus::Library);
                    if let Some((item, from_cw)) = self.home_flat_target(target) {
                        self.app.home_play_target(item, from_cw);
                    }
                    self.push_home_content();
                }
                ShellRequest::HomeRowContextMenu { anchor } => {
                    self.app.set_panel_focus(crate::app::PanelFocus::Library);
                    self.app.open_context_menu_at(
                        anchor.0,
                        anchor.1,
                        self.home_continue_watching_selected(),
                        self.home_cw_item(),
                    );
                    self.push_home_content();
                }
                ShellRequest::HomePillClick { target } => {
                    self.select_home_section_from_component(target);
                }
                // Home typed effects (task 5.3d, Home typed-effect
                // prep): `HomeComponent` owns the cursor and reports the
                // flat target index it resolved; the shell forwards it
                // straight to the `App` effect so the requested target
                // is acted on directly (no App-owned flat cursor remains).
                request @ (ShellRequest::HomePlay(_)
                | ShellRequest::HomeEnqueue(_)
                | ShellRequest::HomeContextMenu { .. }
                | ShellRequest::HomeDelete(_)
                | ShellRequest::HomeToggleWatched
                | ShellRequest::HomeSectionSelected(_)) => self.handle_home_request(request),
                // Queue mouse gestures are recognized by `QueueComponent`'s
                // private `MouseGestureState` (ADR 0024, design.md D3/D4): it
                // owns the double-click window and wheel throttle, switches
                // its own scope, and resolves the slot itself. The shell only
                // applies the cross-boundary effect.
                ShellRequest::QueueScroll { delta } => {
                    self.app.handle_mouse_scroll_queue(delta);
                }
                ShellRequest::QueueScopeClick { scope } => {
                    self.app.handle_mouse_selector_click_queue(scope);
                    self.queue_click_reproject();
                }
                ShellRequest::QueueRowClick { slot_id } => {
                    self.app.handle_mouse_single_click_queue(slot_id);
                    self.queue_click_reproject();
                }
                ShellRequest::QueueRowActivate { slot_id } => {
                    self.app.handle_mouse_double_click_queue(slot_id);
                    self.queue_click_reproject();
                }
                ShellRequest::QueueContextMenu { slot_id } => {
                    self.app.handle_keyboard_context_menu_queue(
                        slot_id,
                        self.home_continue_watching_selected(),
                    );
                    self.queue_click_reproject();
                }
                ShellRequest::QueueRowContextMenu { slot_id, anchor } => {
                    // The authoritative Continue-Watching-selected fact is
                    // resolved here (Model boundary) and passed into the App
                    // builder, so the odd queue->Home coupling reflects the
                    // mounted Home component's section (task 5.3d).
                    self.app.handle_mouse_right_click_queue(
                        slot_id,
                        anchor.0,
                        anchor.1,
                        self.home_continue_watching_selected(),
                    );
                    self.queue_click_reproject();
                }
                // TV keyboard requests are resolved by the mounted
                // workspace component. Cursor and pane movement remain
                // component-local; the shell handles only cross-boundary
                // effects such as activation, back, and letter pills.
                request @ (ShellRequest::TvMoveRows { .. }
                | ShellRequest::TvMoveColumn { .. }
                | ShellRequest::TvJumpCursor { .. }
                | ShellRequest::TvActivate { .. }
                | ShellRequest::TvEpisodeActivate
                | ShellRequest::TvBack
                | ShellRequest::TvCycleLetterPill { .. }
                | ShellRequest::TvEpisodeMove { .. }
                | ShellRequest::TvSeasonMove { .. }) => self.handle_tv_request(request),
                // TV workspace mouse gestures are recognized by
                // `TvWorkspaceComponent`'s private `MouseGestureState` (ADR
                // 0024, design.md D3/D4): it owns the double-click window and
                // wheel throttle, resolves the pane + hit itself, and moves
                // its own pane/cursor before emitting. The shell only applies
                // the cross-boundary effect.
                ShellRequest::TvScroll { .. } => {
                    self.push_tv_workspace_content();
                }
                ShellRequest::TvHitClick { hit } => {
                    if let Some(lib_idx) = self.app.tab.emby_library_index() {
                        self.app.handle_mouse_single_click_tv(lib_idx, hit);
                    }
                    self.push_tv_workspace_content();
                }
                ShellRequest::TvHitDoubleClick { hit } => {
                    if let Some(lib_idx) = self.app.tab.emby_library_index() {
                        self.app.handle_mouse_double_click_tv(lib_idx, hit);
                    }
                    self.push_tv_workspace_content();
                }
                ShellRequest::TvHitContextMenu { hit, anchor } => {
                    if let Some(lib_idx) = self.app.tab.emby_library_index() {
                        self.app
                            .handle_mouse_right_click_tv(lib_idx, hit, anchor.0, anchor.1);
                    }
                    self.push_tv_workspace_content();
                }

                request @ (ShellRequest::PlaylistsBack
                | ShellRequest::PlaylistsOpen(_)
                | ShellRequest::PlaylistsActivate { .. }
                | ShellRequest::PlaylistsRename(_)
                | ShellRequest::PlaylistsDelete(_)
                | ShellRequest::PlaylistsRefresh
                | ShellRequest::DismissPlaylists) => self.handle_playlists_request(request),
                ShellRequest::SettingsIntent(intent) => {
                    if self.handle_settings_intent(intent) {
                        quit = true;
                    }
                }
                ShellRequest::SavePlaylistIntent(intent) => {
                    self.handle_save_playlist_intent(intent);
                }
                ShellRequest::QueueIntent(intent) => {
                    self.handle_queue_intent(intent);
                }
                // Component owns episode_selection/episode_filter; mutated locally in
                // AudiobookshelfPodcastComponent::handle_key before the request is emitted, and
                // handle_audiobookshelf_podcast_episode_intent resolves the target from the
                // component, not App state (commit 0227d748, migrate-tui-to-tuirealm task
                // 5.3d.11 U2). No shell effect remains.
                ShellRequest::AudiobookshelfPodcastEpisodeTransition(_) => {}
                // Emitted only from SettingsComponent::handle_mouse (settings.rs:318); the
                // keyboard dismiss is SettingsIntent::Back. Mouse-only, inert under D16
                // (migrate-tui-to-tuirealm design D16, #628).
                ShellRequest::DismissSettings => {}
                // Produced at shell_overlays_modals.rs:164 and consumed synchronously by
                // handle_selection_modal_request (shell_overlays_menus.rs:232); it never
                // arrives as a top-level Msg here.
                ShellRequest::SelectionModalRefresh => {}
            },
            Msg::Queue(request) => {
                self.handle_queue_request(request);
            }
            Msg::Playback(request) => {
                self.handle_playback_request(request);
            }
            Msg::Service(request) => {
                if self.handle_service_request(request) {
                    quit = true;
                }
            }
        }
        quit
    }

    /// Re-project after a Queue click: the click moves panel focus to the
    /// Queue panel (re-project the Home focus flag) and may mutate Emby
    /// browser content (5.3d.15/M2).
    fn queue_click_reproject(&mut self) {
        self.push_home_content();
        self.push_emby_browser_content();
    }
}
