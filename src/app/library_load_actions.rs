use super::types_playback::HomeContent;
use super::{
    notify_actions::ToastSeverity, App, BrowseLevel, FeedHomeVideoState, HomeLatestSource,
    LibEvent, PanelFocus, PendingQueueAction, TabSelection,
};
use mbv_core::api::EmbyItem;
use mbv_core::playback_queue::QueueItem;
use std::collections::HashMap;

impl App {
    pub(super) fn refresh_lib(&mut self, lib_idx: usize) {
        // Defensive bounds check: the dispatch front door normalizes a stale
        // destination first, but async Service removal can invalidate the
        // matched index between normalization and this call. No-op (never
        // substitute library zero) on a miss. Callers own the panel-focus
        // gate that the pre-parameterization body enforced here.
        if lib_idx >= self.libs.len() {
            return;
        }
        self.start_album_index(lib_idx, true);
        self.clear_saved_library_position(lib_idx);
        if self.is_feed_home_video_group_view(lib_idx) {
            if let Some(state) = self.libs[lib_idx].feed_home_video.as_mut() {
                state.loading = true;
            }
        }
        self.log_feed_home_video_state(lib_idx, "refresh_lib_before_spawn");
        if let Some(lvl) = self.libs[lib_idx].nav_stack.last_mut() {
            lvl.loading = true;
            let parent_id = lvl.parent_id.clone();
            let item_types = lvl.item_types.clone();
            let unplayed_only = lvl.unplayed_only;
            let sort_by = lvl.sort_by.clone();
            let sort_order = lvl.sort_order.clone();
            let loaded_count = lvl.items.len();
            let letter_filter = lvl.letter_filter.clone();
            self.spawn_refresh(
                lib_idx,
                parent_id,
                item_types,
                unplayed_only,
                sort_by,
                sort_order,
                loaded_count,
                letter_filter,
            );
        }
    }

    fn refresh_queue(&mut self) {
        let scope = self.viewed_queue_scope();
        if self.queue_for_scope(scope).total_queue_len() == 0 {
            return;
        }
        let ids: Vec<String> = self
            .queue_for_scope(scope)
            .queue
            .slots()
            .iter()
            .filter_map(|s| s.item.as_emby())
            .map(|i| i.id.clone())
            .collect();
        let Some(client) = self.emby_client() else {
            return;
        };
        let client = client.lock().unwrap();
        if let Ok(fetched) = client.get_items_by_ids(&ids) {
            drop(client);
            let _ = self.merge_refreshed_queue(scope, fetched);
        }
    }

    pub(super) fn refresh_current_view(&mut self) {
        self.force_clear = true;
        match self.effective_panel_focus() {
            // Queue refresh is a refresh of the visible queue only and never
            // indexes the selected browse destination.
            PanelFocus::Queue => self.refresh_queue(),
            PanelFocus::Library => {
                if self.normalize_stale_browse_destination() {
                    return;
                }
                match self.tab {
                    TabSelection::Home => {
                        match self.fetch_home() {
                            Ok(content) => {
                                // The fetch runs synchronously (its App-side
                                // side effects are order-sensitive); the
                                // computed content travels to Model-owned
                                // `home_content` via lib_tx (task 5.3d).
                                let _ = self
                                    .lib_tx
                                    .send(LibEvent::HomeContentRefreshed(Box::new(content)));
                            }
                            Err(e) => {
                                self.flash(format!("Refresh error: {e}"), ToastSeverity::Error)
                            }
                        }
                    }
                    TabSelection::EmbyLibrary(lib_idx) => self.refresh_lib(lib_idx),
                    TabSelection::AudiobookshelfLibrary(index) => {
                        match self.audiobookshelf_kind_at(index) {
                            Some(
                                super::types_audiobookshelf_browse::AudiobookshelfBrowseKind::Book,
                            ) => self.audiobookshelf_book_refresh(),
                            _ => self.audiobookshelf_refresh(),
                        }
                    }
                    TabSelection::Feeds => self.refresh_feeds(),
                }
            }
        }
    }

