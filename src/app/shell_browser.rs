use super::components::{BrowserComponent, BrowserKey, BrowserKind, ComponentId, ShellRequest};
use super::shell::Model;
use super::{ConfirmAction, ConfirmModal, TabSelection};
use crate::app::components::browser::{BrowserContent, BrowserIdentity};
use crate::app::images::NAV_IMAGE_FETCH_IDLE_DELAY;
use crate::app::render::{wide_hero_presentation, LibraryListRenderCtx};
use mbv_core::config::ServiceKind;
use std::time::Instant;

impl Model {
    /// Route the generic Emby browser's selected-item typed effects (task
    /// 5.3d, Emby browser effect decoupling) to their `App` handlers with the
    /// component-resolved owned target. `BrowserComponent` resolves its own
    /// selected `EmbyItem` from its component-local cursor/content; the
    /// effect acts on that supplied item directly — never by copying the
    /// component cursor into a `BrowseLevel.cursor` and re-reading it. The
    /// active library index is derived from the shell's own tab state (the
    /// browser is mounted only for the active generic/Movies/home-video
    /// `EmbyLibrary` tab, same derivation as the `BrowserRow*`/`BrowserPillClick` mouse arms).
    /// A missing library index is a defensive no-op.
    pub(super) fn handle_browser_request(&mut self, request: ShellRequest) {
        let Some(lib_idx) = self.app.tab.emby_library_index() else {
            return;
        };
        match request {
            // A `Series` item routes through the shared Series-activation gate
            // first (task 3.4a): at narrow TV width — the only layout where
            // `BrowserComponent` is mounted for a TV library — that reopens the
            // season-selection modal instead of a flat drill-in. `false` means
            // it was not a Series (or had no id), so fall back to the normal
            // select-item path, including the folder scroll-persist.
            ShellRequest::BrowserActivate { item } => {
                if item.item_type == "Series"
                    && self.app.activate_selected_series_item(lib_idx, &item)
                {
                    // handled by Series activation (season-selection modal at
                    // narrow width, persistent workspace at wide)
                } else {
                    if item.is_folder {
                        self.persist_emby_browser_scroll(lib_idx);
                    }
                    self.app.select_item(lib_idx, item);
                }
            }
            ShellRequest::BrowserPlay { item } | ShellRequest::EmbyLibraryPlay { item } => {
                self.app.play_or_activate_lib_item(lib_idx, item)
            }
            ShellRequest::BrowserEnqueue { item } | ShellRequest::EmbyLibraryEnqueue { item } => {
                self.app.enqueue_lib_item(lib_idx, item)
            }
            ShellRequest::BrowserToggleWatched { item }
            | ShellRequest::EmbyLibraryToggleWatched { item } => {
                self.app.toggle_watched_item(lib_idx, item)
            }
            // '.' raises the context menu for the supplied item via the
            // existing item-targeted seam; the non-folder/mark-watched menu
            // content derives from the shell's own tab state (`lib_idx` just
            // guards that this is an EmbyLibrary tab), never a `BrowseLevel`
            // cursor re-read.
            ShellRequest::BrowserContextMenu { item }
            | ShellRequest::EmbyLibraryContextMenu { item } => self.app.open_context_menu_for(item),
            // Ctrl+S shuffles the supplied item with the preserved
            // `shuffle_play` tail: a folder item shuffles the folder itself;
            // a non-folder item shuffles the current browse level's parent
            // (falling back to the library id). The folder target comes from
            // the component-resolved item, never a `BrowseLevel.cursor`
            // re-read.
            ShellRequest::BrowserShuffle { item } | ShellRequest::EmbyLibraryShuffle { item } => {
                self.app.shuffle_play_selected(lib_idx, item)
            }
            // Bare `r` refreshes the active Emby library (task 5.3d,
            // Emby browser refresh): the shell derives the active library
            // index from its own tab state and runs `App::refresh_lib` on it,
            // the same call the legacy `handle_lib_key` `Char('r')` arm made.
            ShellRequest::BrowserRefresh | ShellRequest::EmbyLibraryRefresh => {
                self.app.refresh_lib(lib_idx)
            }
            // Ctrl+`r` raises the Rescan Library confirmation (task 5.3d,
            // Emby browser rescan): same title/message/hint and
            // `ConfirmAction::RescanLibrary(lib_idx)` as the legacy
            // `handle_lib_key` CONTROL arm, derived from the shell's own tab
            // state (the library name comes from the active library).
            ShellRequest::BrowserRescan | ShellRequest::EmbyLibraryRescan => {
                let name = self.app.libs[lib_idx].library.name.clone();
                self.app.ask_confirm(ConfirmModal {
                    title: " Rescan Library ".into(),
                    message: format!("Rescan '{name}'?"),
                    hint: "[y] Confirm    [Esc] Cancel".into(),
                    on_confirm: ConfirmAction::RescanLibrary(lib_idx),
                });
            }
            // Esc/Backspace go back through the browse history (task 5.3d,
            // Emby browser back): the shell derives the active Emby library
            // index from its own tab state and runs `App::go_back` on it, the
            // same call the legacy `handle_lib_key` `Esc | Backspace` arm
            // made — preserving synthetic-group/root guards, parent-cursor
            // restoration, season-level skip, persistence, and stale-index
            // behavior.
            ShellRequest::BrowserBack => {
                if self.app.libs[lib_idx].nav_stack.len() > 1 {
                    self.persist_emby_browser_scroll(lib_idx);
                }
                self.app.go_back(lib_idx);
            }
            // `[`/`]` cycle the letter-range pill row (task 5.3d, Emby
            // browser selector cycling): the shell derives the active Emby
            // library index from its own tab state and runs
            // `App::cycle_letter_pill` on it, the same call the legacy
            // `handle_key_emby_library` arm made — preserving the
            // `should_show_letter_pills` no-op guard and the existing
            // wrap/select behavior (the component's mount gate has already
            // excluded the Music and feed-home-video group branches).
            ShellRequest::BrowserCycleLetterPill { delta } => {
                self.app.cycle_letter_pill(lib_idx, delta)
            }
            // `[`/`]` on a feed/home-video group-picker library
            // (`is_feed_home_video_group_view`, migrate-narrow-browse task
            // 2.2): the component's projected content carries the
            // group-pill flag, so its bracket keys mean group cycling; the
            // shell derives the active library index from its own tab state
            // and runs `App::switch_feed_folder_group` (rem_euclid wrap over
            // "All" + every visible group).
            ShellRequest::BrowserCycleGroup { delta } => {
                self.app.switch_feed_folder_group(lib_idx, delta)
            }
            // Every local browser cursor key (arrows/hjkl, Page keys,
            // Home/End) resolves to an item index inside the component and
            // arrives here already resolved. Keep the resting-position write
            // and its navigation effects in this shell arm.
            ShellRequest::BrowserCursorIndex { index } => {
                if lib_idx >= self.app.libs.len() {
                    return;
                }
                let now = Instant::now();
                let idle = now.duration_since(self.app.last_nav_at) >= NAV_IMAGE_FETCH_IDLE_DELAY;
                self.app.last_nav_at = now;
                self.app.mark_library_navigation(now);
                if self.app.is_feed_home_video_group_view(lib_idx) {
                    if let Some(state) = self.app.libs[lib_idx].feed_home_video.as_mut() {
                        if state.selected_len() > 0 {
                            state.video_cursor = index;
                            self.app.save_default_library_position(lib_idx);
                        }
                    }
                    return;
                }
                if let Some(level) = self.app.libs[lib_idx].nav_stack.last_mut() {
                    level.set_resting_cursor(index);
                    self.app.save_default_library_position(lib_idx);
                }
                if idle {
                    self.app.maybe_fetch_next_page(lib_idx, index);
                }
            }
            // unreachable: shell_messages.rs top-level dispatch routes only the
            // Browser* and EmbyLibrary* activate/effect groups plus
            // BrowserCursorIndex into handle_browser_request; every one has
            // an arm above.
            _ => {}
        }
    }

