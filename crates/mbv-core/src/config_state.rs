// Queue, library-position, and last-remote-connection state persistence.
// Included via `include!("config_state.rs")` in config.rs, so all items
// share the same module scope as the other config_*.rs files.

pub fn save_queue_state(state: &QueueState) -> Result<(), String> {
    let path = queue_state_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("create directory {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string(state).map_err(|e| format!("serialize queue state: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| format!("rename {} to {}: {e}", tmp.display(), path.display()))
}

pub fn load_queue_state() -> Option<QueueState> {
    let text = std::fs::read_to_string(queue_state_path()).ok()?;
    match serde_json::from_str(&text) {
        Ok(state) => Some(state),
        Err(e) => {
            log::warn!(target: "queue", "queue_state.json failed to parse, queue not restored: {e}");
            None
        }
    }
}

pub fn clear_queue_state() -> Result<(), String> {
    let path = queue_state_path();
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("remove {}: {error}", path.display())),
    }
}

/// Which remote connection (if any) was active when mbv last exited
/// (issue #236). `App::teardown` writes this; `App::new` reads it back at
/// the next launch when `Config.auto_reconnect` is true. The two
/// variants mirror `App`'s own separate `active_route` (#223 library
/// routing) and `connected_session_id`/`connected_session_state`
/// (Sessions-panel direct-remote/attached) fields -- #222 and #223 were
/// distinct features and stay distinct here, even though both are
/// restored under the same on/off switch.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind")]
pub enum LastRemoteConnection {
    /// A #223 library route, keyed by the library name that was resolved
    /// active (`App.active_route`). Re-resolved fresh against current
    /// `library_routes` at startup, not replayed verbatim -- if the config
    /// changed since the last exit, the new config wins.
    LibraryRoute { library: String },
    /// A Sessions-panel direct-remote or attached session, keyed by the
    /// other device's name (`SessionInfo.device_name`), not its session id
    /// -- Emby session ids are ephemeral per-connection and would not
    /// still identify the same device at the next launch.
    DirectSession { device_name: String },
}

fn last_remote_connection_path() -> PathBuf {
    state_dir().join("last_remote_connection.json")
}

/// Persists (or, given `None`, clears) the connection active at exit.
/// Called from `App::teardown` only when `auto_reconnect` is
/// enabled -- when the feature is off, this file is never written or
/// read, by design (Task 1's `Global Constraints`).
fn save_last_remote_connection_at(
    path: &std::path::Path,
    conn: Option<&LastRemoteConnection>,
) -> Result<(), String> {
    let Some(conn) = conn else {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("remove {}: {e}", path.display())),
        };
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("create directory {}: {e}", dir.display()))?;
    }
    let json =
        serde_json::to_string(conn).map_err(|e| format!("serialize {}: {e}", path.display()))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| format!("rename {} to {}: {e}", tmp.display(), path.display()))
}

pub fn save_last_remote_connection(conn: Option<&LastRemoteConnection>) -> Result<(), String> {
    save_last_remote_connection_at(&last_remote_connection_path(), conn)
}

fn load_last_remote_connection_at(
    path: &std::path::Path,
) -> Result<Option<LastRemoteConnection>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    match serde_json::from_str(&text) {
        Ok(conn) => Ok(Some(conn)),
        Err(e) => {
            std::fs::remove_file(path).map_err(|remove_error| {
                format!(
                    "parse {}: {e}; remove corrupt {}: {remove_error}",
                    path.display(),
                    path.display()
                )
            })?;
            Err(format!(
                "parse {}: {e}; corrupt file removed",
                path.display()
            ))
        }
    }
}

pub fn load_last_remote_connection() -> Result<Option<LastRemoteConnection>, String> {
    load_last_remote_connection_at(&last_remote_connection_path())
}

/// Path for the cast receiver identifier attached at exit (task 7.2).
/// Deliberately its own file rather than a `LastRemoteConnection` variant:
/// `App::attach_cast` never touches `active_route`/`connected_session_id`,
/// so a cast receiver can be attached alongside -- or instead of -- an Emby
/// remote session, and the two attachments can't share one on/off record.
/// Gated on `Config.auto_reconnect` in `App::teardown` the same #236
/// switch `LastRemoteConnection` uses, but (unlike `LastRemoteConnection`)
/// not on `launched_as_remote`/`home_is_local_daemon`: cast discovery runs
/// on whatever machine the mbv process itself is on, independent of which
/// daemon its player talks to, so there is no "this launch doesn't own the
/// record" case to skip.
fn last_cast_receiver_path() -> PathBuf {
    state_dir().join("last_cast_receiver.json")
}

