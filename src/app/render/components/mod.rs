pub(super) mod album_art;
pub(super) mod artwork_placeholder;

pub(super) mod album_detail;
#[cfg(test)]
#[path = "artwork_placeholder_tests.rs"]
mod artwork_placeholder_tests;
pub(super) mod audiobookshelf_book;
pub(super) mod audiobookshelf_books;

pub(super) mod audiobookshelf_podcast;
pub(super) mod backdrop;
pub(in crate::app) mod card;
pub(in crate::app) mod chrome;
pub(super) mod chrome_player;
pub(in crate::app) mod chrome_player_context;
pub(in crate::app) mod chrome_status;
pub(in crate::app) mod chrome_tabs;
pub(super) mod confirm_modal;
pub(super) mod context_menu;
pub(super) mod daemon_lost_modal;
pub(super) mod detail;
pub(super) mod detail_series_view;
pub(super) mod feeds;
pub(super) mod feeds_manage;
pub(super) mod help;
pub(super) mod hero;
pub(in crate::app) mod hero_model;
pub(super) mod home;
pub(super) mod home_feed;
pub(super) mod home_hero;
pub(super) mod home_hero_emby;
pub(super) mod home_latest_row;
pub(super) mod home_pills;
pub(super) mod home_video;
pub mod indicators;
pub(in crate::app) mod inline_search;
pub(super) mod library_routes;
pub(super) mod list;
pub(super) mod list_context;
pub(super) mod list_letter_groups;
pub(super) mod list_narrow;
pub(super) mod list_rows;
pub(super) mod media_list;
pub(super) mod modal_frame;
pub(super) mod multiselect;
pub(in crate::app) mod music;
pub(super) mod music_wide;
pub(super) mod music_wide_browser;
pub(super) mod playlists;
pub(in crate::app) mod queue;
pub(super) mod remote_reanchor;
pub(super) mod search_sidebar;
pub(super) mod selection_modal;
pub(super) mod sessions;
pub(super) mod settings;
pub(super) mod settings_component;
pub(super) mod tv_wide;
pub(super) mod visualizer;
pub(in crate::app) mod widgets;

#[cfg(test)]
#[path = "home_video_tests.rs"]
mod home_video_tests;

#[cfg(test)]
#[path = "hero_tests.rs"]
mod hero_tests;

#[cfg(test)]
#[path = "detail_series_tests.rs"]
mod detail_series_tests;

#[cfg(test)]
#[path = "list_narrow_tests.rs"]
mod list_narrow_tests;
