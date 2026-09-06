## 1. Component wheel arm

- [x] 1.1 Add `MouseGesture::Scroll` arm to `MusicWorkspaceComponent::handle_mouse` (`src/app/components/music_workspace.rs`): resolve pane from painted geometry; wide right rail → `move_album_rows(±page_rows)`; wide track table → step track cursor by ±1; narrow → `move_album_rows(±page_rows)`; elsewhere → `None`.

## 2. Ledger

- [x] 2.1 Update Grouped Music workspace row in `docs/architecture/interactive-surface-ledger.md`: add wheel behavior per pane (rail page move, track step, narrow page move).

## 3. Verification

- [x] 3.1 `cargo fmt`, `cargo check -p mbv`, `cargo clippy --workspace --all-targets`, `cargo nextest run -p mbv` clean.
- [x] 3.2 Manual check at wide and narrow breakpoints that wheel pages albums / steps tracks and wheel over chrome does nothing.
