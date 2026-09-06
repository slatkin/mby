mod primitives;

use ratatui::style::Color;

// ---------------------------------------------------------------------------
// Roles (openspec/changes/centralize-ui-design-language,
// openspec/changes/enforce-mbv-ui-design-system) — the public API. A role
// names what a colour *means*, never what hue it is. `primitives` holds the
// raw constants and is private to this module, so nothing outside `theme`
// can name a hue directly.
// ---------------------------------------------------------------------------

// Surfaces
pub const SURFACE_BACKDROP: Color = primitives::LIBRARY_SIDE_BG;
pub const SURFACE_CHROME: Color = primitives::DARK_BG;
pub const SURFACE_FOCUSED: Color = primitives::BG_GREEN;
pub const SURFACE_RESTING: Color = primitives::PLAYBACK_PANEL_BG; // resting-content / unfocused half
pub const SURFACE_PLAYBACK: Color = primitives::PLAYBACK_PANEL_BG; // now-playing-strip half
pub const SURFACE_ACCENT_SOFT: Color = primitives::BG_GREEN_SOFT;
pub const SURFACE_ITEM_FOCUSED: Color = primitives::FOCUSED;
pub const SURFACE_STATUS_PILL: Color = SURFACE_CHROME; // pills sit on the chrome status row; same bg
pub const SURFACE_SIDEBAR: Color = primitives::PANEL_BG; // plain (non-hero) sidebar/panel background
pub const SURFACE_ARTWORK_PLACEHOLDER: Color = primitives::ARTWORK_PLACEHOLDER;

// Accents
pub const ACCENT: Color = primitives::AQUA; // selection marker, watched, folders, Emby brand glyph
pub const ACCENT_ACTIVE: Color = primitives::IRIS; // active tab, focused pill/badge text, selected-row bg
pub const ACCENT_AUDIOBOOKSHELF: Color = primitives::AMBER; // audiobookshelf brand glyph

// Rules
pub const BORDER_UNFOCUSED: Color = primitives::OVERLAY;

// Pill selector (renamed without value changes)
pub const PILL_ROW_BG: Color = primitives::PILL_SELECTOR_ROW_BG;
pub const PILL_BG: Color = primitives::PILL_SELECTOR_BG;
pub const PILL_SELECTED_BG: Color = primitives::PILL_SELECTOR_SELECTED_BG;
pub const PILL_FG: Color = primitives::PILL_SELECTOR_FG;
pub const PILL_SELECTED_FG: Color = primitives::PILL_SELECTOR_SELECTED_FG;
pub const PILL_OVERFLOW_FG: Color = primitives::PILL_SELECTOR_OVERFLOW_FG;

// Text: readable content foregrounds, independent of surface
pub const TEXT_PRIMARY: Color = primitives::TEXT;
pub const TEXT_SECONDARY: Color = primitives::SUBTLE; // paired with TEXT_PRIMARY/TEXT_STRONG
pub const TEXT_MUTED: Color = primitives::MUTED; // dim text, icons, unfocused state
pub const TEXT_STRONG: Color = primitives::WHITE; // bold titles/headings
pub const TEXT_EMPHASIS: Color = primitives::SOFT_WHITE; // warm emphasis text (focused rows, dialogs)
pub const TEXT_FOCUS_ACCENT: Color = primitives::YELLOW; // focused-row title accent
pub const TEXT_ON_ACCENT: Color = primitives::BASE; // near-black text painted on a colored surface
pub const TEXT_ACCENT_MUTED: Color = primitives::BG_GREEN; // "loaded"/"playing"/confirmed value text
pub const TEXT_DETAIL_META: Color = primitives::MUTED_GREEN; // detail-screen label/meta text
pub const TEXT_METADATA: Color = primitives::FOAM; // secondary metadata (durations, pct, badges)

// Status
pub const STATUS_ERROR: Color = primitives::RED;
pub const STATUS_AVAILABLE: Color = primitives::GREEN; // checkmarks, available/positive metadata

// Media indicators (resolution/audio glyphs)
pub const INDICATOR_RESOLUTION_FG: Color = primitives::ORANGE;
pub const INDICATOR_AUDIO_FG: Color = primitives::PURPLE;

// Playback panel
pub const PLAYBACK_VALUE_FG: Color = primitives::PLAYBACK_CONTENT_FG; // title/codec value
pub const PLAYBACK_META_FG: Color = primitives::PLAYBACK_META_FG; // captions/time

// Progress and queue
pub const PROGRESS_TRACK: Color = primitives::SEEK_TRACK; // unplayed seek/progress track

// Chrome
pub const SCROLLBAR: Color = primitives::SCROLLBAR;

/// The central focus lever (design decision 8). Every panel and component
/// resolves its focused/unfocused surface through this single function
/// instead of naming `SURFACE_FOCUSED`/`SURFACE_RESTING` at the call site.
///
/// `focused` is the caller's two-input focus model already collapsed to one
/// bool: the existing `PanelFocus` (which panel is focused) for
/// inline screens with one focusable region, or `PanelFocus` combined
/// with a pane bit (`left_focused`) for Wide hero screens with two.
pub fn resolve_surface_focus(focused: bool) -> Color {
    if focused {
        SURFACE_FOCUSED
    } else {
        SURFACE_RESTING
    }
}

/// The focused selected-row background for a canonical media list. The row
/// "punches through" to the surface *containing* the panel that holds the
/// list, not the list panel itself. Every library rail plus Home and Feeds
/// sits inside a resting-surface parent — even while the list panel is
/// focused-green — so this is the constant [`SURFACE_RESTING`], not
/// `resolve_surface_focus(focused)`. Queue is the one non-library caller
/// whose parent is itself focus-green; it passes [`SURFACE_FOCUSED`]
/// directly instead of calling this.
pub fn list_selected_row_bg() -> Color {
    SURFACE_RESTING
}