    pub(super) fn persist_emby_browser_scroll_for_active_library(&mut self) {
        let TabSelection::EmbyLibrary(lib_idx) = self.app.tab else {
            return;
        };
        self.persist_emby_browser_scroll(lib_idx);
    }

    fn persist_emby_browser_scroll(&mut self, lib_idx: usize) {
        let Some(id) = self.emby_browser_id.as_ref() else {
            return;
        };
        let scroll = self
            .application
            .get_component(id)
            .and_then(|comp| comp.as_any().downcast_ref::<BrowserComponent>())
            .map(BrowserComponent::scroll);
        if let Some(scroll) = scroll {
            self.app.persist_library_scroll(lib_idx, scroll);
        }
    }

    pub(super) fn emby_browser_component_id(&self) -> Option<ComponentId> {
        let TabSelection::EmbyLibrary(index) = self.app.tab else {
            return None;
        };
        let library = self.app.libs.get(index)?;
        let kind = BrowserKind::from_collection_type(&library.library.collection_type);
        let owns = match kind {
            BrowserKind::Generic | BrowserKind::Movies | BrowserKind::HomeVideos => true,
            // Narrow TV is a flat series list this component already handles
            // (D4). Wide TV routes to TvWorkspaceComponent instead; the two
            // gates share `wide_tv_library_area(index)` so they are mutually exclusive
            // for a TV library at every width.
            BrowserKind::TvShows => !self.app.wide_tv_library_area(index).is_some(),
            _ => false,
        };
        if !owns {
            return None;
        }
        let id = ComponentId::Browser(BrowserKey {
            service: ServiceKind::Emby,
            library_id: library.library.id.clone(),
            kind,
        });
        debug_assert!(
            self.tv_workspace_component_id().as_ref() != Some(&id),
            "narrow BrowserComponent and wide TvWorkspaceComponent must not share a ComponentId"
        );
        Some(id)
    }

