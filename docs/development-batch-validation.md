---
title: Development batch validation
permalink: /development-batch-validation/
description: Implementation checklist, synthetic verification, and attended acceptance for the next substantial batch.
---

# Development batch validation

This checklist covers the October 9, 2026 reliability and daily-use batch requested
for Zap Faster. Source changes and synthetic fixtures are distinguishable from
phone delivery, physical audio, and native desktop acceptance. The implementation
is committed as `d9f0bb54481a245d6bb38e1363553ca1d728ae3c` on local `main`.
This separate validation record tracks the completed checks and the concrete
Linux test-linking environment blocker.

The source version remains **0.21.0**. The user selected **0.30.0** as an acceptable
future release name. This implementation task does not bump the version, tag or
publish a release, install a build, or replace the running client. Changes belong
in focused, linear commits on `main`.

## Implementation checklist

| Requested item | Implemented behavior and synthetic evidence | Remaining acceptance or blocker |
| --- | --- | --- |
| Archive/unarchive, #437/#440 | Encrypted durable intentions; local pending state overlays remote snapshots; acknowledgement, watermark and queue deletion are atomic. Archive, read and star writes serialize through `regular_low` with collection backoff. Tests cover replay, storage failure, newest remote retry, stale completion, encrypted restart, and privacy-id migration of both visible state and pending ownership. | Phone convergence and the absent-record snapshot blocker below. Diagnostics alone are not proof of repair. See [archive synchronization](archive-sync.md). |
| Privacy-id/Business sends, #383 | Device resolution refreshes once only for typed `NoRecipientDevice` or `PrimaryDeviceRejected` failures before transmission. Tests preserve the existing queue and prove acknowledgement timeouts and uncertain transport failures send once. Recipient addressing still comes from the protocol library. | Real direct and Business recipients, both PN and LID addressing. A successful send call or server acknowledgement does not prove recipient delivery. |
| Group rename crash, #313 | Current rename path has synthetic Enter/Escape, permissions and stale metadata coverage. Twelve synthetic render updates pass through the actual AccessKit consumer tree, including focus changes when the editor opens and closes. The targeted Windows test passed. | The original panic was not reproduced on current synthetic content. This rules out that fixture, not every native desktop path. Attended reproduction remains necessary before declaring the reported crash fixed. |
| Presence, #461 | `online_sent` records only a successful network operation; one in-flight operation, bounded timeout, failure retry and connection generation prevent stale connection completions from changing presence. Tests cover focus changes, failure, reconnect and account routing. | Phone push behavior while switching windows, accounts and reconnecting. |
| Linux delayed tray, #429 | Hidden startup waits for the tray watcher with a deadline. A watcher appearing in time keeps the window hidden; absence opens a recovery window. Synthetic tests cover both paths. | Actual Linux desktop with a delayed or missing StatusNotifier watcher. |
| Linux browser reaping, #446 | Browser launch follows the same `gio`/`xdg-open` path as file opening, with bounded launcher verdict and a waiter that reaps a long-running child. There is no global `SIGCHLD` ignore. | Linux browser success/failure, launcher exit and independent subprocess management. |
| Audio and microphone, #421/#382 | Existing default-device routing and stream-error reopening are retained. The pinned microphone iterator waits for samples; end-of-stream logs give content-free failure diagnostics. No temporary-empty-read retry was added. Existing synthetic silence and microphone checks require physical hardware and are ignored by ordinary test runs. | Default output switching, physical microphone capture, unplug/error handling and permissions. |
| Original documents, #469/#362 | Original image files retain bytes and filename as documents; clipboard pixels encode as lossless PNG named `clipboard.png`. Worker tests verify original byte preservation without decoding. | Receiver-side byte/name comparison and clipboard RGBA comparison. Clipboard images do not promise original EXIF or filename. See [original documents](original-documents.md). |
| Refused attachments | Recovery stays bound to the originating account and chat, preserving document choices, order, first caption, mentions and quote. A newer draft stays available. Synthetic tests cover account switch, cancellation, stale send, hidden account, lock and deletion behavior. Durable queued uploads keep their existing uncertain-send semantics. | Pre-transmission refusal and recovery while switching accounts and editing a newer draft. |
| Phone labels, #449/#408 | Offline international formatting uses the number's own country metadata. Canonical recipient digits are unchanged; unknown formats retain every digit with a leading plus. | Read labels in direct chats, account picker and dialogs with several international fixtures. |
| Edit caret, #456 | Editing moves the caret to the Unicode character count at the end. | Synthetic Unicode regression plus keyboard acceptance with accents and multi-code-point emoji. |
| Submenu sizing, #435 | Translated submenu labels determine width, capped by the viewport. Narrow viewport and long-label fixtures cover clipping. | Windows, Linux and macOS menus at narrow widths, high scale and long translations. |
| Notification previews, #376 | Per-account sender/content controls mask title, body, avatar and custom sound consistently, including hidden-account arrivals. App/chat lock and screen privacy remain enforced. | All four sender/content combinations, account switches and locked screens through the native service. |
| macOS sound/DND, #211 | Notification sounds are prepared for the native notification service instead of independent playback. Synthetic tests decode built-in audio to PCM and reject malformed sound inputs. | Focus/Do Not Disturb with built-in and custom sounds; no audible bypass. |
| macOS Dock/banner, #368/#346 | Main-thread Dock label uses unread-chat count. Existing native registration already selects the packaged bundle ID; the packaging identity test passes. Added content-free warning diagnostics for registration/submission failures. Demo/test builds do not publish badges. | No current bundle-ID mismatch was found and the missing-banner report was not reproduced. Signed bundle, notification permission/banner settings, visible banner, Focus and Dock clearing still require native acceptance. |
| Stars, #236 | Durable ordered intent; serialized writes; remote action timestamp ordering; history seeds only unknown state. Paginated account-local cross-chat list, navigation, locked-chat exclusion and revoke/delete/clear cleanup. Synthetic storage, worker, application and demo fixtures cover race, retry, restart, mapping and privacy behavior. | Bidirectional phone sync, list over 200 rows and account switch. Absent-record snapshot recovery remains blocked upstream. See [starred and pinned messages](starred-pinned-messages.md). |
| Pins, #234/#187 | Library durations of 24 hours, 7 days and 30 days; three-pin client cap; server-refused actions do not produce confirmed pins. Banner navigation, timeline notices, millisecond ordering, cleanup and active expiry refresh. Uncertain pin sends are not blindly resent. Synthetic fixtures include exact/same-second ordering, late completion, deletion and privacy. | Phone verification of all durations, actual three-pin limit, group permissions and remote updates. The protocol enum and local cap do not establish complete mobile parity. |
| Background history, #194 | Per-account opt-in; at least 20 seconds between requests; fair rotation over at most ten current eligible chats; five requests/chat and 50/account per process session. Reader and poll recovery take priority. PDO IDs and a bounded early-answer buffer preserve ownership across timeout/reconnect; missing IDs use conservative handling. Locks, exhaustion and deletion barriers exclude work. Synthetic tests cover cancellation, failure identity, stale/duplicate callbacks, restart policy and canonical mapping. | Online phone response, pacing, late-response ownership and unchanged reader viewport. Recovery is not guaranteed and does not enable automatic media downloads. See [background recovery](background-history.md). |

