#[cfg(test)]
use crate::app::palette;
#[cfg(test)]
use crate::app::render::components::hero::{paint_hero_content, HeroContent};
use crate::app::ui_util::*;
use crate::app::{images, App};
#[cfg(test)]
use mbv_core::api::TICKS_PER_SECOND;
use mbv_core::playback_queue::QueueItem;
#[cfg(test)]
use ratatui::layout::*;
#[cfg(test)]
use ratatui::style::*;
#[cfg(test)]
use ratatui::text::*;
#[cfg(test)]
use ratatui::Frame;
use textwrap::wrap;

/// Pre-wrapped text for the generic Home hero detail (title lines, optional
/// show-name row, overview lines) plus the total row count the text needs.
/// Computed once (mirroring `KeepWatchingHeroLayout`'s measure-before-render
/// pattern) so `render_home_list` can size the narrow hero to its content
/// instead of asking for the whole content area, and the renderer can wrap
/// each piece exactly once per frame.
pub(in crate::app::render) struct HomeLatestDetailText {
    #[cfg(test)]
    pub(in crate::app::render) title_lines: Vec<String>,
    #[cfg(test)]
    pub(in crate::app::render) show_name: String,
    #[cfg(test)]
    pub(in crate::app::render) overview_lines: Vec<(String, bool)>,
    pub(in crate::app::render) meta_height: u16,
}

/// Measures/wraps the generic hero detail's text for `item` at the given
/// content width. `text_w` is where the title wraps; `ov_w` where the
/// overview wraps (`overview_pad` inset in the wide hero). The overview's
/// `false` flag is irrelevant here (no beside-image wrap-around in this
/// layout) but kept so the renderer can pass the lines straight through.
pub(in crate::app::render) fn home_latest_detail_text(
    item: &QueueItem,
    text_w: usize,
    ov_w: usize,
) -> HomeLatestDetailText {
    let title_lines: Vec<String> = wrap(item.title(), text_w)
        .into_iter()
        .map(|s| s.into_owned())
        .collect();
    let show_name = match item {
        QueueItem::Audiobookshelf(ep) => ep.show_title.clone().unwrap_or_default(),
        _ => String::new(),
    };
    let overview_lines: Vec<(String, bool)> = match item.overview() {
        None | Some("") => Vec::new(),
        Some(overview) => {
            // Strip URLs (an unbroken URL is one giant unbreakable word to
            // the wrapper below) and cap long descriptions, with an
            // ellipsis, matching the 600-char cap Emby's home-video/podcast
            // library views already use (`trunc_overview`) -- podcast
            // overviews routinely carry ad copy that would otherwise grow
            // the hero unboundedly.
            let capped = trunc_overview(overview);
            wrap(&capped, ov_w.max(1))
                .into_iter()
                .map(|s| (s.into_owned(), false))
                .collect()
        }
    };
    let meta_height = title_lines.len() as u16
        + if show_name.is_empty() { 0 } else { 1 }
        + 1 // meta row
        + 1 // blank separator
        + if overview_lines.is_empty() {
            0
        } else {
            1 + overview_lines.len() as u16 + 1 // overview block: pad + lines + pad
        };
    HomeLatestDetailText {
        #[cfg(test)]
        title_lines,
        #[cfg(test)]
        show_name,
        #[cfg(test)]
        overview_lines,
        meta_height,
    }
}

impl App {
    /// Triggers the Audiobookshelf cover fetch for `library_item_id` and
    /// returns its image cache key, or `None` with no server configured.
    /// Shared by every generic-hero cover: the two-column/Feed stacked-below
    /// detail (`render_home_latest_detail_content`) and the narrow beside-image
    /// hero (`home_hero.rs`'s `render_generic_hero_content`).
    pub(in crate::app::render) fn audiobookshelf_cover_key(
        &mut self,
        library_item_id: &str,
    ) -> Option<String> {
        let setup = self.config.lock().unwrap().audiobookshelf_setup.clone()?;
        if self.images_enabled() {
            self.fetch_audiobookshelf_cover(setup.server_url.clone(), library_item_id.to_string());
        }
        Some(images::audiobookshelf_cover_cache_key(
            &setup.server_url,
            library_item_id,
            self.current_protocol_suffix(),
        ))
    }

    /// Triggers the Audiobookshelf book-cover fetch for `library_item_id` and
    /// returns its isolated image cache key, or `None` with no server configured.
    /// The book-browsing spec requires book artwork to remain isolated from
    /// podcast artwork (line 124).
    pub(in crate::app::render) fn audiobookshelf_book_cover_key(
        &mut self,
        library_item_id: &str,
    ) -> Option<String> {
        let setup = self.config.lock().unwrap().audiobookshelf_setup.clone()?;
        if self.images_enabled() {
            self.fetch_audiobookshelf_book_cover(
                setup.server_url.clone(),
                library_item_id.to_string(),
            );
        }
        Some(images::audiobookshelf_book_cover_cache_key(
            &setup.server_url,
            library_item_id,
            self.current_protocol_suffix(),
        ))
    }
}

