pub(in crate::app) mod arrangements;
pub(in crate::app) mod components;
mod screens;
mod theme;

// Render-seam re-exports for Interactive Components (design D9): the free
// functions extracted from `impl App` methods are `pub(in crate::app)` inside
// `render::components::help`, but the `components` module is private. Re-export
// them here so `crate::app::components::help` can import them without widening
// the whole `render::components` module.
pub(in crate::app) use components::album_art::MusicImagePaint;
pub(in crate::app) use components::artwork_placeholder::render_artwork_placeholder;
pub(in crate::app) use components::audiobookshelf_book::{
    render_audiobookshelf_book_content, AudiobookshelfBookGeometry, BookInteraction,
};
pub(in crate::app) use components::audiobookshelf_podcast::{
    render_audiobookshelf_podcast_content, AudiobookshelfPodcastGeometry, PodcastInteraction,
};
#[allow(unused_imports)]
pub(in crate::app) use components::chrome_player::{
    render_player_panel, render_title_row, PlaybackRenderContext,
};
pub(in crate::app) use components::confirm_modal::render_confirm_modal_content;
pub(in crate::app) use components::context_menu::render_context_menu_content;
pub(in crate::app) use components::daemon_lost_modal::render_daemon_lost_modal_content;
#[allow(unused_imports)]
pub(in crate::app) use components::detail::{
    compact_banner_layout, render_compact_detail_with_ctx, CompactBannerLayout, CompactDetailCtx,
};
pub(in crate::app) use components::feeds::{render_feeds_content, FeedsRenderModel};
pub(in crate::app) use components::feeds_manage::{
    render_feeds_manage_content, FeedsManageRenderModel,
};
pub(in crate::app) use components::help::{help_destination, render_help_panel, HelpDestination};
pub(in crate::app) use components::home::render_home_content;
pub(in crate::app) use components::home_hero::HomeImagePaint;
pub(in crate::app) use components::inline_search::render_inline_search;
pub(in crate::app) use components::library_routes::{
    render_library_routes_content, save_route_config, LibraryRoutesRenderModel,
};
pub(in crate::app) use components::list::render_generic_movies_home_video_rows_with_ctx;
pub(in crate::app) use components::list_narrow::{
    paint_feed_group_pills_row, render_narrow_browse_with_ctx,
};
pub(in crate::app) use components::list_rows::LibraryListRenderCtx;
pub(in crate::app) use screens::feeds_model::{
    current_time_secs, feed_display_rows, feed_duration_text, FeedDisplayRow,
};
// Task 5.3d.17a: BrowserComponent paints the wide Movies/home-video
// Wide hero layout itself (mirroring HomeComponent's image-deferral),
// so the legacy wide renderer can be deleted in 5.3d.17b. Re-export the
// shared helpers it needs at crate::app visibility.
pub(in crate::app) use arrangements::library::wide_library_panes;
pub(in crate::app) use arrangements::padded_rect;
pub(in crate::app) use arrangements::wide_hero::{
    wide_hero_browser_border, wide_hero_browser_pane, wide_hero_hero_pane, wide_hero_presentation,
    LeftPaneFocus, PANE_PAD_X, PANE_PAD_Y,
};
pub(in crate::app) use components::hero::render_search_box;
pub(in crate::app) use components::home_hero::{
    prepare_wide_emby_hero_card, render_home_hero_content, HeroData,
};
// `LetterFilter` is already `pub(crate)` re-exported below (screens::sort_filter).
pub(in crate::app) use components::media_list::render_wide_media_list;
pub(in crate::app) use components::multiselect::{
    render_multiselect_content, MultiSelectRenderModel,
};
pub(in crate::app) use components::music_wide::{
    render_narrow_music_group_with_ctx, render_wide_music_group_with_ctx, MusicWideRenderCtx,
};
pub(in crate::app) use components::playlists::{
    render_playlists_content, render_save_playlist_content, PlaylistsRenderGeometry,
    PlaylistsViewState,
};
pub(in crate::app) use components::queue::{
    render_queue_title_content, QueueRenderGeometry, QueueTitleModel,
};
pub(in crate::app) use components::remote_reanchor::render_remote_reanchor_popup_content;
pub(in crate::app) use components::search_sidebar::render_search_sidebar;
pub(in crate::app) use components::selection_modal::{
    render_selection_modal_content, SelectionModalRenderModel,
};
pub(in crate::app) use components::sessions::render_sessions_overlay_content;
pub(in crate::app) use components::settings_component::{
    render_settings_content, SettingsRenderGeometry, SettingsRenderModel,
};
pub(in crate::app) use components::tv_wide::{render_wide_tv_with_ctx, TvWideRenderCtx};
pub(in crate::app) use components::widgets::{render_count_label, render_pill_bar, PillBar};
// Render-seam re-exports (design D9, task 3.1): the panel shell/scrollbar/row
// free functions extracted from `impl App` in `chrome.rs`. Used by the
// Interactive Components in `crate::app::components` (task 3.2+).
pub(in crate::app) use components::chrome::{render_panel_shell_at, render_sidebar_scrollbar};

