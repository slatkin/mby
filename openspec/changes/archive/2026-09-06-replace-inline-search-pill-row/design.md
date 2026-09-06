## Context

See `proposal.md` for motivation and `specs/inline-library-search/spec.md` for the behavior contract. ADR 0025 keeps Inline Search embedded in the active destination: Browser, MusicWorkspace, or TvWorkspace owns placement, painting, and input interpretation for its current presentation.

Today the destination already computes the canonical pill slot and normal content rectangle, but the shared Inline Search renderer receives only the content rectangle and carves a second three-row input from it. Wide destinations can therefore paint both the one-row pill-slot search bar and the shared bordered input, while Normal destinations can paint pill controls above the bordered input.

## Goals / Non-Goals

**Goals:**

- Keep canonical pill-slot and normal-content geometry authoritative at each destination composition point.
- Keep one shared Inline Search painter for both the one-row bar and result rows.
- Keep ordinary pill controls non-interactive while search is active.

**Non-Goals:**

- Changing Inline Search ownership, lifecycle, query handling, result ordering, or shell effects.
- Changing responsive transfer semantics or ordinary library layouts.
- Adding a presentation variant, overlay, or second painter.

## Decisions

### 1. Destinations supply the two canonical rectangles

The active destination SHALL pass the existing pill rectangle and the existing normal library-content rectangle to the shared Inline Search renderer. Normal presentations obtain them from the shared pill-bar arrangement; Wide presentations use the pill and padded list rectangles already produced by the wide browser-pane arrangement.

This preserves each arrangement's existing padding, panel borders, spacer, and breakpoint behavior. The alternative—passing one outer rectangle and letting Inline Search derive both areas—would duplicate destination-specific Wide padding and panel geometry.

### 2. The shared renderer owns the complete search presentation

The shared renderer SHALL paint the one-row search bar into the supplied pill rectangle and result rows into the supplied content rectangle. The current three-row search-area derivation and bordered input painter SHALL be removed or reduced so they cannot create a second input.

The existing one-row visual treatment is reused rather than introducing another search-bar style. Destination painters SHALL not pre-paint a search bar before invoking the shared renderer. The shared renderer SHALL continue publishing the supplied result rectangle through the embedded `InlineSearch` control's layout so cursor visibility, page movement, mouse row resolution, and responsive transfer keep using painted geometry.

### 3. Search and pill controls are mutually exclusive

At each destination composition point, active search SHALL select the shared search painter instead of the ordinary pill painter, so ordinary pill controls are neither visible nor available to mouse interaction.

Relying only on input precedence while retaining the ordinary pill presentation was rejected because it would violate the one-owner, one-painter surface contract.

## Risks / Trade-offs

- [A destination supplies a panel rectangle instead of its padded content rectangle] → Verify the shared renderer publishes exactly the supplied result rectangle.
- [A pill painter still runs before search on one destination] → Exercise each destination's search branch and verify its ordinary pill presentation is unavailable.
- [Removing three rows changes viewport size and handoff offsets] → Retain the existing cursor-visibility and Normal/Wide transfer checks using the newly published result rectangle.
