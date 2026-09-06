use super::components::inline_search::InlineSearchHost;
use super::components::{
    BrowserComponent, ComponentId, MusicWorkspaceComponent, SearchPool, TvWorkspaceComponent,
};
use super::shell::Model;
use super::{AlbumIndexState, PanelFocus, TabSelection};

impl Model {
    pub(crate) fn active_inline_search_is_open(&self) -> bool {
        let Some(id) = self.active_inline_search_host() else {
            return false;
        };
        self.application
            .get_component(&id)
            .is_some_and(|component| {
                component
                    .as_any()
                    .downcast_ref::<BrowserComponent>()
                    .is_some_and(|host| host.inline_search().is_active())
                    || component
                        .as_any()
                        .downcast_ref::<MusicWorkspaceComponent>()
                        .is_some_and(|host| host.inline_search().is_active())
                    || component
                        .as_any()
                        .downcast_ref::<TvWorkspaceComponent>()
                        .is_some_and(|host| host.inline_search().is_active())
            })
    }

    fn active_inline_search_host(&self) -> Option<ComponentId> {
        // Resolve through the same active-destination pointer used for focus.
        // TV keeps both owners mounted across a breakpoint transition, but only
        // the destination selected for this tab may receive search state.
        self.library_child_id()
            .filter(|id| self.application.mounted(id))
    }

    fn with_active_inline_search_host(
        &mut self,
        f: impl FnOnce(&mut dyn InlineSearchHost),
    ) -> bool {
        let Some(id) = self.active_inline_search_host() else {
            return false;
        };
        let Some(component) = self.application.get_component_mut(&id) else {
            return false;
        };
        if let Some(host) = component.as_any_mut().downcast_mut::<BrowserComponent>() {
            f(host);
            return true;
        }
        if let Some(host) = component
            .as_any_mut()
            .downcast_mut::<MusicWorkspaceComponent>()
        {
            f(host);
            return true;
        }
        if let Some(host) = component
            .as_any_mut()
            .downcast_mut::<TvWorkspaceComponent>()
        {
            f(host);
            return true;
        }
        false
    }

    fn inline_search_needs_full_load(&self, index: usize) -> bool {
        self.app.libs[index].nav_stack.last().is_some_and(|level| {
            level.all_items.is_none()
                && (level.letter_filter.is_some() || level.items.len() < level.total_count)
        })
    }

    pub(super) fn capture_inline_search_transfer(
        &self,
        id: &ComponentId,
    ) -> Option<super::shell::InlineSearchTransfer> {
        let component = self.application.get_component(id)?;
        let search = component
            .as_any()
            .downcast_ref::<BrowserComponent>()
            .map(|h| h.inline_search())
            .or_else(|| {
                component
                    .as_any()
                    .downcast_ref::<MusicWorkspaceComponent>()
                    .map(|h| h.inline_search())
            })
            .or_else(|| {
                component
                    .as_any()
                    .downcast_ref::<TvWorkspaceComponent>()
                    .map(|h| h.inline_search())
            })?;
        // Only an open session transfers (design.md: a transfer moves an
        // already-open session); a closed search must not open on the
        // receiving host.
        search.is_active().then_some(())?;
        let (selected_id, selected_type) = search
            .selected_target()
            .map_or((None, None), |(id, ty)| (Some(id), Some(ty)));
        Some(super::shell::InlineSearchTransfer {
            query: search.query().to_string(),
            selected_id,
            selected_type,
            row_offset: search.scroll(),
        })
    }

    pub(super) fn dismiss_active_inline_search(&mut self) {
        if let Some(id) = self.active_inline_search_host() {
            self.close_inline_search_host(&id);
        }
    }

    pub(super) fn close_inline_search_host(&mut self, id: &ComponentId) {
        if let Some(component) = self.application.get_component_mut(id) {
            if let Some(host) = component.as_any_mut().downcast_mut::<BrowserComponent>() {
                host.close_inline_search();
            } else if let Some(host) = component
                .as_any_mut()
                .downcast_mut::<MusicWorkspaceComponent>()
            {
                host.close_inline_search();
            } else if let Some(host) = component
                .as_any_mut()
                .downcast_mut::<TvWorkspaceComponent>()
            {
                host.close_inline_search();
            }
        }
    }

    pub(super) fn apply_inline_search_transfer(
        &mut self,
        id: &ComponentId,
        transfer: super::shell::InlineSearchTransfer,
    ) {
        let target = transfer.selected_id.zip(transfer.selected_type);
        if let Some(component) = self.application.get_component_mut(id) {
            if let Some(host) = component.as_any_mut().downcast_mut::<BrowserComponent>() {
                host.apply_inline_search_transfer(transfer.query, target, transfer.row_offset);
            } else if let Some(host) = component
                .as_any_mut()
                .downcast_mut::<MusicWorkspaceComponent>()
            {
                host.apply_inline_search_transfer(transfer.query, target, transfer.row_offset);
            } else if let Some(host) = component
                .as_any_mut()
                .downcast_mut::<TvWorkspaceComponent>()
            {
                host.apply_inline_search_transfer(transfer.query, target, transfer.row_offset);
            }
        }
    }

    pub(super) fn apply_pending_inline_search_transfer(&mut self) {
        let Some(transfer) = self.inline_search_transfer.take() else {
            return;
        };
        let Some(id) = self.active_inline_search_host() else {
            return;
        };
        self.apply_inline_search_transfer(&id, transfer);
    }

