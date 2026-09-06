use super::notify_actions::ToastSeverity;
use super::types_audiobookshelf_browse::{AudiobookshelfEpisodeFilter, BookRow};
use super::App;
use mbv_core::api::TICKS_PER_SECOND;
use mbv_core::playback_queue::{AudiobookshelfBookQueueItem, AudiobookshelfQueueItem, QueueItem};

impl App {
    /// Resolve the browse kind for Audiobookshelf library `index` from its
    /// `media_type`, once. This is the single resolution point the
    /// service-browse-dispatch spec requires: downstream renderers, input
    /// handlers, refresh, and position restore for this destination branch on
    /// the returned kind instead of re-reading `media_type` per action.
    ///
    /// `None` when the index is stale (library removed/replaced); the caller
    /// must stop the triggering operation, matching `normalize_stale_*`.
    pub(super) fn audiobookshelf_kind_at(
        &self,
        index: usize,
    ) -> Option<super::types_audiobookshelf_browse::AudiobookshelfBrowseKind> {
        self.audiobookshelf_libraries.get(index).map(|library| {
            super::types_audiobookshelf_browse::AudiobookshelfBrowseKind::from_media_type(
                &library.media_type,
            )
        })
    }

    /// Fetches the selected podcast show's downloaded episodes.
    pub(super) fn start_audiobookshelf_detail(&mut self, library_item_id: String) {
        let Some(index) = self.tab.audiobookshelf_index() else {
            return;
        };
        let Some(state) = self.audiobookshelf_browse.get_mut(index) else {
            return;
        };
        if let Some(cached) = state.detail_cache.get(&library_item_id).cloned() {
            state.episodes = Some(cached);
            state.detail_loading = false;
            return;
        }
        if state.episodes.is_some() || state.detail_loading {
            return;
        }
        state.detail_loading = true;
        let config_snapshot = self.config.lock().unwrap().clone();
        let Some((setup, key)) =
            super::service_startup::audiobookshelf_setup_and_key(&config_snapshot)
        else {
            return;
        };
        let generation = self.audiobookshelf_runtime.generation();
        let tx = self.lib_tx.clone();
        std::thread::spawn(move || {
            let result = mbv_core::audiobookshelf::AudiobookshelfClient::new(&setup.server_url)
                .and_then(|client| {
                    client.podcast_detail_bounded(
                        &key,
                        &library_item_id,
                        mbv_core::audiobookshelf::AudiobookshelfClient::REQUEST_HARD_BOUND,
                    )
                });
            let _ = tx.send(super::types_events::LibEvent::AudiobookshelfDetailFetched {
                generation,
                library_item_id,
                result,
            });
        });
    }

    /// Fetches the selected book's chapters/audio-files detail keyed by
    /// `library_item_id`.
    pub(super) fn start_audiobookshelf_book_detail(&mut self, library_item_id: String) {
        let Some(index) = self.tab.audiobookshelf_index() else {
            return;
        };
        let Some(state) = self.audiobookshelf_book_browse.get_mut(index) else {
            return;
        };
        if state.detail_cache.contains_key(&library_item_id)
            || state.detail_loading_ids.contains(&library_item_id)
        {
            state.detail_loading = state
                .selected_id
                .as_ref()
                .is_some_and(|id| state.detail_loading_ids.contains(id));
            return;
        }
        state.detail_loading_ids.insert(library_item_id.clone());
        state.detail_loading = true;
        let config_snapshot = self.config.lock().unwrap().clone();
        let Some((setup, key)) =
            super::service_startup::audiobookshelf_setup_and_key(&config_snapshot)
        else {
            return;
        };
        let generation = self.audiobookshelf_runtime.generation();
        let tx = self.lib_tx.clone();
        std::thread::spawn(move || {
            let result = mbv_core::audiobookshelf::AudiobookshelfClient::new(&setup.server_url)
                .and_then(|client| {
                    client.book_detail_bounded(
                        &key,
                        &library_item_id,
                        mbv_core::audiobookshelf::AudiobookshelfClient::REQUEST_HARD_BOUND,
                    )
                });
            let _ = tx.send(
                super::types_events::LibEvent::AudiobookshelfBookDetailFetched {
                    generation,
                    library_item_id,
                    result,
                },
            );
        });
    }