// Re-exports so paths that resolved at `render::X` (or, from render's other
// submodules, `super::X`) before the render/mod.rs split (issue #365 step 2,
// lane C) keep resolving without editing any call site. The `pub(crate)`
// trio is referenced from outside `render` entirely (src/app/mod.rs,
// src/app/actions.rs); the rest are referenced via `super::X` from render's
// sibling submodules (album, card, detail, home, list, music, pills, queue)
// and/or `use super::*` in render/tests.rs.
pub use components::indicators;
use components::widgets::{
    render_placeholder, render_right_scrollbar, render_selected_block_background,
    MUSIC_ALBUM_IMAGE_TYPES, RENDER_FILTER,
};
pub(in crate::app) use components::widgets::{
    render_selected_block_borders, SelectedBlockBorderStyle,
};
pub(super) use screens::album_plan::sorted_group_album_order;
pub(super) use screens::sort_filter::{
    effective_sort_str, letter_bucket, parse_album_folder_name, strip_article,
};
pub(crate) use screens::sort_filter::{
    initial_group_artist_sort_key, LetterFilter, LIBRARY_PILL_THRESHOLD,
};
// `theme`'s roles are re-exported here (rather than reached directly) so
// `palette.rs` — a sibling of `render`, not a descendant — can bridge to them;
// see `palette.rs`'s own re-export.
pub(crate) use theme::{
    list_selected_row_bg, resolve_surface_focus, ACCENT, ACCENT_ACTIVE, ACCENT_AUDIOBOOKSHELF,
    BORDER_UNFOCUSED, INDICATOR_AUDIO_FG, INDICATOR_RESOLUTION_FG, PILL_BG, PILL_FG,
    PILL_OVERFLOW_FG, PILL_ROW_BG, PILL_SELECTED_BG, PILL_SELECTED_FG, PLAYBACK_META_FG,
    PLAYBACK_VALUE_FG, PROGRESS_TRACK, SCROLLBAR, STATUS_AVAILABLE, STATUS_ERROR,
    SURFACE_ACCENT_SOFT, SURFACE_ARTWORK_PLACEHOLDER, SURFACE_BACKDROP, SURFACE_CHROME,
    SURFACE_FOCUSED, SURFACE_ITEM_FOCUSED, SURFACE_PLAYBACK, SURFACE_RESTING, SURFACE_SIDEBAR,
    SURFACE_STATUS_PILL, TEXT_ACCENT_MUTED, TEXT_DETAIL_META, TEXT_EMPHASIS, TEXT_FOCUS_ACCENT,
    TEXT_METADATA, TEXT_MUTED, TEXT_ON_ACCENT, TEXT_PRIMARY, TEXT_SECONDARY, TEXT_STRONG,
};