    /// Reconcile the mounted Emby browser against the currently-active Emby
    /// library tab (task 5.3d.15/M1 extraction from `sync_emby_browser`),
    /// idempotently. If the active id matches the gate (`emby_browser_component_id`)
    /// it does nothing. Mount lifecycle only; content projection and layout
    /// adapters stay in `sync_emby_browser`.
    ///
    /// Keep-mounted (keep-destination-components-mounted task 2.1): the
    /// component stays mounted across tab switches so it retains its private
    /// state; the `*_id` field is an active-destination pointer. When the
    /// active id differs, mount the new id only if not already mounted,
    /// repoint the pointer (which may become `None`), and refresh content on
    /// re-point. Focus is owned by `sync_active_destination`, so this no
    /// longer calls `active()`.
    pub(super) fn mount_emby_browser(&mut self) {
        // The feed/home-video group-picker surface's root listing must be
        // loaded shell-side (migrate-narrow-browse task 2.2): the legacy
        // painter called `ensure_lib_loaded_for` from inside the render, and
        // `is_feed_home_video_group_view` — the mount/projection gate — only
        // turns true once that load has produced state. Mirrors the
        // unconditional call `render_list` still makes for library tabs.
        if let TabSelection::EmbyLibrary(index) = self.app.tab {
            if index < self.app.libs.len()
                && (self.app.is_feed_home_video_library(index)
                    || self.app.is_podcast_library(index))
            {
                self.app.ensure_lib_loaded_for(index);
            }
        }
        let next_id = self.emby_browser_component_id();
        if self.emby_browser_id != next_id {
            match next_id {
                Some(id) => {
                    if !self.application.mounted(&id) {
                        let kind = match &id {
                            ComponentId::Browser(key) => key.kind,
                            _ => unreachable!("Emby browser id must be Browser"),
                        };
                        self.application
                            .mount(
                                id.clone(),
                                Box::new(BrowserComponent::new_for_kind(kind)),
                                vec![],
                            )
                            .expect("mount Emby browser");
                        self.register_destination(&id);
                    }
                    self.emby_browser_id = Some(id);
                    self.push_emby_browser_content();
                }
                None => {
                    self.emby_browser_id = None;
                }
            }
        }
    }

