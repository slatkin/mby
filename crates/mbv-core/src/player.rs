use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use crate::api::{EmbyClient, EmbyItem, TICKS_PER_SECOND};
use crate::id_types::{EmbySessionId, ItemId, MediaSourceId};
#[cfg(test)]
use crate::playback_queue::QueueMutationResult;
use crate::playback_queue::{PlaybackQueue, QueueItem, QueueSlotId};
use libmpv2::{
    events::{Event, PropertyData},
    mpv_end_file_reason, EndFileReason, Format, Mpv,
};

fn mpv_err_str(e: &libmpv2::Error) -> String {
    if let libmpv2::Error::Raw(code) = e {
        format!("Raw({}) [{}]", code, libmpv2_sys::mpv_error_str(*code))
    } else {
        format!("{e:?}")
    }
}

fn mpv_title_opt(title: &str) -> String {
    // Use mpv's %N% length-prefix format so the value is passed verbatim —
    // no escaping needed, handles commas, backslashes, and any other character.
    format!("force-media-title=%{}%{}", title.len(), title)
}

fn resume_start_pos(item: &QueueItem) -> f64 {
    match item {
        QueueItem::Emby(emby) if !emby.is_audio() && emby.should_resume() => emby.resume_seconds(),
        QueueItem::Emby(_) => 0.0,
        QueueItem::Feed(entry) => {
            let runtime = entry.duration_ticks.unwrap_or(0) as i64;
            if crate::api::should_resume(entry.position_ticks, runtime) {
                entry.position_ticks as f64 / crate::api::TICKS_PER_SECOND as f64
            } else {
                0.0
            }
        }
        QueueItem::Audiobookshelf(ep) => {
            let runtime = ep.duration_ticks.unwrap_or(0) as i64;
            if crate::api::should_resume(ep.position_ticks, runtime) {
                ep.position_ticks as f64 / crate::api::TICKS_PER_SECOND as f64
            } else {
                0.0
            }
        }
        QueueItem::AudiobookshelfBook(book) => {
            let runtime = book.duration_ticks.unwrap_or(0) as i64;
            if crate::api::should_resume(book.position_ticks, runtime) {
                book.position_ticks as f64 / crate::api::TICKS_PER_SECOND as f64
            } else {
                0.0
            }
        }
    }
}

fn mpv_load_opts(item: &QueueItem) -> String {
    let title = mpv_title_opt(&item.display_name());
    let start = resume_start_pos(item);
    if start > 0.0 {
        format!("{title},start={start}")
    } else {
        title
    }
}

fn queue_load_indices(len: usize, start_idx: usize) -> impl Iterator<Item = usize> {
    std::iter::once(start_idx)
        .chain(0..start_idx)
        .chain(start_idx + 1..len)
}

fn queue_load_location(index: usize, start_idx: usize) -> (&'static str, String) {
    if index == start_idx {
        ("replace", "-1".to_string())
    } else if index < start_idx {
        ("insert-at", index.to_string())
    } else {
        ("append", "-1".to_string())
    }
}

fn send_ep_info(mpv: &Mpv, item: &crate::api::EmbyItem) {
    let val =
        if item.item_type == "Episode" && item.parent_index_number > 0 && item.index_number > 0 {
            format!(
                "Season {}  Episode {}",
                item.parent_index_number, item.index_number
            )
        } else {
            String::new()
        };
    let _ = mpv.set_property("user-data/mbv/ep-tag", val.as_str());
}

include!("player_types.rs");
include!("player_sources.rs");
include!("player_runtime.rs");
include!("player_report_worker.rs");
include!("player_reporting.rs");
include!("player_run_state.rs");
include!("player_run_types.rs");
include!("player_run_queue.rs");
include!("player_run_commands.rs");
include!("player_run_events.rs");
include!("player_run_run.rs");
include!("player_runtime_controller.rs");
include!("player_proxy.rs");

#[cfg(test)]
mod tests {
    include!("player_tests_basic.rs");
    include!("player_tests_session.rs");
    include!("player_tests_session_feed.rs");
    include!("player_tests_status.rs");
    include!("player_tests_submit.rs");
    include!("player_tests_active_file.rs");
    include!("player_proxy_tests.rs");
}