/// Renders the generic Home hero detail's non-image content (title/meta/
/// overview, or the whole no-image hero for a Feed) without `App`, returning
/// the Audiobookshelf cover's target `Rect` (if any) still needing paint.
/// Shared by Home's render paths (task 3.4's confirmed extraction).
#[cfg(test)]
pub(in crate::app::render) fn render_home_latest_detail_content(
    f: &mut Frame,
    area: Rect,
    item: &QueueItem,
    focused: bool,
    overview_pad: usize,
) -> Option<super::home_hero::HomeImagePaint> {
    if area.width == 0 || area.height == 0 {
        return None;
    }

    // Feed entries have no artwork; use the shared Model A no-image hero
    // rather than the legacy Home metadata painter. Audiobookshelf keeps
    // the beside-image path below because its cover is a wide thumbnail.
    if !matches!(item, QueueItem::Audiobookshelf(_)) {
        let hero: &dyn super::hero_model::Hero = item;
        let meta = item
            .duration()
            .map(|ticks| fmt_duration_short((ticks / TICKS_PER_SECOND as u64) as i64));
        let content = HeroContent {
            title: Some(hero.title()),
            meta_line: meta.as_deref(),
            meta_color: palette::TEXT_SECONDARY,
            show_playing: false,
            unconditional_spacer_after_meta: false,
            lines: &[],
            image: None,
        };
        paint_hero_content(f, area, &content, focused);
        return None;
    }

    let text_w = area.width as usize;
    let ov_w = text_w.saturating_sub(overview_pad * 2);
    let HomeLatestDetailText {
        title_lines,
        show_name,
        overview_lines,
        meta_height,
    } = home_latest_detail_text(item, text_w, ov_w);

    // Terminal cells are roughly twice as tall as they are wide, so a
    // 16:9 image needs 9 rows for every 32 columns, matching the Emby hero.
    let image_height = (area.width.saturating_mul(9).saturating_add(31) / 32)
        .max(1)
        .min(area.height.saturating_sub(meta_height + 1));
    let img_w = area.width;
    // The shared wide hero presentation puts artwork first, then a spacer,
    // then metadata. Keep the metadata in its own rect so the shell's
    // deferred image paint cannot overwrite it.
    let meta_area = Rect {
        x: area.x,
        y: area.y.saturating_add(image_height).saturating_add(1),
        width: area.width,
        height: area.height.saturating_sub(image_height.saturating_add(1)),
    };

    super::hero::render_home_hero_meta_block(
        f,
        meta_area,
        meta_area,
        &title_lines,
        &show_name,
        None,
        item.duration()
            .map(|ticks| {
                vec![vec![Span::styled(
                    trunc_str(
                        &fmt_duration_short((ticks / TICKS_PER_SECOND as u64) as i64),
                        text_w,
                    ),
                    Style::default().fg(palette::TEXT_SECONDARY),
                )]]
            })
            .unwrap_or_default(),
        &overview_lines,
        overview_pad as u16,
        focused,
    );

    // Cover art: only Audiobookshelf episodes carry artwork today.
    let QueueItem::Audiobookshelf(episode) = item else {
        return None;
    };
    if image_height == 0 {
        return None;
    }
    Some(super::home_hero::HomeImagePaint::AudiobookshelfCover {
        area: Rect {
            x: area.x,
            // Wide Wide hero reserves artwork above metadata.
            y: area.y,
            width: img_w,
            height: image_height,
        },
        library_item_id: episode.library_item_id.clone(),
        show_placeholder: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::render::test_helpers::buffer_to_string;
    use mbv_core::playback_queue::{AudiobookshelfQueueItem, FeedEntry};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn feed_item(id: &str) -> QueueItem {
        QueueItem::Feed(FeedEntry {
            guid: format!("guid-{id}"),
            title: format!("Feed entry {id}"),
            enclosure_url: None,
            link: None,
            mime_type: None,
            duration_ticks: None,
            pub_date_secs: None,
            feed_kind: Some(mbv_core::config::FeedKind::Audio),
            feed_id: None,
            position_ticks: 0,
            played: false,
        })
    }

    fn abs_item(id: &str, duration_ticks: Option<u64>, cover_path: Option<String>) -> QueueItem {
        QueueItem::Audiobookshelf(AudiobookshelfQueueItem {
            library_item_id: format!("show-{id}"),
            episode_id: format!("episode-{id}"),
            title: format!("Episode {id}"),
            show_title: Some("Podcast".into()),
            author: None,
            description: None,
            duration_ticks,
            position_ticks: 0,
            played: false,
            pub_date_secs: None,
            is_finished: false,
            cover_path,
        })
    }

    /// Task 10.2: the generic detail shows the title and, when known, the
    /// duration; a missing cover or unknown duration degrades gracefully
    /// rather than panicking or rendering an empty duration row.
    #[test]
    fn detail_shows_title_and_duration_when_known() {
        let item = abs_item(
            "a",
            Some(65 * TICKS_PER_SECOND as u64),
            Some("cover.jpg".into()),
        );
        let backend = TestBackend::new(40, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let _ = render_home_latest_detail_content(f, Rect::new(0, 0, 40, 6), &item, true, 0);
        })
        .unwrap();
        let out = buffer_to_string(&term);
        assert!(out.contains("Episode a"), "title row: {out:?}");
        assert!(out.contains("Podcast"), "show-name row: {out:?}");
        assert!(out.contains("1:05"), "duration row: {out:?}");
    }

    /// Task 10.2: detail with no known duration skips the duration row but
    /// still renders the title and show name; no configured server means no
    /// cover fetch.
    #[test]
    fn detail_without_duration_omits_duration_row() {
        let item = abs_item("b", None, None);
        let backend = TestBackend::new(40, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let _ = render_home_latest_detail_content(f, Rect::new(0, 0, 40, 6), &item, true, 0);
        })
        .unwrap();
        let out = buffer_to_string(&term);
        assert!(out.contains("Episode b"), "title row: {out:?}");
        assert!(out.contains("Podcast"), "show-name row: {out:?}");
        assert!(!out.contains("0:00"), "no fabricated duration: {out:?}");
    }

    #[test]
    fn detail_places_podcast_artwork_at_hero_top() {
        let item = abs_item("top", None, None);
        let backend = TestBackend::new(80, 30);
        let mut term = Terminal::new(backend).unwrap();
        let mut paint = None;
        term.draw(|f| {
            paint = render_home_latest_detail_content(f, Rect::new(3, 4, 40, 20), &item, true, 0);
        })
        .unwrap();
        match paint {
            Some(
                crate::app::render::components::home_hero::HomeImagePaint::AudiobookshelfCover {
                    area,
                    ..
                },
            ) => {
                assert_eq!(area.y, 4);
            }
            _ => panic!("expected podcast artwork paint"),
        }
    }

    /// Long ABS descriptions are capped at 600 display columns with an
    /// ellipsis so the hero doesn't grow unboundedly.
    #[test]
    fn detail_truncates_long_description_with_ellipsis() {
        // The truncation limit is on the description width; build a much wider
        // buffer item by item so the assertion below is about the ellipsis,
        // not about a coincidental line-wrap boundary.
        let long = "word ".repeat(160);
        let item = QueueItem::Audiobookshelf(AudiobookshelfQueueItem {
            library_item_id: "show-t".into(),
            episode_id: "episode-t".into(),
            title: "Episode t".into(),
            show_title: Some("Podcast".into()),
            author: None,
            description: Some(long),
            duration_ticks: None,
            position_ticks: 0,
            played: false,
            pub_date_secs: None,
            is_finished: false,
            cover_path: None,
        });
        let backend = TestBackend::new(200, 40);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let _ = render_home_latest_detail_content(f, Rect::new(0, 0, 200, 40), &item, true, 0);
        })
        .unwrap();
        let out = buffer_to_string(&term);
        // Reassemble the description block's visible lines after the
        // artwork-first layout: title, show name, and separator precede it.
        let lines: Vec<&str> = out.split('\n').collect();
        let title_row = lines
            .iter()
            .position(|line| line.contains("Episode t"))
            .expect("title row");
        let desc_region: String = lines
            .iter()
            .skip(title_row + 3)
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            desc_region.ends_with('\u{2026}'),
            "long description ends with an ellipsis: ...{desc_region:?}"
        );
        assert!(
            desc_region.chars().count() <= 601,
            "description column budget is 600 + ellipsis, got {} chars",
            desc_region.chars().count()
        );
    }

    /// The generic hero measure sizes the hero to its text content, not the
    /// available area: a text-only Feed measures a few rows (title + meta
    /// row + separator), and an ABS item with a show name adds its subtitle
    /// row. The narrow Home layout builds the hero height from this measure
    /// (plus the 16:9 cover slot for ABS), so a bare Feed no longer fills
    /// the whole content area.
    #[test]
    fn generic_detail_measure_sizes_to_text_content() {
        let feed = feed_item("1");
        let text = home_latest_detail_text(&feed, 100, 100);
        assert_eq!(text.meta_height, 3, "Feed hero = title + meta + separator");

        let abs = abs_item("1", None, None);
        let text = home_latest_detail_text(&abs, 100, 100);
        assert_eq!(
            text.meta_height, 4,
            "ABS hero = title + show name + meta + separator"
        );
        assert_eq!(text.show_name, "Podcast");
    }

    /// Task 14.3: the generic detail renders a Feed entry's title with no
    /// duration row and no cover (the cover-fetch branch only ever runs for
    /// Audiobookshelf items), never panicking.
    #[test]
    fn feed_detail_renders_title_without_duration_or_artwork() {
        let item = feed_item("2");
        let backend = TestBackend::new(40, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let _ = render_home_latest_detail_content(f, Rect::new(0, 0, 40, 6), &item, true, 0);
        })
        .unwrap();
        let out = buffer_to_string(&term);
        let lines: Vec<&str> = out.split('\n').collect();
        assert!(lines[0].contains("Feed entry 2"), "title row: {out:?}");
        assert!(
            lines[1..].iter().all(|l| !l.contains("0:00")),
            "no fabricated duration: {out:?}"
        );
        assert!(
            !out.contains("image"),
            "no image path reached for a Feed entry: {out:?}"
        );
    }
}
