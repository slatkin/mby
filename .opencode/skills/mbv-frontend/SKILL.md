---
name: mbv-frontend
description: Ownership rules and workflow for mbv's terminal UI - the TuiRealm Interactive Components in src/app/components/ and the render tree in src/app/render/. Use this before adding or changing any component, painter, arrangement, or theme role; before handling a key, a mouse event, or component-local state; before adding a new visual variant; and before reporting a TUI change complete. Use it even when the change looks like a one-line tweak, because the two trees have colliding file names and the keyboard has exactly one legal routing site.
---

# mbv-frontend

`src/app/render/` is split into four kinds of module. The split is the
enforcement boundary from the completed change archived at
`openspec/changes/archive/2026-08-23-enforce-mbv-ui-design-system/`; the term
definitions live in `CONTEXT.md` under Presentation and are authoritative if this
skill and `CONTEXT.md` ever disagree.

| Module | Owns | Must not |
|---|---|---|
| `screens/` | app state in, typed content model out | call Ratatui, construct a `Rect`, compute a hit target |
| `arrangements/` | placement of components within a `Rect`, breakpoints | own painting or app state |
| `render/components/` | painting, its own geometry within a `Rect` | take arbitrary `Color`/`Style` from a screen |
| `theme/` | semantic roles (public) | expose raw `Color` primitives (private) |

Dependency order: `screens -> arrangements -> render/components -> Ratatui`.
Throughout this section, a bare `components/` means `src/app/render/components/`;
the TuiRealm Interactive Components in `src/app/components/` are a separate tree,
described below.

The visual migration ledger is archived with the completed design-system change.
The interactive-ownership migration to TuiRealm (ADR 0022) is **complete**: every
row of `docs/architecture/interactive-surface-ledger.md` reached `migrated` on
2026-08-27. Existing violations are not licence to add new ones, including in the
same file.

## Two trees, not one

This is the easiest place to put code in the wrong file, because the two trees
have colliding names. `src/app/components/help.rs` and
`src/app/render/components/help.rs` are different things:

| Tree | What lives there | Example |
|---|---|---|
| `src/app/components/` | **Interactive Components** — TuiRealm `impl Component`: local interaction state, event interpretation, `update()`, and the `view()` entry point | `HelpComponent` |
| `src/app/render/` | **The visual substrate** — the Ratatui painters, arrangements, and theme roles a component's `view()` calls into | `render_help_panel` |

The seam is `Component::view(&mut self, f: &mut Frame, area: Rect)`: it receives
an outer area from an arrangement or the root layout, and delegates the painting
to a `render/` function. A component holds *cursor, scroll, drafts, viewport, and
hit geometry*; `render/` holds *pixels*. When unsure where a difference belongs,
ask "is this interaction, state, painting, layout, or style?" and route to
`components/` / `screens/` / `render/components/` / `arrangements/` / `theme/`
accordingly.

## Interactive ownership (ADR 0022)

An Interactive Component owns its private presentation state, event
interpretation, local updates, rendering, viewport, and render-derived hit
geometry. It emits a typed `Msg` (`src/app/components/msg.rs`) for anything
crossing that boundary. The shell `Model` (`src/app/shell*.rs`) owns `App`,
terminal/Service/worker lifecycle, Player and canonical queue authority,
persistence, and external effects.

A component therefore never receives `App`, a Service client, `PlayerProxy`,
`Config`, credentials, or an mpsc channel. `rules/interactive-component-boundary/`
enforces that mechanically (`ast-grep scan`), but the reason matters more than
the rule: this migration existed to delete an `App`-wide input snapshot, and every
one of those handles is a way to grow it back.

Data flows shell→component one way, through `sync_<surface>()` and `push_*`
helpers that project validated snapshots. A `sync_*` that reads component-local
interaction state back into `App` reintroduces exactly the mirror that was
removed — if you find yourself needing one, the state is on the wrong side.

## Canonical media-list composition

Use `WideMediaList` for fixed-row, one-column Wide rails and Queue. Use
`InlineMediaBrowser` for one-column Normal/Narrow selected-row replacement. These
controls are embedded and painted by their destination parent; `Inline Search`
is the separate `InlineSearchComponent`, not a media-list variant. Non-hero
catalogs retain the existing two-column policy.

