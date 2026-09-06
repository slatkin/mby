# audiobookshelf-service-setup Specification

## Purpose
Defines secure Audiobookshelf API-key setup, identity validation, runtime availability, and connection testing without introducing catalog or playback behavior.
## Requirements
### Requirement: Audiobookshelf setup validates an API-key identity before commit
mbv SHALL accept an Audiobookshelf server URL and API key through Services settings, send the key as `Authorization: Bearer <api-key>` to `GET /api/me`, and commit the setup only when the response confirms the associated active user. Audiobookshelf setup SHALL NOT offer a username/password flow.

#### Scenario: New Audiobookshelf setup succeeds
- **WHEN** the user submits a server URL and API key that `/api/me` accepts for an active user
- **THEN** mbv SHALL persist the validated Audiobookshelf setup and Service credential
- **THEN** Audiobookshelf SHALL become Ready

#### Scenario: New Audiobookshelf credential is rejected
- **WHEN** the user submits an API key that the configured server rejects
- **THEN** mbv SHALL report the authentication failure without persisting the attempted setup or API key
- **THEN** the setup input SHALL remain available for correction

#### Scenario: New Audiobookshelf server is unreachable
- **WHEN** `/api/me` cannot validate a submitted setup because the server is unreachable
- **THEN** mbv SHALL report the connectivity failure without persisting the attempted setup or API key

#### Scenario: Failed candidate setup does not replace working setup
- **WHEN** validation of a candidate server URL or API key fails while Audiobookshelf already has a working setup
- **THEN** mbv SHALL preserve the working setup, credential, runtime identity, and Service-owned state

### Requirement: Audiobookshelf credentials remain isolated local secrets
mbv SHALL persist the Audiobookshelf API key only in Audiobookshelf's mode-`0600` Service secret file. The API key SHALL NOT be written to general configuration, ctrl messages, logs, or shared-state storage.

#### Scenario: Audiobookshelf setup is persisted
- **WHEN** Audiobookshelf setup validates successfully
- **THEN** mbv SHALL write the API key to Audiobookshelf's Service secret file with mode `0600`
- **THEN** non-secret setup SHALL identify the server without containing the API key

#### Scenario: Diagnostic output describes an Audiobookshelf failure
- **WHEN** mbv reports or logs an Audiobookshelf request failure
- **THEN** the diagnostic SHALL NOT contain the API key or the request's Authorization value

### Requirement: Configured Audiobookshelf initializes independently
After TUI entry, mbv SHALL validate a configured Audiobookshelf Service through `/api/me` independently of every other Service. It SHALL expose Connecting while validation is pending, Ready with the authenticated user after success, Needs authentication after explicit credential rejection, and Unavailable after connectivity or server failure.

#### Scenario: Persisted Audiobookshelf setup connects
- **WHEN** the TUI starts with a configured Audiobookshelf Service whose `/api/me` request succeeds
- **THEN** Audiobookshelf SHALL transition through Connecting to Ready
- **THEN** its runtime identity SHALL identify the authenticated server and user

#### Scenario: Persisted Audiobookshelf server is unavailable
- **WHEN** background validation cannot reach the configured server or the server cannot complete `/api/me`
- **THEN** Audiobookshelf SHALL become Unavailable
- **THEN** mbv SHALL preserve its server setup and API key

#### Scenario: Persisted Audiobookshelf key is rejected
- **WHEN** the configured server explicitly rejects the persisted API key
- **THEN** Audiobookshelf SHALL become Needs authentication
- **THEN** mbv SHALL preserve its server setup and Service-owned state but delete the rejected API key

#### Scenario: Stale connection result arrives
- **WHEN** a connection result belongs to an Audiobookshelf setup that has since been repaired, replaced, or removed
- **THEN** mbv SHALL ignore that result without changing the current setup, credential, identity, or Service state

### Requirement: Audiobookshelf connection can be tested from Services settings
Services settings SHALL provide a Test connection action for configured Audiobookshelf. The action SHALL call `/api/me` and present a concise result containing the configured server and authenticated user on success, while applying the same failure classification and credential-retention rules as background validation.

#### Scenario: Audiobookshelf connection test succeeds
- **WHEN** the user tests a configured Audiobookshelf Service and `/api/me` succeeds
- **THEN** mbv SHALL report the configured server and authenticated user
- **THEN** it SHALL leave the working setup and API key unchanged

#### Scenario: Audiobookshelf connection test cannot reach the server
- **WHEN** the test request fails because the configured server is unavailable
- **THEN** mbv SHALL report a connectivity failure and preserve the setup and API key