    /// Event-driven content projection for the mounted Emby browser (task
    /// 5.3d.15/M2): the per-frame `sync_emby_browser` no longer rewrites
    /// content every loop pass. This mirror applies the current library
    /// `render_ctx`, cursor and panel-focus flag idempotently, and is called
    /// at every writer seam that can change the active Emby library (the same
    /// seams that re-project Home). `configure_wide_movies` is NOT here — the wide
    /// Movies rail stride is a per-draw layout fact pushed in
    /// `render_emby_browser_component` (D18 step 1).
    pub(super) fn push_emby_browser_content(&mut self) {
        let Some(id) = self.emby_browser_id.as_ref() else {
            return;
        };
        let TabSelection::EmbyLibrary(index) = self.app.tab else {
            return;
        };
        // Feed/home-video group picker (migrate-narrow-browse task 2.2): this
        // surface never flows through `library_list_render_ctx` — every cursor
        // path branches on `is_feed_home_video_group_view` first and
        // reads/writes `feed_home_video.{video_cursor,video_scroll}`. Project
        // the selected group's items with that cursor/scroll and flag the
        // group-pill row so the component's `[`/`]` chord means group cycling.
        let (context, cursor, scroll) = if self.app.is_feed_home_video_group_view(index) {
            let (cursor, scroll) = self.app.libs[index]
                .feed_home_video
                .as_ref()
                .map(|state| (state.video_cursor, state.video_scroll))
                .unwrap_or((0, 0));
            let loading = self.app.libs[index]
                .feed_home_video
                .as_ref()
                .map(|state| state.loading)
                .or_else(|| {
                    self.app.libs[index]
                        .nav_stack
                        .first()
                        .map(|root| root.loading)
                })
                .unwrap_or(false);
            let ctx = LibraryListRenderCtx::from_items(
                self.app.feed_home_video_selected_items(index),
                cursor,
                scroll,
            )
            .with_group_pills(true)
            .with_loading(loading);
            (ctx, cursor, scroll)
        } else {
            let cursor = self.app.libs[index]
                .nav_stack
                .last()
                .map_or(0, |l| l.resting().cursor());
            let scroll = self.app.libs[index]
                .nav_stack
                .last()
                .map_or(0, |l| l.resting().scroll());
            (
                self.app.library_list_render_ctx(index, cursor, scroll),
                cursor,
                scroll,
            )
        };
        let content = BrowserContent::from_render_ctx(context);
        let identity = self.browse_identity(index);
        if let Some(comp) = self.application.get_component_mut(id) {
            if let Some(browser) = comp.as_any_mut().downcast_mut::<BrowserComponent>() {
                browser.set_content(content);
                // Position re-seeds only on a browse-identity change; within one
                // identity (pagination, loading completion, refresh, the
                // component's own cursor echo) the control keeps its cursor.
                if browser.note_browse_identity(identity) {
                    browser.apply_position(cursor, scroll);
                }
            }
        }
    }

    /// The browse identity of library `index`'s current level (task 3.7):
    /// nav-stack depth, level `parent_id`, `letter_filter`, sort, `unplayed_only`,
    /// and the selected feed/home-video group. `push_emby_browser_content`
    /// re-seeds position only when this differs from the previous push.
    fn browse_identity(&self, index: usize) -> BrowserIdentity {
        let lib = &self.app.libs[index];
        let level = lib.nav_stack.last();
        BrowserIdentity {
            depth: lib.nav_stack.len(),
            parent_id: level.map(|l| l.parent_id.clone()).unwrap_or_default(),
            letter_filter: level.and_then(|l| l.letter_filter.as_ref().map(|f| f.index)),
            sort_by: level.map(|l| l.sort_by.clone()).unwrap_or_default(),
            sort_order: level.map(|l| l.sort_order.clone()).unwrap_or_default(),
            unplayed_only: level.is_some_and(|l| l.unplayed_only),
            feed_group: lib
                .feed_home_video
                .as_ref()
                .map(|s| s.selected_group_index()),
        }
    }