## Concrete protocol blocker

The pinned whatsapp-rust recovery path emits replayed app-state records present
in a recovered snapshot. It does not provide an application-level reconciliation
event for records that disappeared from the replacement snapshot. An archive or
star value previously stored locally cannot safely be cleared merely because the
application did not see a replayed record. Clearing everything before replay
would also risk overwriting pending local intentions or showing incomplete
snapshot state as final state.

Durable local writes, timestamp ordering and replayed-record application are
implemented here. Full absent-record snapshot convergence needs an upstream
protocol-library fix and its failure/restart tests. The pinned library documents
this limitation in its [snapshot recovery implementation](https://github.com/oxidezap/whatsapp-rust/blob/ee89f5ad9cf0553825f201f0e264ad32c4b73966/src/features/app_state_resync.rs#L48-L73).
No dependency was vendored,
forked or patched in this repository. This remains an explicit blocker to full
snapshot convergence, rather than a claim that the whole archive/star repair is
complete.

Phone history responses can omit their PDO session id. In that case this client
archives ambiguous pages silently rather than letting a late automatic page
complete a newer reader spinner. A newer reader request may need a matching
session ID or a reconnection. This is a documented protocol ambiguity, not a
promise of unlimited or guaranteed recovery.

## Required gates and evidence record

Results below come from the integrated source and synthetic fixtures, with exit
status, counts and ignored hardware checks recorded separately. The prior
release's CI results are historical context only. No new release, installation
or remote publication was performed.

| Gate | Current result |
| --- | --- |
| `cargo fmt --all --check` | Pass on Windows, exit 0 |
| `cargo clippy --locked --all-targets -- -D warnings` | Pass on Windows, exit 0 |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | Pass on Windows, exit 0 |
| `cargo test --locked --all-targets` | Pass on Windows: 1,528 library, 8 binary and 2 integration tests passed; 10 physical/native checks ignored |
| `cargo test --locked --all-targets --all-features` | Pass on Windows: 1,528 library, 10 binary and 2 integration tests passed; 10 physical/native checks ignored |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --all-features --no-deps` | Pass on Windows, exit 0 |
| Translation extraction and repository translation checks | Pass: gettext extraction, `--check`, format checks for all ten catalogs, and no untranslated or fuzzy new entries |
| Synthetic demo/layout checks, including `starred`, `star-menu`, `pinned`, `background-history` and group rename | Full Windows and macOS library suites passed. Every-surface layout includes the six new demo states. Linux captures were not produced because of the linking environment blocker below |
| Windows source compilation/tests | All six required gates passed on final source. Both suites passed; final expanded pin-error translation regression included |
| Linux source compilation/tests | Docker Ubuntu 24.04 all-target/all-feature check passed, exit 0. Library tests, executable/integration tests and synthetic captures did not complete because of the linking environment blocker below; no Linux runtime pass is claimed |
| macOS source compilation/tests | Apple Silicon SSH scratch all-target/all-feature check passed on latest source. Full all-feature library suite: 1,521 passed, 10 physical/native checks ignored. Final expanded pin-error locale regression also passed. No installed application touched |
| Focused local `main` commits | Implementation: `d9f0bb54481a245d6bb38e1363553ca1d728ae3c`. This checklist is retained in the subsequent documentation-only validation commit |

The pre-existing macOS failure at baseline `7befc5b` in
`a_contact_typing_steps_the_window_only_with_a_screen_reader` included a zero-delay
repaint from egui's independent composer caret blink. Asset settling determined
which blink boundary the fixture sampled. This batch keeps the caret visible
in that focused fixture while retaining the no-reader zero-delay assertion,
screen-reader 75 to 100 ms assertion, and typing-dot repaint-origin assertion.
Product animation behavior and dependency code are unchanged. The integrated
targeted fixture passed in the macOS full library suite.

Captured gate output is retained in the task scratch directories:

- Windows: `H:\Scratch\Codex\zap-faster-batch-20261009\`, including the
  default/all-feature test logs, both Clippy logs, rustdoc and translation checks.
- macOS: `H:\Scratch\Codex\zap-faster-macos-20261009\`, including the
  all-feature library suite and final source/locale checks with exit-status files.
- Linux: `H:\Scratch\Codex\zap-faster-linux-check.log`,
  `H:\Scratch\Codex\zap-faster-linux-tests-serial.log` and
  `H:\Scratch\Codex\zap-faster-linux-source.sha256`.
  The source manifest verifies all 220 copied source/build/asset files against
  implementation commit `d9f0bb5`.

The first concurrent Linux test build encountered severe disk/paging contention
while producing several executables. It was deliberately stopped with exit 137;
this is an environment cancellation, not a compiler or test failure. The retry
uses one build job, unchanged compiler flags and the preserved cache in the
task-only Ubuntu 24.04 Docker container. No host resource settings were changed.

The serial retry also encountered severe paging at its single final library-test
link. At 17:36:22 UTC, the linker had 3m16s elapsed, 8s CPU and 11,594,014,720
bytes read. At 17:38:09 UTC, it had 5m04s elapsed, 12s CPU and 18,632,896,512
bytes read, with no output written. Both samples showed an uninterruptible I/O
wait; the 2 GiB swap had become saturated. About 7.04 GB of additional reads in
107 seconds with only four seconds of CPU show severe paging, not a source
diagnostic or a completed test result. Samples are retained as
`H:\Scratch\Codex\zap-faster-linux-link-progress-a.log` and
`H:\Scratch\Codex\zap-faster-linux-link-progress-b.log`. The retry was
deliberately cancelled with exit 137 without weakening flags, clearing the build
cache or changing host resources; the cancellation record is
`H:\Scratch\Codex\zap-faster-linux-serial-cancellation.txt`.
Linux runtime validation remains explicitly unverified.

## Attended acceptance on Windows, Linux and macOS

Use test accounts, synthetic conversations and fixtures only. Do not inspect
personal chat contents or export private logs. Record platform, application
commit/build, observed result and whether the phone confirmed the outcome.
Retain only count/state/error-category evidence without phone numbers or content.
Do not replace the installed client as part of these implementation checks.

1. **Archive, stars and recovery:** From each device, archive/unarchive and
   star/unstar a synthetic message. Disconnect before acknowledgement, change the
   intention again, restart and reconnect. Compare the final phone and desktop
   states. Replay older history and confirm it cannot undo a newer action. Run the
   absent-record snapshot scenario separately and record the known blocker.
2. **Starred list and privacy:** Star over 200 synthetic rows across several
   chats, load every page and open a row beyond the initially loaded timeline.
   Switch accounts while writes await completion. Verify the hidden account's
   result cannot change the visible composer, dialog, selection or read state.
   Lock the app and a chat: list/banner previews must disappear.
3. **Pins:** Send 24-hour, seven-day and 30-day pins and verify each duration on
   the phone. Test ordinary member/admin roles and restricted group settings;
   refusals must not add a confirmed pin. Reach three active pins and test a
   fourth from both devices. Verify banner navigation and timeline notices.
   Use a short-expiry synthetic fixture to observe expiry without waiting a day.
   Revoke, delete or clear while requests wait; late callbacks must not restore
   pins, stars or previews. Check the phone before retrying an uncertain pin.
4. **Business/PN/LID delivery:** Send one labeled synthetic message each to a
   normal recipient and a consenting Business test recipient whose PN/LID mapping
   is known, then one whose mapping is newly learned. Record recipient resolution,
   server acknowledgement and receiving-device delivery separately. Confirm only
   typed pre-transmission device failures refresh once. A missing acknowledgement
   or uncertain transmission must remain unconfirmed without duplicate sends.
5. **Presence:** Focus and unfocus the window, hide it, switch accounts and drop
   the connection while a presence operation is waiting. Verify stale completions
   cannot restore the wrong account's online state, retries recover, and the phone
   resumes notifications while the desktop reader is away.
6. **Background history:** Enable only one account. Verify at least 20-second
   pacing and alternating eligible chats, five requests/chat and 50 total per
   session. Confirm reconnect/toggle do not reset budgets. Keep the timeline open:
   background pages must not scroll it or use its spinner. Disconnect the phone
   until timeout, then deliver late pages. Ask explicitly for another chat's
   history and recover a poll simultaneously; each result must keep its owner.
   Turn recovery off mid-request, lock the app, lock/clear a chat, switch accounts
   and restart. Verify exclusions, saved opt-in and no unsolicited timeout notices.
7. **Documents and recovery:** Send a transparent image file with known bytes
   and filename, clipboard RGBA pixels as a document, and a PDF in that order.
   Compare received bytes/name for the file and decoded RGBA for the clipboard PNG.
   Refuse a send before transmission while switching accounts and writing a newer
   caption. Confirm original order, caption, quote and document choices return only
   to their originating account; the newer draft remains available.
8. **Daily UI:** Check several international phone labels without changing any
   recipient digits. Edit accented text and multi-code-point emoji, type at the
   end and verify the inserted text follows the existing message. Open long
   translated submenus at narrow viewport sizes and high display scale. Rename a
   synthetic group with Enter and cancel with Escape; retain actual crash evidence
   if the reported failure occurs.
9. **Physical audio:** With synthetic silence or consenting test speech only,
   play a voice message, change the system default output, start another clip and
   verify the new output is used. Exercise an output-stream failure and reopening.
   Start/stop/send a microphone recording, deny microphone permission, and unplug
   the selected input while recording. Confirm meaningful failure state, no hung
   iterator and no content/device identifiers in shipped diagnostics. Ordinary
   synthetic CI cannot establish these physical outcomes.
10. **Native notifications:** Test every sender/content preview combination,
    hidden-account arrivals, app lock and locked chats. Where native click handling
    is supported, click a notification and verify account/chat/message navigation. On Windows verify toast permissions,
    taskbar badge and tray recovery. On Linux verify the notification daemon and
    launcher badge where supported. On macOS use a signed bundle; check system
    notification permission and banner settings, confirm an actual visible banner,
    enable Focus/Do Not Disturb and verify built-in/custom sounds obey it. Mark
    chats read and switch accounts to check Dock unread-chat counts and clearing.
11. **Linux startup and children:** Start hidden before a StatusNotifier watcher
    is available. If the watcher arrives within the deadline, confirm the window
    stays hidden and the foreground application's focus does not change. Repeat
    without a watcher; the recovery window must become available at the deadline.
    Open a synthetic HTTPS link with `gio`, then with the fallback launcher. After
    a long-running child exits, inspect only process identities/state to verify
    it is reaped. Confirm unrelated decoder/helper children remain waitable.

Compilation and CI are recorded separately from these attended outcomes. A
platform is runtime-accepted only after its corresponding checks have evidence.

### Exact commands for attended synthetic checks

To finish the blocked Linux checks on a disposable Linux build machine with the
repository's native build prerequisites, Xvfb, fonts and software OpenGL installed:

```sh
cargo check --locked --all-targets --all-features -j1
cargo test --locked --all-features --lib -j1 -- --test-threads=4
cargo test --locked --all-targets --all-features -j1 -- --test-threads=4
cargo build --locked --all-features --bin zap-faster -j1
mkdir -p .cache/demo-captures
for page in starred star-menu pinned background-history original-documents international-sender; do
  timeout 45s xvfb-run -a --server-args="-screen 0 1280x900x24" \
    env LIBGL_ALWAYS_SOFTWARE=1 ./target/debug/zap-faster --demo \
    --demo-page "$page" --demo-size 1100x760 --demo-shot-delay 3500 \
    --demo-shot ".cache/demo-captures/$page.png" || exit "$?"
done
```

Inspect all six PNGs for clipping, readable labels and correct synthetic state.
These demo commands use offline sample content and do not prove phone delivery
or desktop tray/notification acceptance. Remove only this generated capture
directory after recording the evidence.

Run these individually when you are ready for the stated device effect. They
target existing opt-in tests, without opening a personal archive:

```sh
# Opens the default physical output using synthetic silence.
cargo test --locked --all-features --lib audio::tests::synthetic_silence_uses_the_current_default_and_stream_errors_request_reopening -- --ignored --exact --nocapture
# Records one second from the microphone. Use consenting test speech only.
cargo test --locked --all-features --lib audio::tests::records_a_second_on_this_machine -- --ignored --exact --nocapture
# Submits one synthetic desktop notification; inspect its actual native delivery.
cargo test --locked --all-features --lib notify::tests::shows_one_on_this_desktop -- --ignored --exact --nocapture
```

On a disposable Linux test desktop, this checks the no-watcher recovery path on
an isolated D-Bus session and isolated application state. The linking window
should appear after 15 seconds. Close the test process when finished, then remove
only the generated acceptance directory. Existing instances and their archives
remain outside this session:

```sh
task_profile="$PWD/.cache/tray-acceptance-$$"
mkdir -p "$task_profile"/{config,data,state,cache,runtime}
chmod 700 "$task_profile/runtime"
dbus-run-session -- env XDG_CONFIG_HOME="$task_profile/config" \
  XDG_DATA_HOME="$task_profile/data" XDG_STATE_HOME="$task_profile/state" \
  XDG_CACHE_HOME="$task_profile/cache" XDG_RUNTIME_DIR="$task_profile/runtime" \
  ./target/debug/zap-faster --start-hidden
```

For delayed-watcher acceptance, use a clean test session with its normal tray
panel initially stopped. Start the isolated application and, after five seconds,
its tray panel on the same test D-Bus session. It should stay hidden and become
reachable from the tray. Repeat with a delay beyond 15 seconds; the recovery
window should already be available. The correct panel command depends on the
desktop, so do not stop or replace your working desktop's panel for this check.
