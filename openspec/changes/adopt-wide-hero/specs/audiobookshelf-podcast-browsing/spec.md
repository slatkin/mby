## MODIFIED Requirements

### Requirement: Podcast libraries use the shared responsive hero presentation
An Audiobookshelf podcast library SHALL use Wide hero when the shared wide geometry conditions fit and selected-row replacement otherwise. Wide detail occupies the right workspace beside a single-column show browser; inline detail replaces the selected show row in one scrolling column. The podcast tab SHALL not reserve a separate detail block or define a surface-specific geometry rule.

The following substitutions SHALL be the only domain changes to that composition:

| TV Shows tab | Audiobookshelf podcast tab |
|---|---|
| Series | Podcast show |
| Series Primary image | Audiobookshelf podcast cover |
| Season selector | `All` / `Played` / `Unplayed` filter selector |
| Episodes in the selected season | Downloaded episodes matching the selected filter |

All other observable layout behavior SHALL match the TV Shows tab, including the hero shell and content padding, image slot, row budgeting, list column count, selected-cell treatment, focus styling, scrolling, and loading placeholder stability.

#### Scenario: Podcast library is displayed
- **WHEN** an Audiobookshelf podcast library and a TV Shows library are displayed at the same terminal dimensions and image setting
- **THEN** both tabs SHALL use the same shared wide or inline presentation for their available geometry
- **THEN** the podcast tab SHALL render podcast shows in the browser positions occupied by Series rows in the TV Shows tab
- **THEN** wide podcast detail SHALL occupy the right workspace beside the single-column browser

#### Scenario: Podcast selection changes
- **WHEN** the user moves selection between podcast shows
- **THEN** the hero or replacement detail SHALL update to the newly selected podcast
- **THEN** the show list SHALL retain provider-native selection identity across loaded-page changes

#### Scenario: Selected show scrolls outside the visible list rows
- **WHEN** the selected podcast's row is outside the visible portion of the lower show list
- **THEN** inline scrolling SHALL keep the selected show and its replacement detail addressable together

#### Scenario: Terminal width crosses the TV list column breakpoint
- **WHEN** the podcast tab crosses a width at which the TV Shows tab changes between one and two list columns
- **THEN** the podcast tab SHALL switch between Wide hero and selected-row replacement at the shared boundary

#### Scenario: Terminal height cannot fit the hero
- **WHEN** the TV Shows tab would suppress its hero because the available height cannot fit the minimum hero and a usable list
- **THEN** the podcast tab SHALL use selected-row replacement and restore the ordinary selected row if detail cannot fit
