# Architecture

## Process and ownership

One Windows GUI executable owns a UI thread and a persistent read-only worker. The UI thread owns the window, tray icon, embedded fonts and GDI resources. The worker retains a query/read-only game handle and checked runtime references, publishes locked observations and posts a window notification. It never draws or changes the game.

Closing the app cancels the worker and joins it before shared state is dropped. Cancellation is checked before memory reads, process enumeration and executable-hash chunks. Monotonic budgets bound discovery to 60 seconds and ordinary sampling operations to five seconds. These are cooperative checks, not forced interruption of individual blocking OS calls. A worker shares the app process; it is not an isolation boundary.

## Crates

| Crate | Responsibility |
| --- | --- |
| wave-overlay | Native window/tray, fonts, rendering, observation consumer and application lifetime |
| reader | OS query/read boundary, runtime discovery, allocation/session guards, bounded local caches and sampling |
| wave-core | Configuration, observation validation and native wave timing calculations |
| observation-model | Bounded envelopes, sequence/generation validation and pure HUD presentation |

The reader's 0.5.0 package version is an internal observation-schema compatibility pin, independent of the app version. Some inherited observation field names mention a helper or get_live_world_state; they are data-validation identifiers, not MCP dependencies or exposed remote tools. None of these crates are published to crates.io.

## Two observation cadences

Menu checks target 50 ms and can promptly move the overlay. Complete world/timer observations target 250 ms. Both run serially, so a full observation can defer a menu check. Menu timestamps cannot refresh world evidence or establish simulation pause.

Timer evidence includes process creation, executable hash, world/object allocation identities, a bounded read window and validated native timing configuration. Torn phase/configuration reads are discarded; identity failures mark data unavailable before reacquisition. Cache files are bounded hints, checked before reuse, and never a saved-state fallback.

Waiting can use a directly verified countdown. Active Heat remaining uses native progress and inverse duration, retaining float32, clipping and transition-tick limits. Actor deadline drift is diagnostic. Normal-menu simulation pause requires pauser/delay/clock evidence. Local wave arrival and exact audiovisual substage cues are not independently established.

The HUD floors seconds and suppresses isolated one-second upward flicker within an unchanged phase/session. It never advances a countdown from wall time. Five-second stale evidence shows unavailable. Exact color, geometry, glyph counts and play/pause anchors are protected by native render checks and presentation tests.

## Native API boundary

Win32 handles close through RAII. The process handle remains on the worker thread using Rc; cancellation and deadlines use thread-safe shared state. Reads validate pointer/size bounds before ReadProcessMemory. The supported path and SHA256 remain pinned. No process write, remote execution, hook, debug-privilege or save-editing path exists.
