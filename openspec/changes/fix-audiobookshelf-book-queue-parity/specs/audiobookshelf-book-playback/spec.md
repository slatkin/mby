## MODIFIED Requirements

### Requirement: Book activation uses ordinary play and enqueue semantics
A selected book SHALL support ordinary play and enqueue actions. Play SHALL select or create the corresponding queue slot and start it when the submission destination is eligible; enqueue SHALL add it without starting playback. Every Player owner SHALL classify a book as requiring Audiobookshelf Service availability before binding or source preparation, using the same admission outcome as an Audiobookshelf podcast episode while retaining book-specific source preparation.

#### Scenario: User plays a selected book
- **WHEN** the user invokes ordinary play on a selected book toward an eligible Player owner
- **THEN** mbv SHALL place or select the book in the Bound queue as a single queue item and start that slot

#### Scenario: User enqueues a selected book
- **WHEN** the user invokes ordinary enqueue on a selected book
- **THEN** mbv SHALL add it through the canonical queue operation without opening a playback session or starting it

#### Scenario: Book is submitted without Audiobookshelf owner context
- **WHEN** a book is submitted to a Player owner that lacks loaded Audiobookshelf setup or credential
- **THEN** the owner SHALL refuse the submission before binding or source preparation
- **AND** SHALL report the same clean admission failure used for other unplayable items

#### Scenario: Book is appended during active playback
- **WHEN** a book is appended to an eligible Bound queue during active playback
- **THEN** the canonical queue SHALL retain the book as one slot
- **AND** playback SHALL use owner-driven active-file projection when that book becomes active

### Requirement: A book's audio files project as one continuous mpv timeline
mbv SHALL hand a book's `audioFiles` to mpv as a single merged timeline using mpv's native multi-file playlist/EDL projection, rather than opening and sequencing separate mpv items or computing per-file offsets in mbv. The book SHALL remain one queue item and one playback session across its entire runtime, including across its constituent files. This projection SHALL apply on both cold Player startup and reuse of an active Player.

#### Scenario: Book has multiple audio files
- **WHEN** the selected book's `audioFiles` span more than one file
- **THEN** mbv SHALL project them as one continuous timeline without a queue-advance or playback-session boundary at each file transition

#### Scenario: Book has a single audio file
- **WHEN** the selected book has exactly one audio file
- **THEN** mbv SHALL play it directly without invoking multi-file projection

#### Scenario: Book starts on a cold Player
- **WHEN** an eligible owner starts a book while no Playback run exists
- **THEN** the new Playback run SHALL use owner-driven active-file projection
- **AND** SHALL preserve the book-relative resume position across all of its audio files

### Requirement: Every opened book playback session is finalized
One idempotent bounded lifecycle path SHALL synchronize final position/listening time and close every opened Audiobookshelf book playback session before discarding it, reusing the same finalization mechanics the podcast playback capability applies to its sessions. Natural completion SHALL close at the book's runtime rather than at a last observed position that may precede the end.

#### Scenario: Book completes naturally
- **WHEN** an Audiobookshelf book reaches natural completion at the end of its merged timeline
- **THEN** mbv SHALL finalize its session at the book's runtime before advancing or stopping
- **AND** Audiobookshelf SHALL be able to record the book as finished

#### Scenario: Active book leaves playback
- **WHEN** the user stops, skips, selects another slot, replaces the queue, or removes the active slot
- **THEN** mbv SHALL finalize the prior session before discarding its lifecycle
