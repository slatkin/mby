use crate::app::components::media_list::{
    InlineMediaBrowser, MediaKind, MediaListRow, MediaSemanticState, RowGeometry, ViewportAnchor,
    WideMediaList,
};
use crate::app::render::arrangements::padded_rect;
use crate::app::render::arrangements::wide_hero::{
    self, wide_hero_browser_border, wide_hero_browser_pane, PANE_PAD_X, PANE_PAD_Y,
};
use crate::app::render::components::detail_series_view::{
    SERIES_DETAIL_DIVIDER_ROWS, SERIES_DETAIL_EPISODE_ROWS_ESTIMATE,
    SERIES_DETAIL_TRAILING_BLANK_ROWS, SERIES_IMAGE_COLS, SERIES_IMAGE_ROWS,
};
use crate::app::render::components::hero::{
    inline_hero_text_width, selected_detail_shell, wrap_overview_lines, HeroContent, HeroImage,
    HeroLine, HERO_BLOCK_EXTRA_ROWS, HERO_TITLE_ROWS,
};
use crate::app::render::components::list_rows::SELECTED_BLOCK_SIDE_PADDING;
use crate::app::render::components::media_list::{
    render_inline_media_browser, render_wide_media_list,
};
use crate::app::render::{render_pill_bar, render_placeholder, HomeImagePaint, PillBar};
use crate::app::types_audiobookshelf_browse::{
    build_show_title_buckets, AudiobookshelfBrowseState, AudiobookshelfEpisodeFilter,
};
use mbv_core::audiobookshelf::AudiobookshelfShow;

/// Podcast hero content row budget, shared by the legacy `App` narrow
/// renderer and `AudiobookshelfPodcastComponent`'s narrow path so both admit
/// the same inline-detail height. Mirrors the prior
/// `App::audiobookshelf_hero_content_rows` behavior exactly: title row,
/// optional author row, blank before a nonempty description, wrapped
/// description (capped at four rows) using `wrap_overview_lines` +
/// `inline_hero_text_width` with the image dimensions, episode
/// divider/visible-or-estimated episode rows, trailing blank, and the
/// image-height minimum when images are enabled.
pub(in crate::app::render) fn podcast_hero_content_rows(
    state: &AudiobookshelfBrowseState,
    interaction: PodcastInteraction,
    width: u16,
    images_enabled: bool,
) -> u16 {
    let title_rows = HERO_TITLE_ROWS;
    let author_rows = state
        .selected_show()
        .and_then(|show| show.author.as_ref())
        .is_some() as u16;
    let mut rows = title_rows + author_rows;
    if let Some(description) = state
        .selected_show()
        .and_then(|show| show.description.as_deref())
        .filter(|description| !description.is_empty())
    {
        rows += 1;
        let (image_width, image_height) = if images_enabled {
            (SERIES_IMAGE_COLS, SERIES_IMAGE_ROWS)
        } else {
            (0, 0)
        };
        let description_start = title_rows + author_rows + 1;
        rows += wrap_overview_lines(description, |line| {
            let row = description_start + line as u16;
            inline_hero_text_width(width, image_width, image_height, row) as usize
        })
        .len()
        .min(4) as u16;
    }
    if interaction.episode_selection.is_some() {
        rows += 1 + SERIES_DETAIL_DIVIDER_ROWS as u16;
        rows += state
            .episodes
            .as_ref()
            .map(|_| state.visible_episodes(interaction.episode_filter).len())
            .unwrap_or(SERIES_DETAIL_EPISODE_ROWS_ESTIMATE) as u16;
    }
    rows += SERIES_DETAIL_TRAILING_BLANK_ROWS as u16;
    if images_enabled {
        rows = rows.max(SERIES_IMAGE_ROWS + 1);
    }
    rows
}
use crate::app::palette;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::Frame;

/// The component-owned interaction values the podcast renderer needs, passed
/// in rather than read off the projected content type
/// (split-browse-state-interaction-fields task 3.2).
#[derive(Clone, Copy)]
pub(in crate::app) struct PodcastInteraction {
    pub episode_filter: AudiobookshelfEpisodeFilter,
    pub episode_selection: Option<usize>,
}