    pub(super) fn spawn_load_playlists(&mut self) {
        if self.playlists_loading {
            return;
        }
        self.playlists_loading = true;
        let Some(client) = self.emby_snapshot() else {
            self.playlists_loading = false;
            return;
        };
        let tx = self.lib_tx.clone();
        std::thread::spawn(move || match client.get_playlists() {
            Ok(items) => {
                let _ = tx.send(LibEvent::PlaylistsLoaded(items));
            }
            Err(e) => {
                let _ = tx.send(LibEvent::PlaylistsLoadError(format!(
                    "Playlist list failed: {e}"
                )));
            }
        });
    }

    pub(super) fn spawn_rename_playlist(&mut self, playlist_id: String, new_name: String) {
        let Some(client) = self.emby_snapshot() else {
            return;
        };
        let tx = self.lib_tx.clone();
        std::thread::spawn(move || {
            if let Err(e) = client.rename_playlist(&playlist_id, &new_name) {
                let _ = tx.send(LibEvent::Error(format!("Rename failed: {e}")));
            } else {
                let _ = tx.send(LibEvent::PlaylistRenamed { new_name });
            }
            match client.get_playlists() {
                Ok(items) => {
                    let _ = tx.send(LibEvent::PlaylistsLoaded(items));
                }
                Err(e) => {
                    let _ = tx.send(LibEvent::Error(e));
                }
            }
        });
    }

    pub(super) fn spawn_delete_playlist(&mut self, playlist_id: String, name: String) {
        let Some(client) = self.emby_snapshot() else {
            return;
        };
        let tx = self.lib_tx.clone();
        std::thread::spawn(move || {
            if let Err(e) = client.delete_playlist(&playlist_id) {
                let _ = tx.send(LibEvent::Error(format!("Delete failed: {e}")));
            } else {
                let _ = tx.send(LibEvent::PlaylistDeleted { name });
            }
            match client.get_playlists() {
                Ok(items) => {
                    let _ = tx.send(LibEvent::PlaylistsLoaded(items));
                }
                Err(e) => {
                    let _ = tx.send(LibEvent::Error(e));
                }
            }
        });
    }

    pub(super) fn spawn_open_playlist(&mut self, playlist: EmbyItem) {
        if self.playlists_open_loading {
            return;
        }
        self.playlists_open_loading = true;
        self.playlists_open = Some(playlist.clone());
        self.playlists_open_items = Vec::new();
        self.playlists_open_cursor = 0;
        self.playlists_open_scroll = 0;
        let Some(client) = self.emby_snapshot() else {
            self.playlists_open_loading = false;
            return;
        };
        let tx = self.lib_tx.clone();
        let playlist_id = playlist.id.clone();
        std::thread::spawn(move || match client.get_playlist_items(&playlist_id) {
            Ok(items) => {
                let _ = tx.send(LibEvent::PlaylistItemsLoaded { playlist_id, items });
            }
            Err(e) => {
                let _ = tx.send(LibEvent::PlaylistItemsLoadError {
                    playlist_id,
                    error: format!("Playlist load failed: {e}"),
                });
            }
        });
    }

    pub(super) fn open_playlists_panel(&mut self) {
        self.request_sidebar_dismiss(super::SidebarId::Sessions);
        self.close_settings();
        self.request_sidebar_open(super::SidebarId::Playlists);
        if self.playlists.is_empty() && !self.playlists_loading {
            self.spawn_load_playlists();
        }
    }

