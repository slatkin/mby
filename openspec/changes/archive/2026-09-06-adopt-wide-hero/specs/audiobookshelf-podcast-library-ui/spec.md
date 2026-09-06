## MODIFIED Requirements

### Requirement: Podcast libraries use responsive hero presentations

An Audiobookshelf podcast library SHALL use the shared Wide hero presentation when it meets the wide geometry conditions and selected-row replacement otherwise. In Wide hero, the selected podcast's cover, metadata, and downloaded-episode workspace SHALL occupy the right pane while the single-column podcast-show browser occupies the left rail. In the replacement presentation, the same selected-show detail (title, author, description, cover) SHALL replace the active podcast-show row in list flow. The podcast tab SHALL obtain placement from the shared arrangement and SHALL NOT define a separate fallback.

The podcast tab SHALL supply podcast-native data without changing the shared placement rule: Podcast show for Series, Audiobookshelf cover for Series Primary image, and matching downloaded episodes for the selection modal. Image shape, metadata lines and order, colour variant, element presence, and image source MAY remain podcast-specific declarations.

#### Scenario: Podcast library is displayed wide

- **WHEN** an Audiobookshelf podcast library meets the shared wide geometry conditions
- **THEN** selected-show detail and downloaded episodes render in the right pane
- **AND** podcast shows render in the single-column left rail

#### Scenario: Podcast library is displayed narrow

- **WHEN** an Audiobookshelf podcast library does not meet the shared wide geometry conditions
- **THEN** podcast shows render in one scrolling column with alphabetical panel pills
- **AND** selected-show detail (title, author, description, cover) replaces the active show row
- **AND** no separate hero area is reserved above the show browser
- **AND** no episode rows or filter pills render inside the inline hero

#### Scenario: Podcast selection changes

- **WHEN** the user moves selection between podcast shows
- **THEN** the hero or detail workspace updates to the newly selected podcast
- **AND** the show list retains provider-native selection identity across loaded-page changes

#### Scenario: Selected show scrolls in the inline presentation

- **WHEN** the active podcast show moves through the narrow browser
- **THEN** scrolling keeps its media row and inline detail addressable together
- **AND** the replacement block owns the selected parent target while explicit child targets take precedence

#### Scenario: Terminal height cannot fit Wide hero

- **WHEN** the width meets the shared breakpoint but the minimum-height guard fails
- **THEN** the podcast tab uses selected-row replacement
- **AND** it restores the ordinary selected row if detail cannot fit

#### Scenario: Shared placement changes

- **WHEN** the shared Wide hero or inline presentation changes
- **THEN** the podcast tab renders the placement change without an individual geometry edit

#### Scenario: Podcast library is displayed

- **WHEN** an Audiobookshelf podcast library is displayed
- **THEN** it uses Wide hero when wide geometry fits and inline selected-show detail (title, author, description, cover) otherwise

#### Scenario: Selected show scrolls outside the visible list rows

- **WHEN** the selected show scrolls outside visible left-rail rows in Wide hero
- **THEN** the right workspace continues projecting that selected show

#### Scenario: Terminal width crosses the TV list column breakpoint

- **WHEN** the podcast tab crosses the shared width breakpoint
- **THEN** it recomputes Wide hero versus selected-row replacement rather than changing a detail layout column count

#### Scenario: Terminal height cannot fit the hero

- **WHEN** selected detail cannot fit with a usable active row
- **THEN** detail is suppressed and the browser retains the available area

#### Scenario: The retired separate placement changes

- **WHEN** the obsolete separate placement is removed
- **THEN** Audiobookshelf podcasts continue through only Wide hero and selected-row replacement