    pub(super) fn audiobookshelf_refresh(&mut self) {
        let Some(index) = self.tab.audiobookshelf_index() else {
            return;
        };
        let (library_id, generation) = {
            let Some(state) = self.audiobookshelf_browse.get_mut(index) else {
                return;
            };
            state.shows.clear();
            state.total = 0;
            state.next_page = 0;
            state.error = None;
            state.detail_cache.clear();
            state.episodes = None;
            // `episode_selection` / `scroll` are component-owned now
            // (split-browse-state-interaction-fields task 3.2); the content
            // push after this reset drops the selected show, which resets the
            // component's own interaction state.
            state.loading_pages.clear();
            // Mark page 0 pending before re-issuing it so the catalog reloads
            // from the first page (the renderer shows a Loading placeholder
            // until the response lands).
            state.loading_pages.insert(0);
            (
                state.library.id.clone(),
                self.audiobookshelf_runtime.generation(),
            )
        };
        // Restart the catalog request from page 0 after clearing state.
        super::service_startup::start_audiobookshelf_shows(
            self.config.lock().unwrap().clone(),
            generation,
            library_id,
            0,
            self.lib_tx.clone(),
        );
    }

    pub(super) fn select_audiobookshelf_show(&mut self, cursor: usize) {
        let Some(index) = self.tab.audiobookshelf_index() else {
            return;
        };
        let selected_id = {
            let Some(state) = self.audiobookshelf_browse.get_mut(index) else {
                return;
            };
            if state.shows.is_empty() {
                return;
            }
            state.select(cursor.min(state.shows.len() - 1));
            state.selected_id.clone()
        };
        if let Some(id) = selected_id {
            self.start_audiobookshelf_detail(id);
        }
    }

    /// Resolve the downloaded episode at `episode_index` at the Audiobookshelf
    /// playback boundary. Queue submission remains the responsibility of the
    /// later action stage; browse state never sees credentials or playback
    /// state. Read-only resolver seam for the pre-U5 App-level tests; the
    /// shell play/enqueue path threads the component-resolved episode index
    /// and filter directly (task 5.3d.11 U5). The episode filter is
    /// component-owned (split-browse-state-interaction-fields task 3.2), so
    /// this seam resolves against the unfiltered (`All`) view.
    #[cfg(test)]
    pub(super) fn activate_audiobookshelf_episode(
        &mut self,
        audiobookshelf_library_index: usize,
        episode_index: usize,
    ) -> Option<QueueItem> {
        self.selected_audiobookshelf_queue_item(
            audiobookshelf_library_index,
            episode_index,
            AudiobookshelfEpisodeFilter::All,
        )
    }

    /// Resolve the episode at `episode_index` for enqueue without mutating any
    /// queue or opening a playback lifecycle (see `activate_audiobookshelf_episode`).
    #[cfg(test)]
    pub(super) fn enqueue_audiobookshelf_episode(
        &mut self,
        audiobookshelf_library_index: usize,
        episode_index: usize,
    ) -> Option<QueueItem> {
        self.selected_audiobookshelf_queue_item(
            audiobookshelf_library_index,
            episode_index,
            AudiobookshelfEpisodeFilter::All,
        )
    }

    /// Ordinary play for the downloaded episode at `episode_index`. The shell
    /// resolves the target from the mounted component's selection (task
    /// 5.3d.11 U5); the App only supplies the provider-native snapshot, while
    /// canonical queue ownership and the eligible Player boundary remain here
    /// with the other ordinary actions.
    pub(super) fn play_selected_audiobookshelf_episode(
        &mut self,
        index: usize,
        episode_index: usize,
        filter: AudiobookshelfEpisodeFilter,
    ) {
        let Some(item) = self.selected_audiobookshelf_queue_item(index, episode_index, filter)
        else {
            return;
        };
        if !self.player.can_admit_audiobookshelf() {
            self.flash(
                "Audiobookshelf playback owner is unavailable".into(),
                ToastSeverity::Error,
            );
            return;
        }
        self.submit_queue_item(item, true);
    }

