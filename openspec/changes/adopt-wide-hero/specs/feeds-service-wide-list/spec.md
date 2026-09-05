## MODIFIED Requirements

### Requirement: Wide rail has established semantic framing
The Wide Feeds Service left rail MUST paint the existing semantic surface/backdrop and border treatment used by the Wide hero list panel. The treatment MUST be applied by the arrangement/render boundary, not by arbitrary screen colors. Its semantic border/background MUST NOT overwrite the first visible heading or the final visible FeedEntry/marker at the 82-column threshold or a larger Wide width.

#### Scenario: framed rail
- **GIVEN** a Wide Feeds Service panel with visible entries
- **WHEN** its buffer is rendered
- **THEN** the rail background and border cells match the established semantic roles and remain present behind the list
