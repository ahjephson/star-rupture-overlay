# Architecture

## Components and ownership

The UI thread owns the window, tray icon, embedded fonts and GDI resources. A persistent worker retains a query/read-only game handle and checked runtime references, publishes observations under a mutex and posts a window notification. Rendering and window operations remain on the UI thread.

| Crate | Responsibility |
| --- | --- |
| wave-overlay | Native window, tray controls, rendering, observation consumption and application lifetime |
| reader | OS query/read boundary, runtime discovery, allocation/session guards, bounded caches and sampling |
| wave-core | Configuration, observation validation and native wave timing calculations |
| observation-model | Bounded envelopes, sequence/generation validation and HUD presentation |

The data flow is game memory → reader → validated observation → timing calculation → HUD presentation. Shared Rust structs and enums define player, world, clock, pause, phase and timing data. All application crates inherit the workspace release version. The observation validator checks its schema and `READER_CONTRACT_VERSION` independently of the release version.

The worker moves typed observations through validation into shared UI state. The 64 KiB observation limit accounts for the envelope, boxed world payload and allocated string capacities. Named Heat settings and a native-stage enum express timing data; fixed-size arrays hold coordinates and substage identifiers in documented order. The HUD presenter compares process, session, world, phase and configuration fields directly, retaining a debounce key until those fields change. JSON is used for settings, discovery cache files and explicit diagnostic output. Configuration file validation preserves accepted compatibility keys and produces operational settings containing only the cache directory and live-reading switch.

## Reader lifetime and observation guards

Closing the app cancels the worker and joins it before shared state is dropped. A condition variable waits until the next sampling or attachment deadline and wakes immediately when cancellation is requested. Cancellation is also checked before memory reads, process enumeration and executable-hash chunks. Monotonic budgets bound discovery to 60 seconds and ordinary sampling operations to five seconds. These checks are cooperative; an individual blocking OS call can delay cancellation. The worker shares the application process.

Menu checks target 50 ms; complete world/timer observations target 250 ms. They run serially, so a full observation can defer a menu check. Menu timestamps affect layout only and cannot refresh timer evidence or establish simulation pause.

Observations carry a bounded read window, process creation identity, executable hash, session generation and sequence. World/object allocation identities and native timing configuration are validated before use. Typed observation-conflict errors discard torn phase/configuration reads without refreshing evidence. Other identity failures invalidate the observation before reacquisition. Cache files contain bounded discovery hints checked against live identities. Checked player/context offsets, boolean masks and clock offsets are resolved into named runtime bindings at attachment; sampling retains identity and numeric-bound checks.

## Wave timing and presentation

Waiting uses a directly verified native countdown. Active Heat timing uses native progress and inverse duration, retaining float32 accumulation, delta clipping and transition-tick limits. Actor deadline drift is diagnostic only. Normal-menu simulation pause requires pauser/delay/clock evidence. Local wave arrival and exact audiovisual cues are not independently established.

| Native phase | HUD label | Countdown target |
| --- | --- | --- |
| Waiting | STABLE | Rupture |
| PreWave | PRE-WAVE | Rupture |
| Moving | APPROACHING | Next native phase |
| Fadeout, elapsed time below 8.5 seconds | INCINERATING | 8.5-second display boundary |
| Fadeout, elapsed time at least 8.5 seconds | COOLING | Native Fadeout end |
| Growback | GROWBACK | Next native phase |

Fadeout elapsed time is its validated native duration minus native remaining simulation time. INCINERATING and COOLING are presentation labels within the same native phase. Unsupported substage timing retains FADEOUT with `--:--`.

The HUD floors seconds and suppresses an isolated one-second upward rounding correction within an unchanged phase/session. A second independent observation confirms that correction. The presenter resets its debounce when the displayed target changes. Countdowns advance with observed simulation time; timer evidence older than five seconds becomes unavailable. Native render diagnostics and presentation tests cover geometry, colours, glyph counts and play/menu anchors.

## Native API boundary

GDI objects use ownership guards that restore selected objects before release. The UI retains its render bitmap and memory device context until the client dimensions change. Rendering tests compare reused and resized surfaces with fresh pixels and check GDI handle counts after cleanup.

Win32 process handles close through RAII and remain on the worker thread using `Rc`. Cancellation and deadlines use thread-safe shared state. Reads validate pointer/size bounds before `ReadProcessMemory`. The supported path and SHA256 are pinned as described in [compatibility](COMPATIBILITY.md). The reader opens the game with query/read permissions under standard user privileges.

The font-count output for `AddFontMemResourceEx` uses writable raw storage even though the generated binding declares a const pointer. A shared reference would permit an invalid aliasing assumption for this output parameter. See the [Win32 font API contract](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-addfontmemresourceex).

## Window positioning and stacking

The HUD is an ordinary, independently owned top-level window. It positions its own HWND immediately above the validated game window without activation. Position and resize operations preserve ordering. Windows above the game remain above the HUD, including partial overlaps. Moving focus to another monitor leaves the HUD over the visible game; an application covering the game also covers the HUD.

Invalid, hidden, minimized or topmost game windows hide the HUD. Menu layout moves it below the menu border; unlocked manual placement overrides automatic positioning until reset.

Desktop foreground, restore and reorder notifications use `SetWinEventHook` with `WINEVENT_OUTOFCONTEXT` and `WINEVENT_SKIPOWNPROCESS`. The callback coalesces events into a posted message without accessing application state or game memory. The UI handler reuses the last validated game HWND and rendered frame without acquiring the reader mutex or recalculating timing/geometry. Thread-affine RAII disables the target before unhooking. Registration failure retains the 100 ms UI timer fallback. See the [Win32 notification contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook).

Notification delivery and desktop composition are asynchronous, so visible updates depend on their scheduling.

## Native diagnostics

`--self-test-stacking` uses four owned off-screen windows. It covers separate/overlapping regions, partial coverage, repeated synchronization, focus return, manual placement, hidden/minimized/destroyed targets and topmost recovery while preserving foreground focus.

`--self-test-window-events` uses two owned off-screen windows and the real callback/posted-message handler. It performs 20 repairs with the reader mutex held and the UI timer disabled, verifies unchanged focus and checks that teardown stops delivery. This diagnostic measures the callback/message path; real game/desktop composition latency requires a live check.

Additional diagnostics check the hidden native window, eight rendered HUD states and embedded icon extraction/loading. These diagnostics operate on owned fixture windows. Live reader and manual appearance checks remain separate; see [release validation](RELEASING.md).
