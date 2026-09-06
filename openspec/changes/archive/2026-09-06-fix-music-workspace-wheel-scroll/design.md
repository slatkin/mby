## Context

Delivery pipeline is proven: tick-integration tests inject real wheel events and other surfaces (Queue, Home, Browser, TV, ABS, Feeds) scroll today. The gap is local to `MusicWorkspaceComponent::handle_mouse` (`src/app/components/music_workspace.rs`), which matches Click/DoubleClick/RightClick and falls through `_ => None` for scroll. Keyboard equivalents already exist: PageUp/PageDown album paging via `move_album_rows(page_rows)` and j/k track stepping.

## Goals / Non-Goals

**Goals:**
- Wheel over wide right rail pages albums (same as PageUp/PageDown).
- Wheel over wide track table steps track cursor (same as j/k).
- Wheel over narrow album list pages albums.
- Ledger row updated.

**Non-Goals:**
- Wheel over Music chrome (pills, scope buttons) — no keyboard equivalent, not required by parity rule.
- Search sidebar / Inline Search wheel — deliberately out of scope per ledger.
- New shell requests — cursor moves stay component-local like the existing click arms.

## Decisions

- **One `MouseGesture::Scroll` arm in `handle_mouse`**, mirroring the pane split already used by the click arms: resolve the pointer against the pane geometry the component paints, then dispatch per pane. No shell request needed — the existing `MusicAlbumCursor`/track-cursor moves are component-local and re-render happens through the normal component update cycle.
- **Reuse the keyboard paging path** (`move_album_rows` with the same `page_rows` used by PageUp/PageDown) rather than inventing a wheel-specific scroll amount, so wheel and keyboard paging stay identical.
- **Direction mapping**: ScrollUp = back a page / previous track; ScrollDown = forward a page / next track, matching every other surface.

## Risks / Trade-offs

- Album paging is a page-size jump, not a smooth row scroll — matches the existing keyboard behavior, accepted as intentional parity.
- Wheel over the track table when no album is selected is a no-op (same as j/k today).