The primary destination owners and painters are Home (`HomeComponent`), generic
Emby/Movies/homevideos and Emby podcast (`BrowserComponent`), TV Series
(`TvWorkspaceComponent` in Wide, `BrowserComponent` in Normal), grouped Music
(`MusicWorkspaceComponent`), Audiobookshelf Podcast (`AudiobookshelfPodcastComponent`),
Audiobookshelf Books (`AudiobookshelfBookComponent`), Feeds (`FeedsComponent`),
and Queue (`QueueComponent`). The ledger is the detailed breakpoint record.

## Keyboard routing (ADR 0023)

There is exactly one keyboard resolution site: `src/app/router.rs`, with its
ordered policy in `src/app/key_policy.rs`, folded into the tick in
`shell_run.rs`. It returns ADR 0002's `Command` / `Swallow` / `FallThrough` from
a plain-data `RouterSnapshot`. A component interprets only its own local chords
and emits a semantic intent.

Two approaches that look reasonable and are not:

- **A second resolution site** — a global chord claimed inside a component, a
  shell method, or a subscription. Precedence then stops being readable in one
  ordered place, which is the property ADR 0002 bought and ADR 0023 preserves.
- **Precedence expressed as a `SubClause`** — TuiRealm's `tick()` fans events out
  unconditionally to the focused component *and* every satisfied subscription,
  with no consumed signal and an all-or-nothing `sub_lock`. `SubClause` can read
  only `mounted()`, `state()`, and `query()`, so it cannot express `Swallow` or
  `FallThrough`. Reproducing first-match through gates would require every gate
  to encode the negation of every higher-priority claimant — the distributed
  mirror state the migration deleted. That gap is the whole reason the router
  exists.

Legacy-endpoint removal is complete (archived at
`openspec/changes/archive/2026-08-29-remove-legacy-keyboard-endpoint/`):
`GlobalViewKey`, the raw `*Key` shell request variants, `CONTEXT_STACK`,
`Model::handle_legacy_key`, and `src/app/components/typed_key.rs` are deleted.
Do not reintroduce them — three scan gates enforce this:
`no-crossterm-key-payloads`, `no-raw-fallback-variants`, and
`no-second-router-site` (fixtures in
`rules/interactive-component-boundary-tests/`).

## Mouse delivery (ADR 0024)