    /// Ordinary enqueue for the downloaded episode at `episode_index`. A cold
    /// local queue is the Composed stage and is intentionally allowed without
    /// owner admission; an active or remote playback target is Bound and must
    /// be eligible.
    pub(super) fn enqueue_selected_audiobookshelf_episode(
        &mut self,
        index: usize,
        episode_index: usize,
        filter: AudiobookshelfEpisodeFilter,
    ) {
        let Some(item) = self.selected_audiobookshelf_queue_item(index, episode_index, filter)
        else {
            return;
        };
        let scope = self.viewed_queue_scope();
        let bound = scope == self.playing_queue_scope()
            && (self.player.is_remote() || self.player.status.lock().unwrap().active);
        if bound && !self.player.can_admit_audiobookshelf() {
            self.flash(
                "Audiobookshelf playback owner is unavailable".into(),
                ToastSeverity::Error,
            );
            return;
        }
        self.submit_queue_item(item, false);
    }

    fn selected_audiobookshelf_queue_item(
        &self,
        audiobookshelf_library_index: usize,
        episode_index: usize,
        filter: AudiobookshelfEpisodeFilter,
    ) -> Option<QueueItem> {
        let state = self
            .audiobookshelf_browse
            .get(audiobookshelf_library_index)?;
        let episode = state
            .visible_episodes(filter)
            .get(episode_index)?
            .to_owned();
        if episode.library_item_id.trim().is_empty() || episode.episode_id.trim().is_empty() {
            return None;
        }
        let show = state.selected_show();
        let progress = state
            .progress
            .get(&(episode.library_item_id.clone(), episode.episode_id.clone()));
        let position_ticks = progress
            .map(|progress| seconds_to_ticks(progress.current_time_seconds))
            .unwrap_or(0);
        let is_finished = progress.is_some_and(|progress| progress.is_finished);

        Some(QueueItem::Audiobookshelf(AudiobookshelfQueueItem {
            library_item_id: episode.library_item_id.clone(),
            episode_id: episode.episode_id.clone(),
            title: episode.title.clone(),
            show_title: show.map(|show| show.title.clone()),
            author: show.and_then(|show| show.author.clone()),
            description: None,
            duration_ticks: episode.duration_seconds.and_then(seconds_to_ticks_u64),
            position_ticks,
            played: is_finished,
            pub_date_secs: episode
                .published_at
                .as_deref()
                .and_then(super::feed_parse_date::parse_pub_date_secs),
            is_finished,
            cover_path: show.and_then(|show| show.cover_path.clone()),
        }))
    }

    // ---- Book browsing actions -----------------------------------------

    pub(super) fn audiobookshelf_book_refresh(&mut self) {
        let Some(index) = self.tab.audiobookshelf_index() else {
            return;
        };
        let (library_id, generation) = {
            let Some(state) = self.audiobookshelf_book_browse.get_mut(index) else {
                return;
            };
            state.books.clear();
            state.total = 0;
            state.next_page = 0;
            state.error = None;
            state.detail_cache.clear();
            state.detail_loading_ids.clear();
            state.loading_pages.clear();
            state.loading_pages.insert(0);
            (
                state.library.id.clone(),
                self.audiobookshelf_runtime.generation(),
            )
        };
        super::service_startup::start_audiobookshelf_books(
            self.config.lock().unwrap().clone(),
            generation,
            library_id,
            0,
            self.lib_tx.clone(),
        );
    }

    pub(super) fn select_audiobookshelf_book(&mut self, cursor: usize) {
        let Some(index) = self.tab.audiobookshelf_index() else {
            return;
        };
        let selected_id = {
            let Some(state) = self.audiobookshelf_book_browse.get_mut(index) else {
                return;
            };
            if state.books.is_empty() {
                return;
            }
            state.select(cursor.min(state.books.len() - 1));
            state.selected_id.clone()
        };
        self.save_audiobookshelf_book_position(index);
        if let Some(id) = selected_id {
            self.start_audiobookshelf_book_detail(id);
        }
    }

