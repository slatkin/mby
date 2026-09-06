use crate::app::components::media_list::{
    InlineMediaBrowser, MediaKind, MediaListRow, MediaSemanticState, RowGeometry, ViewportAnchor,
    WideMediaList,
};
use crate::app::palette;
use crate::app::render::arrangements::library as library_arrangement;
use crate::app::render::arrangements::padded_rect;
use crate::app::render::arrangements::wide_hero::{
    self, wide_hero_browser_border, wide_hero_browser_pane, PANE_PAD_X, PANE_PAD_Y,
};
use crate::app::render::components::audiobookshelf_books::BookHeroPlan;
use crate::app::render::components::hero::{
    paint_hero_content, selected_detail_shell, wrap_overview_lines, HeroContent, HeroImage,
    HeroLine, HERO_BLOCK_EXTRA_ROWS, HERO_TITLE_ROWS,
};
use crate::app::render::components::list_rows::SELECTED_BLOCK_SIDE_PADDING;
use crate::app::render::components::media_list::{
    render_inline_media_browser, render_wide_media_list,
};
use crate::app::render::{render_pill_bar, render_placeholder, PillBar};
use crate::app::types_audiobookshelf_browse::{AudiobookshelfBookBrowseState, BookRow};
use crate::app::ui_util::{fmt_duration_approx, list_duration_secs};
use ratatui::layout::Rect;
use ratatui::style::Style;

use ratatui::widgets::Block;
use ratatui::Frame;

/// The component-owned interaction values the book renderer needs, passed in
/// rather than read off the projected content type
/// (split-browse-state-interaction-fields task 2.2).
#[derive(Clone, Copy)]
pub(in crate::app) struct BookInteraction {
    pub chapter_selection: Option<usize>,
    pub selected_bucket: usize,
}

#[derive(Default)]
pub(in crate::app) struct AudiobookshelfBookGeometry {
    pub selector_tabs: Vec<(Rect, usize)>,
    pub book_rows: Vec<(Rect, usize)>,
    pub chapter_rows: Vec<(Rect, usize)>,
    /// Painted book-list rect: the wide right-pane browser, or the narrow
    /// content area below the pill bar. Mirrors the legacy
    /// `LayoutMain.left_area` so `lib_page_size()` regains its real stride
    /// after render ownership moved to the component (2.1j).
    pub left_area: Rect,
    /// Whether the last painted presentation is the wide Wide hero
    /// layout (mirrors the legacy wide/narrow gate; the Enter activate
    /// decision now uses `App::is_right_panel_wide()` instead).
    pub wide: bool,
    /// Hero rect the component painted for the selected book (wide left pane,
    /// or narrow inline-detail flow). Mirrors the legacy `LayoutMain.hero_area`
    /// so conformance/context-menu readers keep working after render ownership
    /// moved to the component (task 5.3d.13).
    pub hero_area: Option<Rect>,
    /// Selected-item rect the component painted (the hero when one is shown, or
    /// the selected book row otherwise). Mirrors the legacy
    /// `LayoutMain.selected_item_rect`.
    pub selected_item_rect: Option<Rect>,
    /// Screen-row offset of the selected list row from the viewport top, for
    /// the `ViewportAnchor` read side (§2.5). `None` when nothing is
    /// selected/visible. Not a paint rect; consumed by the component only.
    pub selected_row_offset: Option<usize>,
}

/// Canonical row projection for the book catalog: one selectable `Item` per
/// book in the selected surname bucket, keyed by its stable `library_item_id`.
/// Books carry no in-list letter headings (the surname buckets are a pill row)
/// and no played/active semantic state (matching the legacy book rows).
fn book_rows(
    state: &AudiobookshelfBookBrowseState,
    selected_bucket: usize,
) -> Vec<MediaListRow<String>> {
    let Some(bucket) = state.buckets.get(selected_bucket).copied() else {
        return Vec::new();
    };
    state
        .books
        .get(bucket.start..bucket.end)
        .unwrap_or_default()
        .iter()
        .map(|book| MediaListRow::Item {
            target: book.library_item_id.clone(),
            primary: book.title.clone(),
            trailing: None,
            duration: None,
            kind: MediaKind::Collection,
            semantic_state: MediaSemanticState::Ordinary,
        })
        .collect()
}