/// Geometry painted by the podcast component. Input uses this same geometry,
/// so selector and show targets cannot drift from the rendered surface.
#[derive(Default)]
pub(in crate::app) struct AudiobookshelfPodcastGeometry {
    /// Column count used by the painted show grid and keyboard navigation.
    pub columns: usize,
    pub selector_tabs: Vec<(Rect, usize)>,
    pub show_rows: Vec<(Rect, usize)>,
    pub episode_rows: Vec<(Rect, usize)>,
    /// Painted list/browser area: the wide right panel, or the narrow content
    /// area below the pill bar. Mirrors the legacy `LayoutMain.left_area` so
    /// the shell can anchor overlays after render ownership moved to the
    /// component (task 5.3d.10c).
    pub list_area: Rect,
    /// Wide-only right panel rect; zero in the narrow layout.
    pub right_area: Rect,
    /// Hero rect the component painted (wide hero panel, or narrow
    /// inline-detail hero). Zero when no hero was painted.
    pub hero_area: Rect,
    /// Narrow-only inline hero rect; zero in the wide layout or when the
    /// inline hero was rejected. Equals `hero_area` when set.
    pub inline_hero_area: Rect,
    /// Selected-item rect the component painted (only the narrow inline hero
    /// shell today; `None` in the wide layout, which has no selected-item
    /// shell). Mirrors the legacy `LayoutMain.selected_item_rect`.
    pub selected_item_rect: Option<Rect>,
    /// Screen-row offset of the selected list row from the viewport top, for
    /// the `ViewportAnchor` read side (§2.5). `None` when nothing is
    /// selected/visible. Not a paint rect; consumed by the component only.
    pub selected_row_offset: Option<usize>,
}

/// Canonical row projection for the podcast show list: one selectable `Item`
/// per show, keyed by its stable `library_item_id`. Podcast shows carry no
/// in-list letter headings (the alphabetical buckets are a pill row) and no
/// played/active semantic state.
fn podcast_show_rows(shows: &[AudiobookshelfShow]) -> Vec<MediaListRow<String>> {
    shows
        .iter()
        .map(|show| MediaListRow::Item {
            target: show.library_item_id.clone(),
            primary: show.title.clone(),
            trailing: None,
            duration: None,
            kind: MediaKind::Collection,
            semantic_state: MediaSemanticState::Ordinary,
        })
        .collect()
}

/// Paints the shared alphabetical bucket pill row and records its hit targets.
fn paint_bucket_pills(
    frame: &mut Frame,
    pills_area: Rect,
    state: &AudiobookshelfBrowseState,
    geometry: &mut AudiobookshelfPodcastGeometry,
) {
    let buckets = build_show_title_buckets(&state.shows);
    let selected_bucket = buckets
        .iter()
        .position(|bucket| state.cursor() >= bucket.start && state.cursor() < bucket.end)
        .unwrap_or(0);
    let labels: Vec<String> = buckets.iter().map(|bucket| bucket.label.into()).collect();
    let ids: Vec<usize> = (0..labels.len()).collect();
    geometry.selector_tabs = render_pill_bar(
        frame,
        pills_area,
        PillBar {
            labels: &labels,
            ids: &ids,
            selected_pos: selected_bucket,
            prefix: Some(" \u{2318} "),
        },
    );
}