    /// The book chapter focus is component-owned interaction state
    /// (split-browse-state-interaction-fields task 2.2): the component tracks
    /// it locally and carries the resolved row at activation time. This
    /// handler exists only so the `ChapterFocus` request stays claimed and
    /// routed (a redraw nudge); it stores nothing shell-side.
    pub(super) fn set_audiobookshelf_book_chapter_focus(&mut self, _selection: Option<usize>) {}

    /// Selects bucket `bucket_pos` (a position in `state.buckets`, matching
    /// the pill's click target -- the established pattern from
    /// `select_music_group`), narrowing the right-pane list to it and
    /// re-anchoring the cursor into the new bucket when it falls outside.
    pub(super) fn select_audiobookshelf_book_bucket(&mut self, bucket_pos: usize) {
        let Some(index) = self.tab.audiobookshelf_index() else {
            return;
        };
        let Some(bucket) = self
            .audiobookshelf_book_browse
            .get(index)
            .and_then(|state| state.buckets.get(bucket_pos).copied())
        else {
            return;
        };
        let target = {
            let Some(state) = self.audiobookshelf_book_browse.get(index) else {
                return;
            };
            let cursor = state.cursor();
            if cursor >= bucket.start && cursor < bucket.end {
                cursor
            } else {
                bucket.start
            }
        };
        if bucket.end > bucket.start {
            self.select_audiobookshelf_book(target);
        } else {
            self.save_audiobookshelf_book_position(index);
        }
    }

    /// Chapter-row activation: one absolute seek to `chapters[].start` on the
    /// active book's merged timeline, without stopping/reopening the queue
    /// slot or session (book-playback spec).
    pub(super) fn activate_audiobookshelf_book_row(&mut self, chapter_selection: Option<usize>) {
        let Some(index) = self.tab.audiobookshelf_index() else {
            return;
        };
        let (target_seconds, book_id) = {
            let state = match self.audiobookshelf_book_browse.get(index) {
                Some(state) => state,
                None => return,
            };
            let Some(id) = state.selected_id.as_deref() else {
                return;
            };
            let Some(cursor) = chapter_selection else {
                return;
            };
            let target = match state.visible_rows(id).get(cursor) {
                Some(BookRow::Chapter { start, .. }) => *start,
                // audio-file fallback rows have no chapter offsets; leave the
                // active position untouched rather than seeking somewhere wrong.
                Some(BookRow::AudioFile { .. }) => return,
                None => return,
            };
            (target, id.to_string())
        };
        // Only seek when the active queue slot is this book.
        let active_book = self
            .playback_queue()
            .queue
            .active_slot()
            .and_then(|slot| slot.item.as_audiobookshelf_book())
            .map(|book| book.library_item_id == book_id)
            .unwrap_or(false);
        if active_book {
            self.player
                .send_command(mbv_core::player::PlayerCommand::SeekAbsolute(
                    target_seconds,
                ));
        }
    }

    /// Resolve the selected book as a `QueueItem::AudiobookshelfBook` without
    /// mutating the queue or opening a playback lifecycle. Duration is the
    /// sum of the book's audio-file durations (chapters are offsets, not
    /// durations).
    fn selected_audiobookshelf_book_queue_item(
        &self,
        audiobookshelf_library_index: usize,
    ) -> Option<QueueItem> {
        let state = self
            .audiobookshelf_book_browse
            .get(audiobookshelf_library_index)?;
        let book = state.selected_id.as_ref()?;
        let book = state
            .books
            .iter()
            .find(|candidate| candidate.library_item_id == *book)?;
        if book.library_item_id.trim().is_empty() {
            return None;
        }
        let detail = state.detail_cache.get(&book.library_item_id);
        let duration_seconds = detail
            .map(|(_, audio_files)| audio_files.iter().map(|file| file.duration).sum())
            .filter(|duration| *duration > 0.0)
            .or_else(|| {
                detail.and_then(|(chapters, _)| {
                    chapters
                        .iter()
                        .map(|chapter| chapter.end)
                        .max_by(f64::total_cmp)
                })
            });
        let progress = state.progress.get(&book.library_item_id);
        let position_ticks = progress
            .map(|progress| seconds_to_ticks(progress.current_time_seconds))
            .unwrap_or(0);
        let is_finished = progress.is_some_and(|progress| progress.is_finished);

        Some(QueueItem::AudiobookshelfBook(AudiobookshelfBookQueueItem {
            library_item_id: book.library_item_id.clone(),
            title: book.title.clone(),
            author: book.author_display.clone(),
            duration_ticks: duration_seconds.and_then(seconds_to_ticks_u64),
            position_ticks,
            played: is_finished,
            is_finished,
            cover_path: book.cover_path.clone(),
        }))
    }

