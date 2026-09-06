use super::super::components::{
    ComponentId, HelpComponent, OverlayId, PlaylistsComponent, SearchSidebarComponent,
    SessionsComponent, SettingsComponent,
};
use super::super::components::{Msg, UserEvent};
use super::super::shell::Model;
use super::super::SidebarId;
use tuirealm::component::AppComponent;

impl Model {
    fn sidebar_component_id(sidebar: SidebarId) -> ComponentId {
        ComponentId::Overlay(match sidebar {
            SidebarId::Settings => OverlayId::Settings,
            SidebarId::Sessions => OverlayId::Sessions,
            SidebarId::Playlists => OverlayId::Playlists,
            SidebarId::Search => OverlayId::Search,
        })
    }

    fn sidebar_component(sidebar: SidebarId) -> Box<dyn AppComponent<Msg, UserEvent>> {
        match sidebar {
            SidebarId::Settings => Box::new(SettingsComponent::new()),
            SidebarId::Sessions => Box::new(SessionsComponent::new()),
            SidebarId::Playlists => Box::new(PlaylistsComponent::new()),
            SidebarId::Search => Box::new(SearchSidebarComponent::new()),
        }
    }

    pub(in crate::app) fn mount_sidebar(&mut self, sidebar: SidebarId) {
        let id = Self::sidebar_component_id(sidebar);
        for other in [
            SidebarId::Settings,
            SidebarId::Sessions,
            SidebarId::Playlists,
            SidebarId::Search,
        ] {
            let other_id = Self::sidebar_component_id(other);
            if other_id != id && self.application.mounted(&other_id) {
                let _ = self.application.umount(&other_id);
            }
        }
        if !self.application.mounted(&id) {
            self.application
                .mount(id.clone(), Self::sidebar_component(sidebar), vec![])
                .expect("mount sidebar");
        }
        self.application.active(&id).expect("activate sidebar");
        if sidebar == SidebarId::Sessions {
            self.app.spawn_sessions_load();
            self.app.spawn_cast_discovery();
        }
    }

    pub(in crate::app) fn dismiss_sidebar(&mut self, sidebar: SidebarId) {
        let id = Self::sidebar_component_id(sidebar);
        let _ = self.application.umount(&id);
    }

    pub(in crate::app) fn toggle_sidebar(&mut self, sidebar: SidebarId) {
        let id = Self::sidebar_component_id(sidebar);
        if self.application.mounted(&id) {
            self.dismiss_sidebar(sidebar);
        } else {
            self.mount_sidebar(sidebar);
        }
    }

    pub(in crate::app) fn dismiss_sidebars(&mut self) {
        for sidebar in [
            SidebarId::Settings,
            SidebarId::Sessions,
            SidebarId::Playlists,
            SidebarId::Search,
        ] {
            self.dismiss_sidebar(sidebar);
        }
    }

    pub(in crate::app) fn sync_sidebar_overlays(&mut self) {
        self.update_settings_content();
        self.update_playlists_content();
        self.update_sessions_content();

        let panel_area =
            (self.app.layout.main.panel_area.width > 0).then_some(self.app.layout.main.panel_area);
        let help_id = ComponentId::Overlay(OverlayId::Help);
        if let Some(comp) = self.application.get_component_mut(&help_id) {
            if let Some(help) = comp.as_any_mut().downcast_mut::<HelpComponent>() {
                help.set_panel_area(panel_area);
                help.set_destination(self.app.effective_panel_focus(), self.app.tab);
            }
        }

        let search_id = Self::search_id();
        if let Some(comp) = self.application.get_component_mut(&search_id) {
            if let Some(search) = comp.as_any_mut().downcast_mut::<SearchSidebarComponent>() {
                search.set_panel_area(panel_area);
            }
        }
    }

    fn update_sessions_content(&mut self) {
        let id = Self::sessions_id();
        if !self.application.mounted(&id) {
            return;
        }
        let panel_area =
            (self.app.layout.main.panel_area.width > 0).then_some(self.app.layout.main.panel_area);
        let connected_session_id = self.app.connected_session_id.as_deref();
        let tracking = self.app.remote_tracker.is_some();
        let cast_attachment_id = self
            .app
            .cast_attachment
            .as_ref()
            .map(|attachment| attachment.receiver_id.as_str());
        if let Some(comp) = self.application.get_component_mut(&id) {
            if let Some(sessions) = comp.as_any_mut().downcast_mut::<SessionsComponent>() {
                sessions.set_content(
                    &self.app.panel_targets,
                    self.app.sessions_loading,
                    connected_session_id,
                    tracking,
                    cast_attachment_id,
                    self.app.can_disconnect_remote(),
                    panel_area,
                );
            }
        }
    }

