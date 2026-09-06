## 1. Establish Wide hero vocabulary

- [x] 1.1 Update `CONTEXT.md` so `Wide hero` replaces `Hero-on-left`, defines the left browser/right hero arrangement, and preserves `Inline hero`; verify the current presentation section contains no `Hero-on-left` term.
- [x] 1.2 Add an ADR that supersedes ADR 0021 with the Wide hero/Inline hero decision and leaves accepted historical ADR and archived OpenSpec files unchanged; verify ADR links and supersession text resolve correctly.
- [ ] 1.3 Rename current crate-private `hero_on_left` modules, functions, types, fields, and comments to semantic `wide_hero`, `browser`, and `hero/workspace` names without compatibility aliases; verify `rg 'hero_on_left|Hero-on-left' src CONTEXT.md docs/adr openspec/specs` reports only explicitly retained historical references.

## 2. Mirror the shared arrangement

- [ ] 2.1 Change the shared Wide hero pane calculation to place the existing larger browser pane on the left and the existing approximately 40% hero pane on the right while preserving the gap, minimum widths, breakpoint, minimum-height guard, padding, and status-row reserve; verify `cargo check -p mbv` succeeds.
- [ ] 2.2 Update the shared browser-pane pills/list framing and hero-pane fill/content-box primitives to consume role-named geometry without destination-owned splitting; verify `ast-grep scan` reports no frontend-boundary violation.

## 3. Adopt mirrored geometry everywhere

- [ ] 3.1 Update Home, generic Emby Movies/homevideos/podcasts, and Feeds Wide rendering so the canonical browser and browser-level pills paint left while the read-only hero paints right; verify existing rendered checks pass and read-only hero pointer behavior remains inert.
- [ ] 3.2 Update TV and grouped Music so their canonical browser rails paint left and their Series/episode or album/track workspaces paint right; verify existing focus, selection, Inline Search, image, and pointer checks pass without changing keyboard dispatch.
- [ ] 3.3 Update Audiobookshelf Podcast and Book so show/book browsers and pills paint left and episode/chapter workspaces paint right; verify existing breakpoint, anchor, focus, image, and pointer checks pass without changing input behavior.
- [ ] 3.4 Audit each affected Interactive Component's published paint and hit geometry so pointer targets move with their painted pane and no shell/global hit map is introduced; verify the existing mouse integration tests for affected destinations pass.

## 4. Evaluate existing tests

- [ ] 4.1 Review every failing or renamed placement test and record in the implementation diff whether it protects a durable contract (breakpoint, status reserve, one painter, focus, target geometry, or state preservation) or only obsolete side/name details; update durable tests and delete or relax placement-only tests, adding no unit tests.
- [ ] 4.2 Run the narrowest existing component/render test groups for Home, Browser/Movies, TV, Music, Feeds, Audiobookshelf Podcast, and Audiobookshelf Book and verify they pass with no newly created test function or test file.

## 5. Synchronize and verify

- [ ] 5.1 Sync every delta spec in this change into its matching current `openspec/specs/<capability>/spec.md`, then normalize retained scenario headings to current Wide hero vocabulary; verify `openspec validate adopt-wide-hero --strict` passes and current specs contain no active `Hero-on-left` contract.
- [ ] 5.2 Run `cargo fmt`, `cargo check -p mbv`, the targeted existing tests, `ast-grep scan`, and `make check-code-file-lines`; verify every command succeeds.
- [ ] 5.3 Inspect Home, Movies, TV, grouped Music, Emby podcasts/homevideos, Feeds, Audiobookshelf Podcast, and Audiobookshelf Book at Wide, Normal, and Wide-but-short geometry; verify Wide alone has left browser/right hero order while Inline hero, focus, Enter/Esc behavior, selection, and pointer targets remain unchanged.