/// Rebuilds the mouse-compat `book_rows` hit map from the painted flow
/// geometry: each visible source row that resolves to a book index gets its
/// screen rect. Replacement/detail rows (no source row) are skipped.
fn push_book_rows(
    geo: &RowGeometry<String>,
    area: Rect,
    state: &AudiobookshelfBookBrowseState,
    geometry: &mut AudiobookshelfBookGeometry,
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
        let Some(index) = state
            .books
            .iter()
            .position(|book| &book.library_item_id == *id)
        else {
            continue;
        };
        geometry.book_rows.push((
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
pub(in crate::app) fn render_audiobookshelf_book_content(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    state: &mut AudiobookshelfBookBrowseState,
    interaction: BookInteraction,
    images_enabled: bool,
    geometry: &mut AudiobookshelfBookGeometry,
    browser_offset: &mut usize,
    narrow_list: &mut InlineMediaBrowser<String>,
    chapter_list: &mut WideMediaList<String>,
    flip_anchor: Option<&ViewportAnchor<String>>,
) -> Option<super::home_hero::HomeImagePaint> {
    *geometry = AudiobookshelfBookGeometry::default();
    if state.books.is_empty() {
        render_placeholder(
            frame,
            area,
            state
                .error
                .as_deref()
                .unwrap_or(if state.loading_pages.is_empty() {
                    "No audiobooks"
                } else {
                    "Loading audiobooks…"
                }),
        );
        return None;
    }

    let plan = book_hero_plan(state, area.width, images_enabled);
    if wide_hero::wide_hero_presentation(area).is_some() {
        let panes = library_arrangement::wide_library_panes(area, 0, PANE_PAD_Y)?;
        geometry.left_area = panes.hero_area;
        geometry.wide = true;
        let hero_content_area = wide_hero::wide_hero_hero_pane(
            frame,
            area,
            wide_hero::LeftPaneFocus::Workspace(focused && interaction.chapter_selection.is_some()),
        )
        .expect("wide branch already confirmed wide_hero_presentation fits");
        let hero_height = (plan.content_rows + 1).min(hero_content_area.height);
        let hero_area = Rect {
            height: hero_height,
            ..hero_content_area
        };
        geometry.hero_area = Some(hero_area);
        let image = render_book_hero(
            frame,
            hero_area,
            state,
            focused && interaction.chapter_selection.is_some(),
            true,
            &plan,
        );
        let chapters_area = Rect {
            y: hero_content_area.y + hero_height,
            height: hero_content_area.height.saturating_sub(hero_height),
            ..hero_content_area
        };
        let (_, chapters_content_area) =
            wide_hero::wide_hero_hero_content_box(frame, chapters_area);
        render_book_rows(
            frame,
            chapters_content_area,
            state,
            interaction.chapter_selection,
            focused && interaction.chapter_selection.is_some(),
            chapter_list,
            geometry,
        );
        let rail_focused = focused && interaction.chapter_selection.is_none();
        let right_pane = wide_hero_browser_pane(panes.browser_panel, panes.browser_area);
        geometry.selector_tabs = render_book_pills(
            frame,
            right_pane.pills_area,
            state,
            interaction.selected_bucket,
        );
        let list_panel = right_pane.list_panel;
        let content_area = padded_rect(list_panel, PANE_PAD_X, PANE_PAD_Y);
        if list_panel.height > 0 {
            frame.render_widget(
                Block::default()
                    .style(Style::default().bg(palette::resolve_surface_focus(rail_focused))),
                list_panel,
            );
        }
        // Paint the rail frame before the rows: the border primitive rewrites
        // every panel cell background, so it must not run after the list.
        wide_hero_browser_border(frame, list_panel, rail_focused);

        let mut media: WideMediaList<String> = WideMediaList::new();
        media.set_content(book_rows(state, interaction.selected_bucket));
        if let Some(id) = state.selected_id.as_ref() {
            media.select_target(id);
        }
        media.set_scroll(*browser_offset);
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
            rail_focused,
            palette::list_selected_row_bg(),
        );
        *browser_offset = media.scroll();
        geometry.selected_row_offset = paint
            .row_geometry
            .selected_row()
            .map(|row| row.saturating_sub(paint.row_geometry.offset()));
        push_book_rows(&paint.row_geometry, content_area, state, geometry);
        // In the wide layout the selected book's hero (left pane) is the
        // selected item; record it so conformance/context-menu readers see the
        // same `selected_item_rect` the legacy renderer published.
        geometry.selected_item_rect = Some(hero_area);
        return image;
    }

    render_narrow_book(
        frame,
        area,
        focused,
        state,
        interaction,
        images_enabled,
        geometry,
        browser_offset,
        narrow_list,
        flip_anchor,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_narrow_book(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    state: &mut AudiobookshelfBookBrowseState,
    interaction: BookInteraction,
    images_enabled: bool,
    geometry: &mut AudiobookshelfBookGeometry,
    browser_offset: &mut usize,
    narrow_list: &mut InlineMediaBrowser<String>,
    flip_anchor: Option<&ViewportAnchor<String>>,
) -> Option<super::home_hero::HomeImagePaint> {
    let parts = wide_hero::pill_bar_areas(area);
    geometry.left_area = parts.content_area;
    geometry.wide = false;
    geometry.selector_tabs =
        render_book_pills(frame, parts.pills_area, state, interaction.selected_bucket);
    let content_area = parts.content_area;
    let plan = book_hero_plan(
        state,
        content_area
            .width
            .saturating_sub(SELECTED_BLOCK_SIDE_PADDING * 2),
        images_enabled,
    );

    narrow_list.set_content(book_rows(state, interaction.selected_bucket));
    if let Some(id) = state.selected_id.as_ref() {
        narrow_list.select_target(id);
    }
    narrow_list.set_scroll(*browser_offset);
    let visible = content_area.height.max(1) as usize;
    if let Some(anchor) = flip_anchor {
        narrow_list.apply_viewport_anchor(anchor, visible);
    }

    let desired_detail_rows = (plan.content_rows + HERO_BLOCK_EXTRA_ROWS) as usize;
    let result = render_inline_media_browser(
        frame,
        content_area,
        &*narrow_list,
        desired_detail_rows,
        focused,
        palette::list_selected_row_bg(),
    );
    let geo = &result.row_geometry;
    *browser_offset = geo.offset();
    narrow_list.set_scroll(geo.offset());
    geometry.selected_row_offset = narrow_list.selected_row_offset(visible);
    push_book_rows(geo, content_area, state, geometry);

    let Some(hero_area) = result.hero_area else {
        // Ordinary-row fallback: no inline hero, no selected-item shell.
        geometry.selected_item_rect = geo.selected_row_rect(content_area);
        return None;
    };
    selected_detail_shell(frame, hero_area, hero_area.height, focused);
    geometry.hero_area = Some(hero_area);
    geometry.selected_item_rect = Some(hero_area);
    render_book_hero(frame, hero_area, state, focused, true, &plan)
}

fn render_book_pills(
    frame: &mut Frame,
    area: Rect,
    state: &AudiobookshelfBookBrowseState,
    selected_bucket: usize,
) -> Vec<(Rect, usize)> {
    if state.buckets.is_empty() || area.width == 0 {
        return Vec::new();
    }
    let labels: Vec<String> = state
        .buckets
        .iter()
        .map(|bucket| bucket.label.into())
        .collect();
    let ids: Vec<usize> = (0..labels.len()).collect();
    render_pill_bar(
        frame,
        area,
        PillBar {
            labels: &labels,
            ids: &ids,
            selected_pos: selected_bucket.min(labels.len().saturating_sub(1)),
            prefix: Some(" ⌘ "),
        },
    )
}

fn render_book_hero(
    frame: &mut Frame,
    area: Rect,
    state: &AudiobookshelfBookBrowseState,
    focused: bool,
    show_title: bool,
    plan: &BookHeroPlan,
) -> Option<super::home_hero::HomeImagePaint> {
    let book = state.selected_book()?;
    let mut meta = Vec::new();
    if book.duration_seconds > 0.0 {
        meta.push(fmt_duration_approx(book.duration_seconds as i64));
    }
    if let Some(progress) = state.progress.get(&book.library_item_id) {
        meta.push(if progress.is_finished {
            "Finished".into()
        } else if progress.current_time_seconds > 0.0 && book.duration_seconds > 0.0 {
            format!(
                "{}%",
                ((progress.current_time_seconds * 100.0 / book.duration_seconds).floor() as u8)
                    .clamp(1, 99)
            )
        } else {
            "Not started".into()
        });
    }
    if let Some(narrator) = book.narrator.as_deref().filter(|value| !value.is_empty()) {
        meta.push(format!("Read by {narrator}"));
    }
    if let Some(year) = book
        .published_year
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        meta.push(year.into());
    }
    let overview = book
        .description
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(crate::app::ui_util::trunc_overview)
        .unwrap_or_default();
    let author = book.author_display.as_deref().unwrap_or("");
    let overview_start = HERO_TITLE_ROWS
        + usize::from(!author.is_empty()) as u16
        + usize::from(!meta.is_empty()) as u16 * 2;
    // HeroImage is right-aligned by the painter; keep its fixed cover width
    // separate from the full-width text area. The old slot calculation passed
    // the whole pane as the artwork width, leaving no room for text.
    let (image_width, image_height) = if plan.has_image {
        (plan.image_width, plan.image_height)
    } else {
        (0, 0)
    };
    let lines = wrap_overview_lines(&overview, |line| {
        crate::app::render::components::hero::inline_hero_text_width(
            area.width,
            image_width,
            image_height,
            overview_start + line as u16,
        ) as usize
    });
    let mut hero_lines = Vec::new();
    if !author.is_empty() {
        hero_lines.push(HeroLine::Plain(author.into()));
    }
    hero_lines.extend(lines.into_iter().map(HeroLine::Plain));
    let result = paint_hero_content(
        frame,
        Rect {
            x: area.x + SELECTED_BLOCK_SIDE_PADDING,
            y: area.y + SELECTED_BLOCK_SIDE_PADDING,
            width: area.width.saturating_sub(2 * SELECTED_BLOCK_SIDE_PADDING),
            height: area.height.saturating_sub(2 * SELECTED_BLOCK_SIDE_PADDING),
        },
        &HeroContent {
            title: show_title.then_some(book.title.as_str()),
            meta_line: (!meta.is_empty()).then(|| meta.join("  ")).as_deref(),
            meta_color: palette::TEXT_DETAIL_META,
            show_playing: false,
            unconditional_spacer_after_meta: false,
            lines: &hero_lines,
            image: (image_height > 0).then_some(HeroImage {
                actual_w: image_width,
                height: image_height,
            }),
        },
        focused,
    );
    (plan.has_image && result.img_rect.is_some()).then(|| {
        super::home_hero::HomeImagePaint::AudiobookshelfBookCover {
            area: result.img_rect.unwrap(),
            library_item_id: book.library_item_id.clone(),
        }
    })
}

fn render_book_rows(
    frame: &mut Frame,
    area: Rect,
    state: &AudiobookshelfBookBrowseState,
    chapter_selection: Option<usize>,
    focused: bool,
    chapter_list: &mut WideMediaList<String>,
    geometry: &mut AudiobookshelfBookGeometry,
) {
    if area.height == 0 {
        return;
    }
    let Some(id) = state.selected_id.as_deref() else {
        return;
    };
    if state.detail_loading {
        render_placeholder(frame, area, " Loading…");
        return;
    }
    let rows = state.visible_rows(id);
    if rows.is_empty() {
        render_placeholder(frame, area, " No chapters available");
        return;
    }
    let show_length = area.width > 40;
    let media_rows = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let (primary, duration) = match row {
                BookRow::Chapter { title, start, end } => (
                    title.clone(),
                    list_duration_secs((end - start).max(0.0) as i64),
                ),
                BookRow::AudioFile { index, duration } => (
                    format!("Part {index}"),
                    list_duration_secs(*duration as i64),
                ),
            };
            MediaListRow::Item {
                target: index.to_string(),
                primary,
                trailing: None,
                duration: if show_length { duration } else { None },
                kind: MediaKind::Media,
                semantic_state: MediaSemanticState::Ordinary,
            }
        })
        .collect();
    chapter_list.set_content(media_rows);
    chapter_list.select_index(chapter_selection.unwrap_or(0));
    let paint = render_wide_media_list(
        frame,
        area,
        area,
        &mut *chapter_list,
        focused,
        palette::list_selected_row_bg(),
    );
    geometry.chapter_rows = paint
        .row_geometry
        .visible_rows(area)
        .into_iter()
        .enumerate()
        .map(|(screen, rect)| (rect, paint.row_geometry.offset() + screen))
        .collect();
}

fn book_hero_plan(
    state: &AudiobookshelfBookBrowseState,
    width: u16,
    images_enabled: bool,
) -> BookHeroPlan {
    let Some(book) = state.selected_book() else {
        return BookHeroPlan {
            has_image: false,
            image_width: 0,
            image_height: 0,
            content_rows: HERO_TITLE_ROWS,
        };
    };
    let has_cover = images_enabled && book.cover_path.is_some();
    let image_width = if has_cover { 18 } else { 0 }.min(width);
    let image_height = if has_cover { 12 } else { 0 };
    let author_rows = u16::from(
        book.author_display
            .as_deref()
            .is_some_and(|value| !value.is_empty()),
    );
    let overview = book
        .description
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(crate::app::ui_util::trunc_overview)
        .unwrap_or_default();
    let overview_rows = wrap_overview_lines(&overview, |line| {
        crate::app::render::components::hero::inline_hero_text_width(
            width,
            image_width,
            image_height,
            HERO_TITLE_ROWS + 2 + author_rows + line as u16,
        ) as usize
    })
    .len() as u16;
    BookHeroPlan {
        has_image: has_cover,
        image_width,
        image_height,
        content_rows: image_height
            .saturating_add(1)
            .max(HERO_TITLE_ROWS + 2 + author_rows + overview_rows),
    }
}