#### Scenario: Audiobookshelf connection test rejects the key
- **WHEN** the test request explicitly rejects the persisted API key
- **THEN** mbv SHALL report an authentication failure, preserve the server setup, delete the rejected key, and show Needs authentication

### Requirement: Audiobookshelf follows the singleton Service lifecycle
mbv SHALL repair, replace, and remove Audiobookshelf through the Service lifecycle established for Remote Services. Repair of the same configured server SHALL preserve Service-owned state; replacement with a different server and removal SHALL require confirmation and clear Audiobookshelf-owned setup, credentials, runtime identity, and local state as applicable.

#### Scenario: Audiobookshelf authentication is repaired
- **WHEN** a replacement API key validates against the existing configured server
- **THEN** mbv SHALL retain the Service identity and Service-owned state
- **THEN** Audiobookshelf SHALL become Ready with the newly validated credential

#### Scenario: User replaces the Audiobookshelf server
- **WHEN** the user validates and confirms a setup for a different Audiobookshelf server
- **THEN** mbv SHALL clear state belonging to the previous server before committing the replacement
- **THEN** identifiers from the previous server SHALL NOT be resolved against the replacement server

#### Scenario: User removes Audiobookshelf
- **WHEN** the user confirms Audiobookshelf Service removal
- **THEN** mbv SHALL delete its setup, API key, runtime identity, and Service-owned local state
- **THEN** Audiobookshelf SHALL become Not configured without affecting Emby or Feeds

### Requirement: Setup remains identity-only
This capability SHALL use Audiobookshelf only to validate the authenticated user. It SHALL NOT request libraries or catalog contents, create queue items, open playback sessions, connect to Socket.IO, or add Audiobookshelf media support to a Player owner.

#### Scenario: Audiobookshelf reaches Ready
- **WHEN** `/api/me` validates a configured Audiobookshelf Service
- **THEN** mbv SHALL expose the validated Service identity and connection actions
- **THEN** it SHALL NOT load or display Audiobookshelf libraries or media

### Requirement: Audiobookshelf setup carries a persisted revision
Every committed owner-local Audiobookshelf setup SHALL contain a persisted unsigned 64-bit `revision`, initially `1` and incremented exactly once for every successful initial setup, same-server repair, or different-server replacement. The persisted revision SHALL identify the commit to another process. It SHALL be distinct from the in-memory setup generation, which advances for each runtime install or replacement so stale asynchronous Audiobookshelf work cannot affect the current runtime.

#### Scenario: Initial setup commits
- **WHEN** a validated Audiobookshelf candidate commits for the first time
- **THEN** the persisted setup SHALL carry revision `1`

#### Scenario: Same-server repair commits
- **WHEN** a validated candidate for the already installed server commits
- **THEN** the persisted setup SHALL carry the next revision with the replacement credential
- **THEN** Audiobookshelf-owned state SHALL be preserved

#### Scenario: Different-server replacement commits
- **WHEN** a validated candidate for a different server commits
- **THEN** the persisted setup SHALL carry the next revision
- **THEN** state owned by the previous setup SHALL be cleared before the replacement is usable

### Requirement: Daemon owners load Audiobookshelf owner context from their own storage
A Local daemon and packaged `mbvd` SHALL load their owner-scoped Audiobookshelf setup, API key, setup generation, and stable device identity from their own storage without transporting credentials through ctrl. Constructing the owner context SHALL NOT authenticate; a daemon SHALL start and remain Service-independent even when the configured server is unavailable or the setup is absent.

#### Scenario: Owner has a configured Audiobookshelf setup
- **WHEN** a Local daemon or packaged `mbvd` starts with a persisted Audiobookshelf setup and credential
- **THEN** it SHALL construct an Audiobookshelf owner context holding setup, API key, generation, and stable device identity
- **THEN** it SHALL NOT authenticate or enable Audiobookshelf playback

#### Scenario: Owner has no Audiobookshelf setup
- **WHEN** a daemon owner starts without an Audiobookshelf setup
- **THEN** it SHALL hold no Audiobookshelf owner context
- **THEN** every unrelated Service and core daemon behavior SHALL remain available

#### Scenario: Credentials stay out of ctrl
- **WHEN** a daemon owner loads or reconciles Audiobookshelf owner context
- **THEN** the API key and any Authorization value SHALL NOT appear in ctrl messages, queue state, or logs

#### Scenario: Device identity is stable
- **WHEN** an owner constructs Audiobookshelf owner context
- **THEN** it SHALL load the same stable, non-secret device identifier used by every Audiobookshelf playback session request