/// Rebuilds the mouse-compat `show_rows` map from the painted flow geometry:
/// each visible source row that resolves to a show index gets its screen rect.
/// Replacement/detail rows (no source row) are skipped, so a selected show
/// whose row the inline hero swallowed owns no `show_rows` entry.
fn push_show_rows(
    geo: &RowGeometry<String>,
    area: Rect,
    shows: &[AudiobookshelfShow],
    geometry: &mut AudiobookshelfPodcastGeometry,
) {
    let offset = geo.offset();
    let targets: Vec<Option<&String>> = geo.targets().collect();
    for (screen_row, flow_row) in (offset..geo.len()).take(area.height as usize).enumerate() {
        if geo.source_row(flow_row).is_none() {
            continue;
        }
        let Some(Some(id)) = targets.get(flow_row) else {
            continue;
        };
        let Some(index) = shows.iter().position(|show| &show.library_item_id == *id) else {
            continue;
        };
        geometry.show_rows.push((
            Rect {
                x: area.x,
                y: area.y + screen_row as u16,
                width: area.width,
                height: 1,
            },
            index,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::app) fn render_audiobookshelf_podcast_content(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    images_enabled: bool,
    state: &mut AudiobookshelfBrowseState,
    interaction: PodcastInteraction,
    scroll: &mut usize,
    narrow_list: &mut InlineMediaBrowser<String>,
    wide_episode_list: &mut WideMediaList<String>,
    flip_anchor: Option<&ViewportAnchor<String>>,
    geometry: &mut AudiobookshelfPodcastGeometry,
) -> Option<HomeImagePaint> {
    *geometry = AudiobookshelfPodcastGeometry::default();
    let Some(wide_hero::WideHeroPanes {
        hero: hero_panel,
        browser: right_panel,
    }) = wide_hero::wide_hero_presentation(area)
    else {
        return render_narrow_podcast(
            frame,
            area,
            focused,
            images_enabled,
            state,
            interaction,
            scroll,
            narrow_list,
            wide_episode_list,
            flip_anchor,
            geometry,
        );
    };

    // Wide layout: the list/browser occupies the right pane; the hero panel is
    // the painted hero. No inline hero and no selected-item shell exist here.
    geometry.columns = 1;
    geometry.list_area = right_panel;
    geometry.right_area = right_panel;
    let right_pane = wide_hero_browser_pane(right_panel, right_panel);
    if !state.shows.is_empty() {
        // Bucket pills before the hero so its wide-only episode-filter pills
        // append after them in `selector_tabs`.
        paint_bucket_pills(frame, right_pane.pills_area, state, geometry);
    }

    // Wide hero: fills and insets the left pane via the shared primitive
    // (D8: this surface gains focus-green when the episode workspace holds
    // focus, mirroring TV -- never a bare `focused`). Title lives in the
    // right show-list panel, so the hero body carries only
    // author/description/image. Persistent-mode episode pills + table are
    // wide-only.
    let hero_content_area = wide_hero::wide_hero_hero_pane(
        frame,
        area,
        wide_hero::LeftPaneFocus::Workspace(focused && interaction.episode_selection.is_some()),
    )
    .expect("wide branch already confirmed wide_hero_presentation fits");
    let image_paint = render_podcast_hero(
        frame,
        hero_content_area,
        state,
        interaction,
        focused,
        false,
        images_enabled,
        true,
        wide_episode_list,
        geometry,
    );
    if state.shows.is_empty() {
        render_placeholder(frame, right_panel, "No podcast shows");
        return image_paint;
    }
    if state.selected_show().is_some() {
        geometry.hero_area = hero_panel;
    }

    let list_panel = right_pane.list_panel;
    let content_area = padded_rect(list_panel, PANE_PAD_X, PANE_PAD_Y);
    if list_panel.height > 0 {
        frame.render_widget(
            Block::default().style(Style::default().bg(palette::resolve_surface_focus(focused))),
            list_panel,
        );
    }
    // Paint the rail frame before the rows: the border primitive rewrites every
    // panel cell background, so it must not run after the canonical list.
    wide_hero_browser_border(frame, list_panel, focused);

    let mut media: WideMediaList<String> = WideMediaList::new();
    media.set_content(podcast_show_rows(&state.shows));
    if let Some(id) = state.selected_id.as_ref() {
        media.select_target(id);
    }
    media.set_scroll(*scroll);
    if let Some(anchor) = flip_anchor {
        media.apply_viewport_anchor(anchor, content_area.height.max(1) as usize);
    }
    let paint_area = Rect {
        x: list_panel.x,
        width: list_panel.width,
        ..content_area
    };
    let paint = render_wide_media_list(
        frame,
        paint_area,
        content_area,
        &mut media,
        focused,
        palette::list_selected_row_bg(),
    );
    *scroll = media.scroll();
    geometry.selected_row_offset = paint
        .row_geometry
        .selected_row()
        .map(|row| row.saturating_sub(paint.row_geometry.offset()));
    push_show_rows(&paint.row_geometry, content_area, &state.shows, geometry);
    image_paint
}

#[allow(clippy::too_many_arguments)]
fn render_narrow_podcast(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    images_enabled: bool,
    state: &mut AudiobookshelfBrowseState,
    interaction: PodcastInteraction,
    scroll: &mut usize,
    narrow_list: &mut InlineMediaBrowser<String>,
    _wide_episode_list: &mut WideMediaList<String>,
    flip_anchor: Option<&ViewportAnchor<String>>,
    geometry: &mut AudiobookshelfPodcastGeometry,
) -> Option<HomeImagePaint> {
    geometry.columns = 1;
    if state.shows.is_empty() {
        render_placeholder(
            frame,
            area,
            state.error.as_deref().unwrap_or("No podcast shows"),
        );
        // Narrow with no shows: the whole area is the (empty) browser.
        geometry.list_area = area;
        return None;
    }
    let parts = wide_hero::pill_bar_areas(area);
    paint_bucket_pills(frame, parts.pills_area, state, geometry);

    let content_area = parts.content_area;
    geometry.list_area = content_area;

    narrow_list.set_content(podcast_show_rows(&state.shows));
    if let Some(id) = state.selected_id.as_ref() {
        narrow_list.select_target(id);
    }
    narrow_list.set_scroll(*scroll);
    let visible = content_area.height.max(1) as usize;
    if let Some(anchor) = flip_anchor {
        narrow_list.apply_viewport_anchor(anchor, visible);
    }

    let hero_content_width = content_area
        .width
        .saturating_sub(2 * SELECTED_BLOCK_SIDE_PADDING);
    let desired_detail_rows =
        podcast_hero_content_rows(state, interaction, hero_content_width, images_enabled) as usize
            + HERO_BLOCK_EXTRA_ROWS as usize;

    let result = render_inline_media_browser(
        frame,
        content_area,
        &*narrow_list,
        desired_detail_rows,
        focused,
        palette::list_selected_row_bg(),
    );
    let geo = &result.row_geometry;
    *scroll = geo.offset();
    narrow_list.set_scroll(geo.offset());
    geometry.selected_row_offset = narrow_list.selected_row_offset(visible);
    push_show_rows(geo, content_area, &state.shows, geometry);

    let Some(hero_area) = result.hero_area else {
        // Ordinary-row fallback: no inline hero, no selected-item shell.
        geometry.selected_item_rect = geo.selected_row_rect(content_area);
        return None;
    };
    selected_detail_shell(frame, hero_area, hero_area.height, focused);
    // Narrow inline hero admitted: the painted hero is both the inline hero and
    // the selected-item shell the shell anchors overlays to.
    geometry.hero_area = hero_area;
    geometry.inline_hero_area = hero_area;
    geometry.selected_item_rect = Some(hero_area);
    // Narrow inline hero: title is painted (the selected show row is replaced);
    // persistent-mode episode pills + table are suppressed.
    render_podcast_hero(
        frame,
        hero_area,
        state,
        interaction,
        focused,
        true,
        images_enabled,
        false,
        _wide_episode_list,
        geometry,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_podcast_hero(
    frame: &mut Frame,
    area: Rect,
    state: &AudiobookshelfBrowseState,
    interaction: PodcastInteraction,
    focused: bool,
    show_title: bool,
    images_enabled: bool,
    wide: bool,
    wide_episode_list: &mut WideMediaList<String>,
    geometry: &mut AudiobookshelfPodcastGeometry,
) -> Option<HomeImagePaint> {
    let show = state.selected_show()?;
    let mut lines = Vec::new();
    if let Some(author) = &show.author {
        lines.push(HeroLine::Plain(author.clone()));
    }
    if let Some(description) = &show.description {
        if !description.is_empty() {
            lines.push(HeroLine::Plain(String::new()));
            lines.push(HeroLine::Plain(description.clone()));
        }
    }
    lines.push(HeroLine::Plain(String::new()));
    // Wide: `area` is already the shared-inset content rect `wide_hero_hero_pane`
    // returned to the caller. Narrow keeps its own selected-item-shell inset.
    let content_area = if wide {
        area
    } else {
        Rect {
            x: area.x + SELECTED_BLOCK_SIDE_PADDING,
            y: area.y + SELECTED_BLOCK_SIDE_PADDING,
            width: area.width.saturating_sub(2 * SELECTED_BLOCK_SIDE_PADDING),
            height: area.height.saturating_sub(2 * SELECTED_BLOCK_SIDE_PADDING),
        }
    };
    // HeroImage is right-aligned by the painter; use the fixed cover width so
    // the full-width overview retains room for title and metadata.
    let image = images_enabled.then_some(HeroImage {
        actual_w: SERIES_IMAGE_COLS,
        height: SERIES_IMAGE_ROWS,
    });
    let result = crate::app::render::components::hero::paint_hero_content(
        frame,
        content_area,
        &HeroContent {
            title: show_title.then_some(show.title.as_str()),
            meta_line: None,
            meta_color: palette::TEXT_SECONDARY,
            show_playing: false,
            unconditional_spacer_after_meta: false,
            lines: &lines,
            image,
        },
        focused,
    );
    // Episode filter pills + table are wide-only (`persistent` legacy
    // gate); narrow routes Enter to the selection modal instead, so
    // `episode_selection` is never set in narrow in practice.
    if wide && interaction.episode_selection.is_some() && result.next_row < area.bottom() {
        let listing_area = Rect {
            y: result.next_row,
            height: area.bottom().saturating_sub(result.next_row),
            ..area
        };
        let (_, listing_content_area) = wide_hero::wide_hero_hero_content_box(frame, listing_area);
        let filter = interaction.episode_filter;
        let labels: Vec<String> = AudiobookshelfEpisodeFilter::ALL
            .iter()
            .map(|filter| filter.label().into())
            .collect();
        let ids: Vec<usize> = (0..labels.len()).collect();
        let tabs = render_pill_bar(
            frame,
            Rect {
                x: listing_content_area.x,
                y: listing_content_area.y,
                width: listing_content_area.width,
                height: listing_content_area.height.min(1),
            },
            PillBar {
                labels: &labels,
                ids: &ids,
                selected_pos: AudiobookshelfEpisodeFilter::ALL
                    .iter()
                    .position(|candidate| *candidate == filter)
                    .unwrap_or(0),
                prefix: Some(" ⌘ "),
            },
        );
        geometry.selector_tabs.extend(tabs);
        let row_y = listing_content_area.y + 1;
        let rows = state
            .visible_episodes(filter)
            .into_iter()
            .map(|episode| MediaListRow::Item {
                target: episode.episode_id.clone(),
                primary: episode.title.clone(),
                trailing: None,
                duration: None,
                kind: MediaKind::Media,
                semantic_state: MediaSemanticState::Ordinary,
            })
            .collect();
        wide_episode_list.set_content(rows);
        wide_episode_list.select_index(interaction.episode_selection.unwrap_or(0));
        let episode_area = Rect {
            x: listing_content_area.x,
            y: row_y,
            width: listing_content_area.width,
            height: area.bottom().saturating_sub(row_y),
        };
        let paint = render_wide_media_list(
            frame,
            episode_area,
            episode_area,
            wide_episode_list,
            focused,
            palette::list_selected_row_bg(),
        );
        geometry.episode_rows = paint
            .row_geometry
            .targets()
            .enumerate()
            .filter_map(|(row, target)| target.map(|_| (row, row)))
            .map(|(row, index)| {
                (
                    Rect {
                        x: episode_area.x,
                        y: episode_area.y + row as u16,
                        width: episode_area.width,
                        height: 1,
                    },
                    index,
                )
            })
            .collect();
    }
    (images_enabled && result.img_rect.is_some()).then(|| HomeImagePaint::AudiobookshelfCover {
        area: result.img_rect.unwrap(),
        library_item_id: show.library_item_id.clone(),
        show_placeholder: true,
    })
}

#[cfg(test)]
mod tests {
    use super::{podcast_hero_content_rows, PodcastInteraction};
    use crate::app::render::components::detail_series_view::{
        SERIES_DETAIL_DIVIDER_ROWS, SERIES_DETAIL_EPISODE_ROWS_ESTIMATE,
        SERIES_DETAIL_TRAILING_BLANK_ROWS, SERIES_IMAGE_COLS, SERIES_IMAGE_ROWS,
    };
    use crate::app::render::components::hero::{
        inline_hero_text_width, wrap_overview_lines, HERO_TITLE_ROWS,
    };
    use crate::app::types_audiobookshelf_browse::{
        AudiobookshelfBrowseState, AudiobookshelfEpisodeFilter,
    };
    use mbv_core::audiobookshelf::{
        AudiobookshelfDownloadedEpisode, AudiobookshelfLibrary, AudiobookshelfShow,
    };

    fn interaction(episode_selection: Option<usize>) -> PodcastInteraction {
        PodcastInteraction {
            episode_filter: AudiobookshelfEpisodeFilter::All,
            episode_selection,
        }
    }

    fn make_state(show: AudiobookshelfShow) -> AudiobookshelfBrowseState {
        let library = AudiobookshelfLibrary {
            id: "lib".into(),
            name: "Podcasts".into(),
            media_type: "podcast".into(),
        };
        let mut state = AudiobookshelfBrowseState::new(library);
        state.append_page(0, 20, 1, vec![show]);
        state.select(0);
        state
    }

    /// Independent oracle reproducing the pre-extraction
    /// `App::audiobookshelf_hero_content_rows` body, so the shared helper is
    /// proved equivalent to the legacy rule it replaces (author, long
    /// description, episode, and image cases all shift the budget exactly as
    /// before).
    fn legacy_hero_content_rows(
        state: &AudiobookshelfBrowseState,
        interaction: PodcastInteraction,
        width: u16,
        images_enabled: bool,
    ) -> u16 {
        let title_rows = HERO_TITLE_ROWS;
        let author_rows = state
            .selected_show()
            .and_then(|show| show.author.as_ref())
            .is_some() as u16;
        let mut rows = title_rows + author_rows;
        if let Some(description) = state
            .selected_show()
            .and_then(|show| show.description.as_deref())
            .filter(|description| !description.is_empty())
        {
            rows += 1;
            let (image_width, image_height) = if images_enabled {
                (SERIES_IMAGE_COLS, SERIES_IMAGE_ROWS)
            } else {
                (0, 0)
            };
            let description_start = title_rows + author_rows + 1;
            rows += wrap_overview_lines(description, |line| {
                let row = description_start + line as u16;
                inline_hero_text_width(width, image_width, image_height, row) as usize
            })
            .len()
            .min(4) as u16;
        }
        if interaction.episode_selection.is_some() {
            rows += 1 + SERIES_DETAIL_DIVIDER_ROWS as u16;
            rows += state
                .episodes
                .as_ref()
                .map(|_| state.visible_episodes(interaction.episode_filter).len())
                .unwrap_or(SERIES_DETAIL_EPISODE_ROWS_ESTIMATE) as u16;
        }
        rows += SERIES_DETAIL_TRAILING_BLANK_ROWS as u16;
        if images_enabled {
            rows = rows.max(SERIES_IMAGE_ROWS + 1);
        }
        rows
    }

    fn assert_matches_legacy(
        state: &AudiobookshelfBrowseState,
        interaction: PodcastInteraction,
        width: u16,
        images_enabled: bool,
    ) {
        let got = podcast_hero_content_rows(state, interaction, width, images_enabled);
        let expected = legacy_hero_content_rows(state, interaction, width, images_enabled);
        assert_eq!(got, expected, "shared helper must match legacy rule");
    }

    #[test]
    fn narrow_podcast_budget_matches_legacy_for_author_only() {
        let state = make_state(AudiobookshelfShow {
            library_item_id: "s".into(),
            title: "Show".into(),
            author: Some("Author".into()),
            description: None,
            cover_path: None,
        });
        // title(1) + author(1) + trailing(1) = 3; no image minimum.
        assert_eq!(
            podcast_hero_content_rows(&state, interaction(None), 40, false),
            3
        );
        assert_matches_legacy(&state, interaction(None), 40, false);
    }

    #[test]
    fn narrow_podcast_budget_matches_legacy_for_long_description() {
        let state = make_state(AudiobookshelfShow {
            library_item_id: "s".into(),
            title: "Show".into(),
            author: Some("Author".into()),
            description: Some("word ".repeat(80)),
            cover_path: None,
        });
        assert_matches_legacy(&state, interaction(None), 40, false);
    }

    #[test]
    fn narrow_podcast_budget_matches_legacy_for_episodes() {
        let mut state = make_state(AudiobookshelfShow {
            library_item_id: "s".into(),
            title: "Show".into(),
            author: None,
            description: None,
            cover_path: None,
        });
        state.episodes = Some(vec![
            AudiobookshelfDownloadedEpisode {
                library_item_id: "s".into(),
                episode_id: "e1".into(),
                title: "E1".into(),
                published_at: None,
                duration_seconds: None,
            },
            AudiobookshelfDownloadedEpisode {
                library_item_id: "s".into(),
                episode_id: "e2".into(),
                title: "E2".into(),
                published_at: None,
                duration_seconds: None,
            },
            AudiobookshelfDownloadedEpisode {
                library_item_id: "s".into(),
                episode_id: "e3".into(),
                title: "E3".into(),
                published_at: None,
                duration_seconds: None,
            },
        ]);
        assert_matches_legacy(&state, interaction(Some(0)), 40, false);
    }

    #[test]
    fn narrow_podcast_budget_matches_legacy_for_images_minimum() {
        let state = make_state(AudiobookshelfShow {
            library_item_id: "s".into(),
            title: "Show".into(),
            author: None,
            description: None,
            cover_path: None,
        });
        // Images enabled lifts even a title-only budget to SERIES_IMAGE_ROWS+1.
        assert_eq!(
            podcast_hero_content_rows(&state, interaction(None), 40, true),
            SERIES_IMAGE_ROWS + 1
        );
        assert_matches_legacy(&state, interaction(None), 40, true);
    }
}