    pub(super) fn push_inline_search_content(&mut self) {
        let TabSelection::EmbyLibrary(index) = self.app.tab else {
            return;
        };
        if self.active_inline_search_host().is_none() {
            return;
        }
        // Flat path: this push only projects the flat `Items` pool. Loading
        // is exactly while the whole-library fetch backing `all_items` is
        // outstanding (see `inline_search_needs_full_load`). Intermediate
        // pushes -- resize, browse completion, activation -- keep the spinner
        // up; the completion push (all_items now present) clears it.
        let loading = self.inline_search_needs_full_load(index);
        let recursive = self.app.recursive_album_search_enabled(index);
        let pool = if recursive {
            let library_id = self.app.libs[index].library.id.clone();
            match self.app.album_indexes.get(&library_id) {
                Some(AlbumIndexState::Ready(entries)) => SearchPool::Albums(entries.clone()),
                _ => SearchPool::Albums(Vec::new()),
            }
        } else {
            let items = self.app.libs[index]
                .nav_stack
                .last()
                .map(|level| {
                    level
                        .all_items
                        .clone()
                        .unwrap_or_else(|| level.items.clone())
                })
                .unwrap_or_default();
            SearchPool::Items(items)
        };
        let focused = matches!(self.app.effective_panel_focus(), PanelFocus::Library);
        self.with_active_inline_search_host(|host| {
            host.set_inline_search_content(pool, loading, focused);
        });
    }

    pub(super) fn open_inline_search(&mut self) {
        if !self.with_active_inline_search_host(|host| host.open_inline_search()) {
            return;
        }
        let TabSelection::EmbyLibrary(index) = self.app.tab else {
            return;
        };
        let recursive = self.app.recursive_album_search_enabled(index);
        let mut needs_full_load = false;
        if recursive {
            self.app.start_album_index(index, false);
        } else {
            needs_full_load = self.inline_search_needs_full_load(index);
            if needs_full_load {
                self.app.spawn_search_items_load(index);
            }
        }
        // Initial pool/loading/focus push (the deleted mirror's first-frame
        // projection, at the open event).
        self.push_inline_search_content();
        if (recursive
            && matches!(
                self.app.album_indexes.get(&self.app.libs[index].library.id),
                Some(AlbumIndexState::Loading { .. })
            ))
            || needs_full_load
        {
            self.set_inline_search_loading(true);
        }
    }

    pub(super) fn activate_inline_search_item(&mut self, id: String, item_type: String) {
        let TabSelection::EmbyLibrary(lib_idx) = self.app.tab else {
            return;
        };
        let selected = self
            .active_inline_search_host()
            .and_then(|id| self.application.get_component(&id))
            .and_then(|component| {
                component
                    .as_any()
                    .downcast_ref::<BrowserComponent>()
                    .and_then(|h| h.selected_inline_search_item())
                    .or_else(|| {
                        component
                            .as_any()
                            .downcast_ref::<MusicWorkspaceComponent>()
                            .and_then(|h| h.selected_inline_search_item())
                    })
                    .or_else(|| {
                        component
                            .as_any()
                            .downcast_ref::<TvWorkspaceComponent>()
                            .and_then(|h| h.selected_inline_search_item())
                    })
            });
        if self.app.recursive_album_search_enabled(lib_idx) {
            let library_id = self.app.libs[lib_idx].library.id.clone();
            let entry = match self.app.album_indexes.get(&library_id) {
                Some(AlbumIndexState::Ready(entries)) => entries
                    .iter()
                    .find(|entry| entry.album.id == id && entry.album.item_type == item_type)
                    .cloned(),
                _ => None,
            };
            if let Some(entry) = entry {
                self.app.activate_recursive_album(lib_idx, entry);
                // Enter on an album result returns to the standard library
                // presentation: dismiss Inline Search and request the Music
                // workspace re-anchor onto the activated album and enter
                // track-selection mode at the next sync. The async
                // `RecursiveAlbumActivated` event refines the nav stack to the
                // album's natural pill/list position and re-sends the same
                // one-shot requests (shell_run.rs) once it lands.
                self.music_workspace_reanchor = true;
                self.music_track_focus_request = Some(true);
                self.dismiss_active_inline_search();
                self.push_music_workspace_content();
                return;
            }
        } else if let Some(item) =
            selected.filter(|item| item.id == id && item.item_type == item_type)
        {
            self.app.select_item(lib_idx, item);
        }
        // Activation may have navigated (flat folder push) or queued
        // playback; re-project the pool/focus at this event point, exactly
        // as the deleted per-frame mirror did on the following tick.
        self.push_inline_search_content();
    }

    fn set_inline_search_loading(&mut self, loading: bool) {
        self.with_active_inline_search_host(|host| {
            host.inline_search_mut().set_loading(loading);
        });
    }

    /// Drain tail for the inline search (called from the shell's `lib_rx`
    /// loop): completions that can change the mounted search's projected pool
    /// — flat `nav_stack` items/`all_items`, or recursive `album_indexes` —
    /// re-push it after the App handles the event. The deleted per-frame
    /// mirror's projection is driven at async event boundaries.
    pub(super) fn handle_inline_search_lib_event(&mut self, ev: super::LibEvent) {
        let pushes_inline_search = matches!(
            ev,
            super::LibEvent::Refreshed { .. }
                | super::LibEvent::AllItemsPrefetched { .. }
                | super::LibEvent::AlbumIndexBuilt { .. }
                | super::LibEvent::NavigateTo { .. }
                | super::LibEvent::SearchItemsLoaded { .. }
        );
        if matches!(
            &ev,
            super::LibEvent::NavigateTo {
                switch_tab: true,
                ..
            }
        ) {
            self.dismiss_active_inline_search();
        }
        self.app.handle_lib_event(ev);
        if pushes_inline_search {
            self.push_inline_search_content();
        }
    }
}
