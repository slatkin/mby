## Why

The Music workspace lost wheel scrolling in the TuiRealm migration: `MusicWorkspaceComponent::handle_mouse` recognizes Click/DoubleClick/RightClick but has no `MouseGesture::Scroll` arm, so wheel over the album rail, track table, or narrow album list does nothing. Every other list surface (Queue, Home, Browser, TV, ABS, Feeds) scrolls on wheel; this is the last gap, and the `mouse-input` spec requires wheel parity wherever a keyboard action exists (PageUp/PageDown, j/k).

## What Changes

- Add a `MouseGesture::Scroll` arm to `MusicWorkspaceComponent::handle_mouse`:
  - Wide: wheel over the right rail pages albums (PageUp/PageDown equivalent); wheel over the track table steps the track cursor (j/k equivalent).
  - Narrow: wheel over the album list pages albums.
- Update the Grouped Music workspace row in `docs/architecture/interactive-surface-ledger.md` to record wheel behavior per pane.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `mouse-input`: add wheel-scroll requirements for the Music workspace surfaces (wide rail/track table, narrow list), closing the "wheel-scroll where a keyboard action exists" parity gap for the last unmigrated surface.

## Impact

- `src/app/components/music_workspace.rs` (handle_mouse)
- `docs/architecture/interactive-surface-ledger.md` (Grouped Music workspace row)
- No shell-side changes: `MusicAlbumCursor` and the paging requests already exist; the component applies its own cursor moves locally.
