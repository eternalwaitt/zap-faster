---
title: Archive synchronization
permalink: /archive-sync/
description: Durable archive intentions, remote updates, and recovery limits.
---

# Archive and unarchive synchronization

Archive changes are saved in the account's encrypted archive before the interface
shows them. A single queue shares the `regular_low` app-state collection with
private read state and starred messages. A failed write backs off the collection;
pending intentions survive restarting the app.

While a local change is pending, its intended archive state overlays phone
updates and recovered snapshots. Remote timestamps still advance the archive's
watermark. Acceptance reasserts only the exact queued intention, advances that
watermark, and removes the queued row in one transaction. An older completion
cannot discard a newer local request. Once the queue is empty, a newer phone
action applies normally. Deleting a chat removes its queued archive changes;
learning a privacy-id mapping migrates them to the canonical chat.

A remote update that cannot be saved is retained for local retries while the
process runs, without sending another action. It becomes durable only when the
write succeeds. A crash before that success still depends on protocol recovery,
with the snapshot limitation below.

Snapshot recovery in the pinned protocol library replays present records but
does not expose reconciliation of records absent from a replacement snapshot.
A missing record therefore cannot prove that an older locally stored archive
state was cleared on the phone. Fixing this recovery case requires an upstream
protocol change; queued-write repair alone does not prove full convergence.
This is an explicit limitation of the pinned library's
[`AppStateResyncMode::Snapshot` contract](https://github.com/oxidezap/whatsapp-rust/blob/ee89f5ad9cf0553825f201f0e264ad32c4b73966/src/features/app_state_resync.rs#L48-L73):
an empty replay can also mean the server did not serve the requested snapshot.

Automated synthetic tests cover stale callbacks, local request replacement,
remote updates during failure, replay after acceptance, shared collection
serialization, and encrypted restart using a mock key. Phone-side convergence
requires attended testing: archive and unarchive a synthetic chat from both
devices, repeat while the phone is offline, then reconnect and restart the app.
Confirm the final state matches both devices and deleted chats stay deleted.