    // --- Help sidebar -------------------------------------------------------

    /// Mount the Help overlay and make it the active component. Closes the
    /// non-blocking overlays (settings/sessions/playlists) first, matching the
    /// legacy F1 arms in each of their handlers.
    pub(in crate::app) fn mount_help(&mut self) {
        self.dismiss_sidebars();
        self.application
            .mount(
                ComponentId::Overlay(OverlayId::Help),
                Box::new(HelpComponent::new()),
                vec![],
            )
            .expect("mount Help");
        self.application
            .active(&ComponentId::Overlay(OverlayId::Help))
            .expect("activate Help");
    }

    /// Unmount the Help overlay; TuiRealm's LIFO focus stack auto-restores
    /// focus to the prior component.
    pub(in crate::app) fn umount_help(&mut self) {
        let _ = self
            .application
            .umount(&ComponentId::Overlay(OverlayId::Help));
    }

    /// Render the Help overlay if mounted, after the legacy `App::render`.
    pub(in crate::app) fn render_help_overlay(&mut self, f: &mut ratatui::Frame) {
        let help_id = ComponentId::Overlay(OverlayId::Help);
        if !self.application.mounted(&help_id) {
            return;
        }
        self.application.view(&help_id, f, f.area());
    }
    // --- Search sidebar -----------------------------------------------------
    //
    // The Search sidebar is a non-blocking overlay mounted by a shell
    // request. The component owns the
    // sidebar state (query, cursor, scroll, type_filter, loading, results)
    // and the 300 ms debounce (driven by `UserEvent::Clock`); the shell owns
    // the Emby client and spawns the search thread (design D4/D5).

    fn search_id() -> ComponentId {
        ComponentId::Overlay(OverlayId::Search)
    }

    /// Render the Search overlay if mounted.
    pub(in crate::app) fn render_search_overlay(&mut self, f: &mut ratatui::Frame) {
        let id = Self::search_id();
        if !self.application.mounted(&id) {
            return;
        }
        self.application.view(&id, f, f.area());
    }

    /// Drain search results from `search_rx` into the `SearchSidebarComponent`
    /// via downcast. The shell owns the channel; the component owns the state.
    pub(in crate::app) fn drain_search_results(&mut self) -> bool {
        let id = Self::search_id();
        let mut received = 0;
        while let Ok((query, result)) = self.app.search_rx.try_recv() {
            received += 1;
            if let Some(comp) = self.application.get_component_mut(&id) {
                if let Some(search) = comp.as_any_mut().downcast_mut::<SearchSidebarComponent>() {
                    search.apply_drain(&query, result);
                }
            }
        }
        received > 0
    }

    /// Sweep the search debounce deadline from the shell side. Production
    /// never wired a `UserEvent::Clock` publisher (#609), so the shell
    /// supplies wall-clock ticks directly via `tick_clock` on the mounted
    /// component. Returns the `Msg` the component emits when its deadline
    /// passes, or `None` if the search isn't mounted or the deadline hasn't
    /// fired yet. Callers forward `Some(msg)` through the same `Msg`
    /// router the component path uses (`handle_terminal_message`,
    /// `handle_service_request`).
    pub(in crate::app) fn tick_search_clock(&mut self, now: std::time::Instant) -> Option<Msg> {
        let id = Self::search_id();
        if !self.application.mounted(&id) {
            return None;
        }
        let comp = self.application.get_component_mut(&id)?;
        let search = comp.as_any_mut().downcast_mut::<SearchSidebarComponent>()?;
        search.tick_clock(now)
    }

    // --- Sessions sidebar ---------------------------------------------------

    fn sessions_id() -> ComponentId {
        ComponentId::Overlay(OverlayId::Sessions)
    }

    /// Render Sessions from shell-owned runtime data. Cursor, scroll, and hit
    /// geometry remain private to the mounted component.
    pub(in crate::app) fn render_sessions_overlay(&mut self, f: &mut ratatui::Frame) {
        let id = Self::sessions_id();
        if !self.application.mounted(&id) {
            return;
        }
        self.application.view(&id, f, f.area());
    }
}