/// Persists (or, given `None`, clears) the cast receiver id attached at
/// exit.
pub fn save_last_cast_receiver(receiver_id: Option<&str>) -> Result<(), String> {
    let path = last_cast_receiver_path();
    let Some(receiver_id) = receiver_id else {
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("remove {}: {e}", path.display())),
        };
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("create directory {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string(receiver_id)
        .map_err(|e| format!("serialize {}: {e}", path.display()))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| format!("rename {} to {}: {e}", tmp.display(), path.display()))
}

pub fn load_last_cast_receiver() -> Result<Option<String>, String> {
    let path = last_cast_receiver_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    match serde_json::from_str(&text) {
        Ok(id) => Ok(Some(id)),
        Err(e) => {
            std::fs::remove_file(&path).map_err(|remove_error| {
                format!(
                    "parse {}: {e}; remove corrupt {}: {remove_error}",
                    path.display(),
                    path.display()
                )
            })?;
            Err(format!(
                "parse {}: {e}; corrupt file removed",
                path.display()
            ))
        }
    }
}

pub fn save_library_position_state(state: &LibraryPositionState) {
    let _ = save_library_position_state_result(state);
}

pub fn save_library_position_state_result(state: &LibraryPositionState) -> Result<(), String> {
    let path = library_position_state_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|error| format!("create directory {}: {error}", dir.display()))?;
    }
    let json =
        serde_json::to_string(state).map_err(|error| format!("serialize positions: {error}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json).map_err(|error| format!("write {}: {error}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .map_err(|error| format!("rename {} to {}: {error}", tmp.display(), path.display()))
}

pub fn load_library_position_state() -> LibraryPositionState {
    let text = match std::fs::read_to_string(library_position_state_path()) {
        Ok(text) => text,
        Err(_) => return LibraryPositionState::default(),
    };
    match serde_json::from_str(&text) {
        Ok(state) => state,
        Err(e) => {
            log::warn!(target: "library_position", "library_position_state.json failed to parse: {e}");
            LibraryPositionState::default()
        }
    }
}

impl QueueState {
    fn without_items<F>(&self, keep: F) -> Self
    where
        F: Fn(&crate::playback_queue::QueueItem) -> bool,
    {
        let items: Vec<crate::playback_queue::QueueItem> = self
            .items
            .iter()
            .filter(|item| keep(item))
            .cloned()
            .collect();
        let positions = self
            .positions
            .iter()
            .filter(|(id, _)| items.iter().any(|item| item.id() == **id))
            .map(|(id, position)| (id.clone(), *position))
            .collect();
        Self {
            source: if items.is_empty() {
                QueueSource::Unknown
            } else {
                self.source.clone()
            },
            cursor: self.cursor.min(items.len().saturating_sub(1)),
            last_played_content_id: self
                .last_played_content_id
                .as_ref()
                .filter(|id| items.iter().any(|item| item.content_id() == **id))
                .cloned(),
            last_played_item_id: self
                .last_played_item_id
                .as_ref()
                .filter(|id| items.iter().any(|item| item.id() == **id))
                .cloned(),
            last_played_completed: self.last_played_completed && !items.is_empty(),
            items,
            positions,
        }
    }

    /// Remove only Emby slots and native-ID keyed positions. Feed and
    /// Audiobookshelf snapshots remain intact for mixed queue restoration.
    /// After this change Emby removal preserves non-Emby items (Feed +
    /// Audiobookshelf) as required by the Audiobookshelf lifecycle.
    pub fn without_emby(&self) -> Self {
        self.without_items(|item| !matches!(item, crate::playback_queue::QueueItem::Emby(_)))
    }

    /// Remove only Audiobookshelf slots (both episode and book shapes) and
    /// their keyed positions. Emby and Feed items remain intact. Used on
    /// confirmed Audiobookshelf Service replacement/removal to purge
    /// Service-owned queue state without affecting other Services.
    pub fn without_audiobookshelf(&self) -> Self {
        self.without_items(|item| !item.is_audiobookshelf_any())
    }
}