### Requirement: Committed Audiobookshelf owner state is reconciled by rereading owner storage
Committed Audiobookshelf owner state SHALL be reconciled by signaling what changed and making the owner reread its own storage. The owner SHALL compare the persisted revision to the signaled revision, apply the committed state when they match, and reject a stale signal. Bare mode SHALL invoke the same semantic operation directly, without a ctrl round trip.

#### Scenario: Owner applies a matching revision
- **WHEN** an owner receives a reconciliation signal whose revision equals the persisted Audiobookshelf setup revision
- **THEN** the owner SHALL reread its own setup and secret and install the committed runtime state with an advanced generation

#### Scenario: Owner rejects a mismatched revision
- **WHEN** an owner receives a reconciliation signal whose revision differs from the persisted setup revision
- **THEN** the owner SHALL reject the signal and keep the installed runtime unchanged

#### Scenario: Bare mode applies directly
- **WHEN** bare-mode mbv commits an Audiobookshelf setup, repair, replacement, or removal
- **THEN** the in-process owner SHALL apply the committed state directly without signaling another process

### Requirement: Bare-mode Audiobookshelf changes apply to a running same-user Local daemon
After mbv commits an Audiobookshelf setup, repair, replacement, or removal through Services, a running same-user Local daemon SHALL adopt the committed state when possible by rereading its own storage. The durable commit SHALL be preserved whether or not a running Local daemon acknowledges it.

#### Scenario: Running Local daemon adopts the commit
- **WHEN** mbv commits an Audiobookshelf change while a same-user Local daemon is running
- **THEN** the Local daemon SHALL reread its own Audiobookshelf storage and install the committed state with an advanced generation

#### Scenario: No Local daemon is running
- **WHEN** mbv commits an Audiobookshelf change while no same-user Local daemon is running
- **THEN** the commit SHALL succeed
- **THEN** the next Local daemon startup SHALL load the committed state

#### Scenario: Live reconciliation is unavailable
- **WHEN** mbv commits an Audiobookshelf change but the running Local daemon cannot acknowledge rereading it
- **THEN** mbv SHALL preserve the durable commit
- **THEN** mbv SHALL report clearly that a daemon restart is required and SHALL NOT claim the change is active in the daemon

### Requirement: Audiobookshelf replacement and removal clean owner state without daemon playback
A different-server Audiobookshelf replacement and an Audiobookshelf removal SHALL advance the owner generation and clear Audiobookshelf-owned state for that owner while leaving unrelated persisted media intact. These operations SHALL NOT make a daemon owner eligible to bind or play Audiobookshelf podcast episodes.

#### Scenario: Different-server replacement clears previous Audiobookshelf state
- **WHEN** a validated different-server Audiobookshelf replacement commits
- **THEN** state owned by the previous setup SHALL be cleared before the replacement is active
- **THEN** identifiers from the previous server SHALL NOT be resolved against the replacement server

#### Scenario: Removal clears setup, credential, and owned state
- **WHEN** Audiobookshelf is removed for an owner
- **THEN** the setup, API key, and Audiobookshelf-owned state SHALL be deleted
- **THEN** Emby, Feeds, and unrelated persisted media SHALL remain

#### Scenario: Daemon playback stays disabled
- **WHEN** a daemon owner loads or reconciles Audiobookshelf owner context
- **THEN** the owner SHALL continue treating Audiobookshelf podcast episodes as unplayable
- **THEN** no Audiobookshelf item SHALL enter a daemon Bound queue or start playback


### Requirement: Audiobookshelf teardown purges every Audiobookshelf queue-item shape
Audiobookshelf Service replacement or removal SHALL finalize an active Audiobookshelf lifecycle and remove every Audiobookshelf-owned slot from the affected owner's live and persisted queue state. This SHALL include both podcast episodes and books while preserving Emby items and Feed entries.

#### Scenario: Service removal with mixed Audiobookshelf queue
- **WHEN** Audiobookshelf is removed while the queue contains podcast episodes, books, Emby items, and Feed entries
- **THEN** every Audiobookshelf episode and book slot SHALL be removed from live and persisted queue state
- **AND** every Emby item and Feed entry SHALL remain in canonical order

#### Scenario: Service replacement while a book is active
- **WHEN** Audiobookshelf is replaced with a different server while a book playback session is active
- **THEN** the prior book session SHALL be finalized before its slot and owner context are discarded
- **AND** the book SHALL NOT survive for resolution against the replacement server

#### Scenario: Interactive process removes Audiobookshelf
- **WHEN** the interactive process removes Audiobookshelf from either a Composed queue or its in-process owner's Bound queue
- **THEN** both Audiobookshelf queue-item shapes SHALL be purged through the same Service-owned-state rule
