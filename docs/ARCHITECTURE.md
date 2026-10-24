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

The font-count output for AddFontMemResourceEx uses writable raw storage even though the generated Windows binding declares a const pointer. This follows the API's actual output behavior; a shared reference would permit an invalid aliasing assumption. See the [Win32 font API remarks](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-addfontmemresourceex).

## Game-relative window stacking

The HUD is an ordinary top-level window, without WS_EX_TOPMOST and without assigning a foreign owner/parent. Queued Windows foreground/restore/window-order notifications repair its stacking immediately on the UI thread; the existing100ms UI tick remains a fallback. It places only its own HWND immediately above the validated game window. Position/resize operations preserve ordering; showing the HUD and positioning it in the stack happen together without activation. Windows above the game remain above the HUD, including partially overlapping windows. A browser on another monitor therefore leaves the game's HUD visible, while an app over the game covers it naturally.

Invalid, hidden, minimized or genuinely topmost game windows hide the HUD. The topmost case fails closed rather than making the HUD globally topmost again. The current visible Unreal window was observed non-topmost using a read-only window query. Topmost applications, OS menus and notifications stay above the ordinary HUD. No DirectX hook, injection, cross-process owner relationship or extra game-memory sampling was added. Notification delivery and desktop composition remain asynchronous; no zero-frame display latency is promised.

The --self-test-stacking diagnostic uses four owned off-screen windows and no game. It checks separate/overlapping regions, partial coverage,100 repeated syncs, returning the game above another app, manual placement, hidden/minimized/destroyed targets, accidental topmost recovery and unchanged foreground focus. It changes no actual game/browser windows and launches no second normal HUD. A real two-monitor appearance check still requires intentionally activating the candidate.

## Focus-return notification path

After the player confirmed occlusion worked but reported approximately half a second of reappearance delay, added desktop window notifications through SetWinEventHook with WINEVENT_OUTOFCONTEXT and WINEVENT_SKIPOWNPROCESS. No DLL is supplied or injected, no callback runs inside the game and no extra memory polling is introduced. See [the Win32 notification contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook).

The callback coalesces relevant window-level events into a posted message. It never accesses App, reads memory or performs stacking directly, avoiding reentrant App borrowing. The UI handler reuses its last validated game HWND and existing rendered HUD without acquiring the reader mutex or recalculating timing/geometry. The original ordinary ordering and no-activation policy remain. Notifications from the overlay process are excluded to avoid feedback. Thread-affine RAII disables the target before unhooking; registration failure keeps the100ms timer fallback and reports it.

The --self-test-window-events diagnostic uses two owned off-screen windows, allowed synthetic object-reorder notifications and the real callback/posted-message handler. It runs20 repairs with the reader mutex held and no UI timer, verifies unchanged foreground focus and checks teardown stops delivery. Its measured native scheduling latency is not a measurement of game/desktop composition latency. The real foreground event is system-generated; the test does not fabricate it or focus an actual app.