    pub(super) fn play_selected_audiobookshelf_book(&mut self, index: usize) {
        let Some(item) = self.selected_audiobookshelf_book_queue_item(index) else {
            return;
        };
        if !self.player.can_admit_audiobookshelf() {
            self.flash(
                "Audiobookshelf playback owner is unavailable".into(),
                ToastSeverity::Error,
            );
            return;
        }

        let scope = self.playing_queue_scope();
        let previous_queue = self.queue_for_scope(scope).clone();
        let existing_index = self
            .queue_for_scope(scope)
            .slots()
            .iter()
            .position(|slot| slot.item.content_id() == item.content_id());
        let selected_index = existing_index.unwrap_or_else(|| {
            self.queue_for_scope_mut(scope).queue.append(item.clone());
            self.queue_for_scope(scope).total_queue_len() - 1
        });
        let selected_slot = self
            .queue_for_scope(scope)
            .slot_id_at(selected_index)
            .expect("selected Audiobookshelf book queue slot disappeared");
        {
            let queue = self.queue_for_scope_mut(scope);
            queue.queue_cursor = selected_index;
            let _ = queue.queue.set_active_slot(selected_slot);
        }

        let all_items = self.queue_for_scope(scope).all_queue_items();
        let audio_only = all_items.iter().all(QueueItem::is_audio);
        let submitted =
            self.player
                .submit_queue(all_items, selected_index, None, audio_only, self.ui_volume);
        if !submitted {
            *self.queue_for_scope_mut(scope) = previous_queue;
            self.flash(
                "Playback owner rejected this Audiobookshelf book".into(),
                ToastSeverity::Error,
            );
            return;
        }
        self.set_queue_scope(scope);
        if !matches!(self.effective_panel_focus(), super::PanelFocus::Library) {
            self.set_panel_focus(super::PanelFocus::Queue);
        }
    }

    pub(super) fn enqueue_selected_audiobookshelf_book(&mut self, index: usize) {
        let Some(item) = self.selected_audiobookshelf_book_queue_item(index) else {
            return;
        };
        if !self.player.can_admit_audiobookshelf() {
            self.flash(
                "Audiobookshelf playback owner is unavailable".into(),
                ToastSeverity::Error,
            );
            return;
        }
        self.queue_for_scope_mut(self.viewed_queue_scope())
            .queue
            .append(item);
        self.queue_dirty = true;
    }
}

pub(super) fn seconds_to_ticks(seconds: f64) -> i64 {
    seconds_to_ticks_u64(seconds)
        .and_then(|ticks| i64::try_from(ticks).ok())
        .unwrap_or(0)
}

fn seconds_to_ticks_u64(seconds: f64) -> Option<u64> {
    (seconds.is_finite() && seconds >= 0.0)
        .then(|| (seconds * TICKS_PER_SECOND as f64).round() as u64)
}

#[cfg(test)]
#[path = "audiobookshelf_book_seek_tests.rs"]
mod book_seek_tests;

#[cfg(test)]
#[path = "split_browse_state_book_tests.rs"]
mod split_browse_state_book_tests;

#[cfg(test)]
#[path = "split_browse_state_podcast_tests.rs"]
mod split_browse_state_podcast_tests;