    /// Legacy per-frame entry point (task 5.3d.15/M2): mount + content
    /// projection only. Kept for test compatibility; the live event loop
    /// still calls it once per loop pass, and the wide-Movies adapter now
    /// rides the per-draw render path.
    pub(super) fn sync_emby_browser(&mut self) {
        self.mount_emby_browser();
        self.push_emby_browser_content();
    }

    pub(crate) fn render_emby_browser_component(&mut self, frame: &mut ratatui::Frame) {
        let Some(id) = self.emby_browser_id.as_ref() else {
            return;
        };
        // When the wide Movies/home-video layout is active, the component
        // paints the full Wide hero rect; otherwise it paints the narrow
        // inner list area. Derive the presentation from the same shared
        // arrangement predicate used by BrowserComponent::view.
        let area = self.app.layout.main.left_area;
        let wide = wide_hero_presentation(area).is_some();
        if area.width == 0 || area.height == 0 {
            return;
        }
        // Per-draw adapter (D18 step 1): the legacy base frame and the mounted
        // component share one paint, so the 1-column right-rail stride is
        // consistent here. `home_video`/`letter_pills` tell the component
        // which pill row to render in the wide right rail.
        let (home_video, letter_pills) = if wide {
            match self.app.tab.emby_library_index() {
                Some(lib_idx) => (
                    self.app.is_home_video_view(lib_idx),
                    self.app.should_show_letter_pills(lib_idx),
                ),
                None => (false, false),
            }
        } else {
            (false, false)
        };
        // Narrow generic/Movies/home-video: resolve the count label, letter
        // pills and inline movie/series hero shell-side and push them to the
        // component before its `view` composes the surface (task 3.3).
        let browser_cursor = self
            .application
            .get_component(id)
            .and_then(|comp| comp.as_any().downcast_ref::<BrowserComponent>())
            .map_or(0, BrowserComponent::cursor);
        let narrow_extras = self
            .app
            .tab
            .emby_library_index()
            .map(|lib_idx| self.app.narrow_browse_extras(lib_idx, browser_cursor));
        if let Some(comp) = self.application.get_component_mut(id) {
            if let Some(browser) = comp.as_any_mut().downcast_mut::<BrowserComponent>() {
                browser.configure_wide_movies(home_video, letter_pills);
                browser.set_use_nerd_fonts(self.app.use_nerd_fonts);
                browser.set_images_enabled(self.app.images_enabled());
                if let Some(extras) = narrow_extras {
                    browser.set_narrow_extras(extras);
                }
                // Poster prefetch is an App/image-cache effect, so keep it
                // beside the other shell-owned image effect. The component's
                // cursor is authoritative; the mirrored library content only
                // supplies the candidate window.
                if !wide {
                    if let Some(lib_idx) = self.app.tab.emby_library_index() {
                        let ctx = self.app.library_list_render_ctx(
                            lib_idx,
                            browser.cursor(),
                            browser.scroll(),
                        );
                        if ctx
                            .clone()
                            .with_cursor_scroll(browser.cursor(), 0)
                            .selected_item()
                            .is_some_and(|item| item.item_type == "Movie" && !item.is_folder)
                        {
                            self.app
                                .fetch_nearby_movie_posters(&ctx.items, browser.cursor());
                        }
                    }
                }
            }
        }
        self.application.view(id, frame, area);
        // Paint the hero cover image the component computed but could not
        // paint itself (no image-cache authority), mirroring HomeComponent.
        // Also read back the scroll the component painted at, so it can be
        // persisted into the App nav level (task 5.3d.17b).
        let image_paint = self
            .application
            .get_component_mut(id)
            .and_then(|comp| comp.as_any_mut().downcast_mut::<BrowserComponent>())
            .and_then(BrowserComponent::take_image_paint);
        self.app.paint_home_image(frame, image_paint);
    }
}

#[cfg(test)]
#[path = "shell_browser_tests.rs"]
mod tests;