    pub(super) fn load_and_play_playlist(&mut self, playlist_id: String) {
        let playlist_name = self
            .playlists
            .iter()
            .find(|p| p.id == playlist_id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let Some(client) = self.emby_snapshot() else {
            self.flash("Emby is unavailable".into(), ToastSeverity::Warning);
            return;
        };
        let items = match client.get_playlist_items(&playlist_id) {
            Ok(r) => r,
            Err(e) => {
                self.flash(format!("Playlist load failed: {e}"), ToastSeverity::Error);
                return;
            }
        };
        if items.is_empty() {
            self.flash("Playlist is empty".into(), ToastSeverity::Error);
            return;
        }
        let playable: Vec<EmbyItem> = items.into_iter().filter(|i| !i.is_folder).collect();
        if playable.is_empty() {
            self.flash("No playable items in playlist".into(), ToastSeverity::Error);
            return;
        }
        let action = PendingQueueAction::PlayItems {
            items: playable,
            start_idx: 0,
            source: crate::config::QueueSource::Playlist {
                id: Some(playlist_id),
                name: playlist_name,
            },
        };
        self.replace_queue_or_prompt(action);
        if self.pending_overlay.is_none() {
            self.request_sidebar_dismiss(super::SidebarId::Playlists);
            self.set_panel_focus(PanelFocus::Queue);
        }
    }

    pub(super) fn rebuild_library_tabs_from_views(&mut self, all_views: &[EmbyItem]) {
        // Drain existing libs, preserving nav stacks and scroll pos so that a
        // UserDataChanged websocket refresh (fired when playback starts)
        // doesn't silently reset list scroll position.
        struct SavedLibState {
            nav_stack: Vec<BrowseLevel>,
            feed_home_video: Option<FeedHomeVideoState>,
            library_total: Option<usize>,
        }
        let old_libs: HashMap<String, SavedLibState> = self
            .libs
            .drain(..)
            .map(|mut l| {
                (
                    l.library.id.clone(),
                    SavedLibState {
                        nav_stack: std::mem::take(&mut l.nav_stack),
                        feed_home_video: l.feed_home_video,
                        library_total: l.library_total,
                    },
                )
            })
            .collect();

        for view in all_views.iter().filter(|v| {
            v.collection_type != "playlists"
                && !self.hidden_libraries.contains(&v.name.to_lowercase())
        }) {
            let saved = old_libs.get(&view.id);
            let stack = saved
                .map(|s| {
                    s.nav_stack
                        .iter()
                        .map(|lvl| BrowseLevel {
                            parent_id: lvl.parent_id.clone(),
                            title: lvl.title.clone(),
                            items: lvl.items.clone(),
                            total_count: lvl.total_count,
                            item_types: lvl.item_types.clone(),
                            unplayed_only: lvl.unplayed_only,
                            sort_by: lvl.sort_by.clone(),
                            sort_order: lvl.sort_order.clone(),
                            loading: false,
                            resting: lvl.resting(),
                            all_items: lvl.all_items.clone(),
                            letter_filter: lvl.letter_filter.clone(),
                            music_grouping: lvl.music_grouping.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let feed_home_video = saved.and_then(|s| s.feed_home_video.clone());
            let library_total = saved.and_then(|s| s.library_total);
            self.libs.push(super::LibraryTab {
                nav_stack: stack,
                feed_home_video,
                library_total,
                ..super::LibraryTab::new(view.clone())
            });
        }
    }

    /// Compute the full Home content snapshot (task 5.3d): the Emby-derived
    /// portion (Continue Watching, library tabs rebuild, Emby `latest` pills)
    /// is built only when an Emby Service is configured and connected.
    /// Without one it is skipped -- not an error -- so Home still populates
    /// from whatever local Sources exist (#543 Part 1). Returns the
    /// computed `HomeContent` instead of writing deleted `App.home`; the
    /// shell assigns it to `Model.home_content` (directly for shell-side
    /// callers, via `LibEvent::HomeContentRefreshed` for App-internal ones)
    /// and preserves the Continue Watching column cursor at the assignment.
    pub(super) fn fetch_home(&mut self) -> Result<HomeContent, String> {
        let mut emby_fetched = false;
        let (continue_items, all_views, user_views) = if let Some(client) = self.emby_client() {
            emby_fetched = true;
            let client = client.lock().unwrap();
            let views = match client.get_views_classified() {
                Ok(views) => views,
                Err(error) => {
                    drop(client);
                    self.handle_emby_runtime_failure(error.clone());
                    return Err(error.to_string());
                }
            };
            (
                client.get_continue_watching(20).unwrap_or_default(),
                views,
                client.get_user_views().unwrap_or_default(),
            )
        } else {
            (Vec::new(), Vec::new(), Vec::new())
        };

        // Library tabs are Emby-modeled state; only rebuild them from the
        // freshly fetched views when Emby was actually reachable, so a broken
        // or absent Emby does not clear existing library tabs.
        if emby_fetched {
            self.rebuild_library_tabs_from_views(&all_views);
            for lib_idx in 0..self.libs.len() {
                self.start_album_index(lib_idx, false);
            }
        }

        // Merge, not replace: the Emby `latest` pills are removed and
        // reinserted at their previous positions, leaving any entries from
        // other providers untouched. This is what lets `fetch_home()` run
        // unconditionally (even before Emby connects) without clearing other
        // providers' Home data whenever Emby is the writer. All three
        // provider portions (Emby, Audiobookshelf shelf cache, Feeds tab)
        // are rebuilt into the local `latest` here; the per-pill cursor
        // tuples are the preserved-but-vestigial legacy cursor fields (the
        // mounted `HomeComponent` owns the real flat cursors).
        let mut latest: Vec<(String, HomeLatestSource, Vec<QueueItem>, usize)> = Vec::new();
        let mut emby_sections: Vec<(String, HomeLatestSource, Vec<QueueItem>)> = Vec::new();
        if let Some(client) = self.emby_client() {
            let client = client.lock().unwrap();
            for v in user_views.iter().filter(|v| {
                let lower = v.name.to_lowercase();
                v.collection_type != "playlists"
                    && !self.hidden_latest.contains(&lower)
                    && !self.hidden_libraries.contains(&lower)
            }) {
                let items = if v.collection_type == "tvshows" {
                    client.get_latest_episodes(&v.id, 30).unwrap_or_default()
                } else {
                    client.get_latest(&v.id, 30).unwrap_or_default()
                };
                emby_sections.push((
                    v.name.clone(),
                    HomeLatestSource::Emby(v.id.clone()),
                    items
                        .into_iter()
                        .map(|item| QueueItem::Emby(Box::new(item)))
                        .collect(),
                ));
            }
        }
        merge_home_sections(&mut latest, emby_sections, |source| {
            matches!(source, HomeLatestSource::Emby(_))
        });
        // The Audiobookshelf portion is rebuilt from the async shelf cache
        // (never a network fetch here), re-applying `hidden_latest`.
        merge_home_sections(
            &mut latest,
            self.audiobookshelf_latest_sections(),
            |source| matches!(source, HomeLatestSource::Audiobookshelf(_)),
        );
        // The Feeds portion is rebuilt from the Feeds tab's combined entries
        // (never a network fetch here), re-applying `hidden_latest`.
        if let Some(feed_section) = self.feeds_latest_section() {
            merge_home_sections(&mut latest, vec![feed_section], |source| {
                matches!(source, HomeLatestSource::Feeds)
            });
        }

        Ok(HomeContent {
            continue_items,
            continue_cursor: 0,
            latest,
            loading: false,
        })
    }

    /// The Audiobookshelf Latest-pill sections rebuildable from
    /// `audiobookshelf_shelf_cache`, one per cached **podcast** library in
    /// `audiobookshelf_libraries` order (book libraries get no pill in this
    /// change, per the spec), honoring `hidden_latest`/`hidden_libraries` by
    /// library name. A library with no cached entries still yields a pill
    /// (empty pills are not selectable and render nothing), matching an empty
    /// Emby Latest section.
    pub(super) fn audiobookshelf_latest_sections(
        &self,
    ) -> Vec<(String, HomeLatestSource, Vec<QueueItem>)> {
        self.audiobookshelf_libraries
            .iter()
            .filter(|library| library.media_type != "book")
            .filter(|library| {
                let lower = library.name.to_lowercase();
                !self.hidden_latest.contains(&lower) && !self.hidden_libraries.contains(&lower)
            })
            .map(|library| {
                (
                    library.name.clone(),
                    HomeLatestSource::Audiobookshelf(library.id.clone()),
                    self.audiobookshelf_shelf_cache
                        .get(&library.id)
                        .cloned()
                        .unwrap_or_default(),
                )
            })
            .collect()
    }

    /// The single Feeds Latest-pill section, built from the Feeds tab's
    /// combined `all_entries` (newest-first "All" group). The pill exists
    /// only when feed subscriptions are configured (mirroring the
    /// Audiobookshelf pill, which exists only per library), and honors
    /// `hidden_latest` via the literal `"feeds"` pseudo-name. Consumed by
    /// `fetch_home()` and by the shell's feed-drain seam, which merges the
    /// freshly computed section into Model-owned `latest` (task 5.3d).
    pub(super) fn feeds_latest_section(
        &self,
    ) -> Option<(String, HomeLatestSource, Vec<QueueItem>)> {
        if !self.has_feeds_subscriptions() {
            return None;
        }
        if self
            .hidden_latest
            .iter()
            .any(|hidden| hidden.eq_ignore_ascii_case("feeds"))
        {
            return None;
        }
        let items = self.feed_latest_items();
        Some(("Feeds".into(), HomeLatestSource::Feeds, items))
    }

    /// The `Newest Episodes` shelf's entries as queue-able items, or an empty
    /// list when the shelf is absent (only that shelf feeds Home).
    pub(super) fn newest_episodes_items(
        shelves: Vec<mbv_core::audiobookshelf::AudiobookshelfShelf>,
    ) -> Vec<QueueItem> {
        shelves
            .into_iter()
            .find(|shelf| shelf.label.eq_ignore_ascii_case("Newest episodes"))
            .map(|shelf| {
                shelf
                    .entries
                    .into_iter()
                    .filter_map(|entry| match entry {
                        mbv_core::audiobookshelf::AudiobookshelfShelfEntry::Episode(item) => {
                            Some(QueueItem::Audiobookshelf(item))
                        }
                        mbv_core::audiobookshelf::AudiobookshelfShelfEntry::Show(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Position- and cursor-preserving splice for `HomeContent.latest`: drops every
/// section whose source matches `kind`, then reinserts `sections` at the
/// previous positions, restoring each section's prior cursor clamped to its
/// item count. Each Home writer (Emby in `fetch_home`, the Audiobookshelf
/// shelf cache) calls this with its own kind, so it only ever touches its own
/// entries and leaves other providers' pills untouched.
pub(super) fn merge_home_sections(
    latest: &mut Vec<(String, HomeLatestSource, Vec<QueueItem>, usize)>,
    sections: Vec<(String, HomeLatestSource, Vec<QueueItem>)>,
    kind: impl Fn(&HomeLatestSource) -> bool,
) {
    let old_positions: Vec<usize> = latest
        .iter()
        .enumerate()
        .filter(|(_, (_, source, _, _))| kind(source))
        .map(|(index, _)| index)
        .collect();
    let old_cursors: HashMap<HomeLatestSource, usize> = latest
        .iter()
        .filter_map(|(_, source, _, cursor)| {
            if kind(source) {
                Some((source.clone(), *cursor))
            } else {
                None
            }
        })
        .collect();
    let mut merged: Vec<(String, HomeLatestSource, Vec<QueueItem>, usize)> = std::mem::take(latest)
        .into_iter()
        .filter(|(_, source, _, _)| !kind(source))
        .collect();
    for (inserted, (title, source, items)) in sections.into_iter().enumerate() {
        let cursor = old_cursors
            .get(&source)
            .copied()
            .unwrap_or(0)
            .min(items.len().saturating_sub(1));
        let insert_at = old_positions
            .get(inserted)
            .copied()
            .unwrap_or(merged.len())
            .min(merged.len());
        merged.insert(insert_at, (title, source, items, cursor));
    }
    // Canonical pill order across providers regardless of arrival order:
    // Emby views, then Audiobookshelf podcast libraries, then Feeds. The
    // merge above preserves cursor positions, but async completion order
    // (Feeds loading before an ABS shelf fetch, Emby bootstrapping last)
    // would otherwise let sections observe arrival order instead. Stable so
    // same-source sections keep their existing relative order.
    merged.sort_by_key(|(_, source, _, _)| home_latest_source_rank(source));
    *latest = merged;
}

/// Pill ordering rank: Emby (0) before Audiobookshelf (1) before Feeds (2).
pub(super) fn home_latest_source_rank(source: &HomeLatestSource) -> u8 {
    match source {
        HomeLatestSource::Emby(_) => 0,
        HomeLatestSource::Audiobookshelf(_) => 1,
        HomeLatestSource::Feeds => 2,
    }
}