use super::ui_util::natural_sort_key;
use super::{palette, App};

// Test-only: these names are otherwise unused in the production build (their
// only production callers moved into root.rs/queue.rs under screens/, which
// import them directly), but render/tests.rs and friends still reach them via
// `use super::*`.
#[cfg(test)]
use crate::app::layout::LayoutMain;
#[cfg(test)]
use components::widgets::right_panel_content_area;
#[cfg(test)]
use mbv_core::api::TICKS_PER_SECOND;
#[cfg(test)]
use ratatui::layout::Rect;
#[cfg(test)]
use unicode_width::UnicodeWidthStr;

#[cfg(test)]
#[path = "tests_album_focus.rs"]
mod album_focus_tests;
#[cfg(test)]
#[path = "tests_confirm_modal.rs"]
mod confirm_modal_tests;
#[cfg(test)]
#[path = "tests_context_menu.rs"]
mod context_menu_tests;
#[cfg(test)]
#[path = "tests_daemon_lost_modal.rs"]
mod daemon_lost_modal_tests;
#[cfg(test)]
#[path = "tests_feeds_manage_popup.rs"]
mod feeds_manage_popup_tests;
#[cfg(test)]
#[path = "tests_help.rs"]
mod help_tests;
#[cfg(test)]
#[path = "tests_home_characterization.rs"]
mod home_characterization_tests;
#[cfg(test)]
#[path = "tests_library_characterization.rs"]
mod library_characterization_tests;
#[cfg(test)]
#[path = "tests_library_routes_popup.rs"]
mod library_routes_popup_tests;
#[cfg(test)]
#[path = "tests_multiselect.rs"]
mod multiselect_tests;
#[cfg(test)]
#[path = "tests_music_characterization.rs"]
mod music_characterization_tests;
#[cfg(test)]
#[path = "tests_music_groups.rs"]
mod music_group_tests;
#[cfg(test)]
#[path = "tests_music_narrow.rs"]
mod music_narrow_tests;
#[cfg(test)]
#[path = "tests_music_wide_reanchor_characterization.rs"]
mod music_wide_reanchor_characterization_tests;
#[cfg(test)]
#[path = "tests_music_wide.rs"]
mod music_wide_tests;
#[cfg(test)]
#[path = "tests_non_music.rs"]
mod non_music_tests;
#[cfg(test)]
#[path = "tests_panel.rs"]
mod panel_tests;
#[cfg(test)]
#[path = "tests_playlists.rs"]
mod playlists_tests;
#[cfg(test)]
#[path = "tests_queue.rs"]
mod queue_tests;
#[cfg(test)]
#[path = "tests_remote_reanchor.rs"]
mod remote_reanchor_tests;
#[cfg(test)]
#[path = "tests_scroll_pills.rs"]
mod scroll_pills_tests;
#[cfg(test)]
#[path = "tests_search_sidebar.rs"]
mod search_sidebar_tests;
#[cfg(test)]
#[path = "tests_sessions.rs"]
mod sessions_tests;
#[cfg(test)]
#[path = "test_helpers.rs"]
mod test_helpers;
#[cfg(test)]
pub(crate) use test_helpers::{
    make_large_movie_library_app, make_movie_app, make_music_group_app, make_queue_app,
};
#[cfg(test)]
#[path = "queue_title_characterization_tests.rs"]
mod queue_title_characterization_tests;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
#[cfg(test)]
mod tests_audiobookshelf_books;
#[cfg(test)]
mod tests_audiobookshelf_podcasts;
#[cfg(test)]
mod tests_conformance_matrix;
#[cfg(test)]
mod tests_feeds;
#[cfg(test)]
mod tests_home_inline;
#[cfg(test)]
mod tests_selection_modal;
#[cfg(test)]
mod tests_wide_hero_pane_characterization;