D16 (in `openspec/changes/archive/2026-08-29-migrate-tui-to-tuirealm/design.md`)
deleted the legacy mouse framework rather than migrating it; `restore-mouse-support`
(#638, archived `2026-09-05`) reversed that and rebuilt mouse delivery on TuiRealm
subscriptions. The contract is `openspec/specs/mouse-input/spec.md`.

Subscriptions decide eligibility pre-delivery, following surfaces painted in the
latest frame (or topmost overlay). The mounted parent owns gesture state and
resolves only geometry it painted; embedded lists resolve their own rows. No
separate mouse loop, no global hit map/router, and never discard a losing message
after its component mutated — the framework mutates a component before it returns
a message, so a discarded message does not undo the mutation.

## Version scope

`tuirealm = "4.1"` is pinned in `Cargo.toml` alongside `ratatui = "0.30"`.
Confirm the locked version and API in `Cargo.toml` before relying on a specific
TuiRealm call — APIs drift between major versions, and the `find-docs` skill can
pull current references. Re-verify ADR 0024's subscription assumption before any
TuiRealm bump.

## Reuse workflow

Before writing rendering code for a screen:

1. **Look for an existing component or arrangement first.** Check
   `src/app/render/components/mod.rs` and `src/app/render/arrangements/mod.rs`
   for something that already paints this shape (a row, a card, a modal
   frame, a hero pane). Reuse it before writing a new painter — for a
   hero-bearing surface this means `hero_on_left_pane`/`LeftPaneFocus`
   (`arrangements/hero_left.rs`) for the pane itself and the `Hero` trait
   (`components/hero_model.rs`) for its content, not a bespoke layout.
2. **If it almost fits, check for a policy or variant** (see the decision
   table below) before reaching for a screen-local branch.
3. **If nothing fits, add centrally** — a new component/arrangement function,
   or a new named variant/policy on an existing one — not inline in the
   screen.
4. **If it genuinely cannot use the shared vocabulary**, register it as a
   named bespoke component (see below). This is the last resort, not the
   default when reuse looks inconvenient.

## Controlled-override decision table

None of these rows permit screen-owned geometry, raw Ratatui calls, or raw
`Color`/`Style` values passed into a shared component. The only question is
*where* the difference lives.

| Kind of difference | Where it lives | Screen does |
|---|---|---|
| **Content change** (different title, metadata, rows, image) | The screen's own typed content model | Populate the model's fields; call the same component/arrangement |
| **Named policy** (a small closed set of valid style/behaviour combinations already exists) | The component/arrangement that defines the policy | Select the named policy constructor, e.g. a focus/unfocus style pair like `list_rows::focused_or_muted(focused)` |
| **Central variant** (a new but still centrally-owned presentation, e.g. Inline hero vs. Hero-on-left) | The owning arrangement or component, as a new named variant | Select the variant; never paint the alternate presentation itself |
| **New component** (no existing painter fits, but the need is general) | A new function in `components/` or `arrangements/`, exposed like `modal_frame::render_modal_frame` | Call the new component; the component is reviewable and reusable by other screens |
| **Bespoke surface** (reuse genuinely does not fit after a real attempt) | A named bespoke component, with its stated reason and its own buffer coverage | Call the bespoke component; it still obeys ownership, semantic theming, and verification rules — it is not exempt from them |

### Worked examples

- *"This screen needs a different subtitle on the modal."* Content change.
  Pass the subtitle into the existing modal's content model; do not add a
  `subtitle_color: Option<Color>` parameter to `render_modal_frame`.
- *"This row should look focused or muted depending on state."* Named policy.
  Use the existing `focused_or_muted`/`focused_or_subtle` style pair in
  `components/list_rows.rs` rather than inlining
  `if focused { palette::X } else { palette::Y }` in the screen.
- *"This browse surface wants hero-on-left instead of inline hero."* Central
  variant. Both presentations already exist in `arrangements/hero_left.rs`
  and `components/hero.rs`; the screen selects which one applies (per the
  width/height gate), it does not build a third layout.
- *"Nothing existing places two panes side by side with this sizing rule."*
  New component/arrangement. Add the placement function to `arrangements/`
  (or extend an existing one with a named variant if the shape is close
  enough) — not a one-off `Layout::horizontal([...])` call inside the screen.
- *"This surface's presentation is genuinely unlike anything else in the
  app."* Bespoke surface. Register it as a named bespoke component with the
  reason written down and its own buffer test; it still may not call Ratatui
  from the screen module, and it still consumes theme roles, not raw colours.

## Ratatui patterns

```rust
// Wrong: screen calls Ratatui directly.
// src/app/render/screens/some_screen.rs
f.render_widget(Paragraph::new(text), Rect { x, y, width, height });

// Right: screen builds a content model, component paints it.
// src/app/render/screens/some_screen.rs
let model = SomeRowModel { text, focused };
self.render_some_row(f, area, &model); // defined in components/

// src/app/render/components/some_component.rs
pub(in crate::app::render) fn render_some_row(f: &mut Frame, area: Rect, model: &SomeRowModel) {
    let fg = focused_or_muted(model.focused); // named policy, not a raw Color
    f.render_widget(Paragraph::new(model.text.clone()).style(Style::default().fg(fg)), area);
}
```

```rust
// Wrong: screen splits its own layout.
let [left, right] = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);

// Right: an arrangement owns the split, screen calls it.
let hero = hero_on_left_pane(f, area, LeftPaneFocus::ReadOnly); // arrangements/hero_left.rs
```

```rust
// Wrong: screen picks an arbitrary colour.
Style::default().fg(palette::ACCENT_ACTIVE).bg(Color::Rgb(20, 20, 20))

// Right: screen passes semantic focus state; the component resolves the role.
// (Raw Color primitives are private to theme/ and cannot be named outside it.)
Style::default().fg(if focused { palette::ACCENT_ACTIVE } else { palette::TEXT_PRIMARY })
```

## What these checks do not catch

Three mechanisms enforce this boundary, in descending strength. Know which
one you're relying on:

1. **The compiler** — private theme primitives. A raw `Color` outside
   `theme/` is a compile error. Cannot be bypassed.
2. **ast-grep**, run as `ast-grep scan` from the repo root over two rule
   directories (`sgconfig.yml` registers both):
   - `rules/frontend-boundary/` scopes to `src/app/render/screens/` and flags
     `use ratatui::`, `render_widget`/`render_stateful_widget`, `Layout::...`,
     `Rect` construction, and `buffer_mut()` in screen modules.
   - `rules/interactive-component-boundary/` scopes to `src/app/components/` and
     rejects `impl App`, `App` as a type, Service clients / `PlayerProxy` /
     `RemotePlayer`, and `std::sync::mpsc`. Fixtures live in
     `rules/interactive-component-boundary-tests/`; `ast-grep test` runs them,
     and `ast-grep test -U` regenerates snapshots after an intentional rule
     change.

   The scan catches the common bypasses and nothing subtler. The bare
   `ast-grep scan` gates the whole tree and must be clean. It does **not** catch:
   - **Duplicated arrangement geometry** — a screen that calls an existing
     arrangement correctly but a second, near-identical arrangement was added
     elsewhere instead of extending the first one.
   - **State smuggled through a sync** — a `sync_*` or push helper that carries
     component-local interaction state back into `App`. This reads as ordinary
     shell plumbing and only review catches it.
   - **Hit targets drifted from painting** — a component's painted geometry
     changes but its own `hit_test`/region arithmetic is not updated to match.
   - Test files (`*tests*.rs`) and inline `#[cfg(test)] mod tests { ... }`
     blocks inside an otherwise-production file are not distinguished by
     these rules; a `#[cfg(test)]` block that legitimately builds a
     `TestBackend` buffer will still be flagged if it lives in a
     non-`*tests*`-named file. Prefer a dedicated `..._tests.rs` file for new
     buffer tests so the check stays accurate.
3. **Review**, against the checklist below — this is what catches the two
   items above. A clean ast-grep run is not proof of conformance.

## Tests

**Never assert raw UI geometry.** Pane order and pane size are an arrangement's
contract — assert them **once**, in that arrangement's own unit test, and
**relationally** (`hero.x == browser.right() + gap`), never as absolute
coordinates. Every other test asserts **containment in a role rect**
(`panes.browser.contains(rect)`) or **rendered content** (buffer cells), never a
coordinate, width, or left/right ordering pulled from an arrangement's return
value. A test that can only break on a deliberate layout change is churn, not
coverage — delete it rather than update it.

## Completion checklist

Before reporting a TUI change complete:

- [ ] **Render boundary** — no `use ratatui::`, `render_widget`, `Layout::`,
  `Rect` construction, or `buffer_mut()` was added to a `screens/` module. If
  ast-grep flags something you added, fix it rather than widening an `ignores`
  glob.
- [ ] **Narrow-width behaviour** — the change was checked at the narrow/mini
  breakpoint, not only the default width.
- [ ] **Interaction targets** — if painted geometry moved or resized, the
  component's own hit-geometry arithmetic still matches it. (Delivery follows the
  latest painted frame per `mouse-input/spec.md` — keep hit arithmetic matched
  to painting.)
- [ ] **Component boundary** — no `App`, Service client, `PlayerProxy`, `Config`, or mpsc
  reached a component; anything crossing the boundary went out as a typed `Msg`.
- [ ] **One router** — no chord is resolved outside `router.rs`/`key_policy.rs`,
  and no new caller of `GlobalViewKey`, a raw `*Key` request, `CONTEXT_STACK`, or
  `handle_legacy_key` was added.
- [ ] **No geometry assertions** — no new test asserts absolute pane
  coordinates/widths or left/right ordering from an arrangement's return value;
  layout claims are buffer-content or role-rect containment assertions.
- [ ] **Buffer tests** — a characterization test exists (or was added first,
  in its own commit, per the ledger migration flow) and passes unchanged
  where the change is not expected to alter output.
