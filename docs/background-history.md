---
title: Background history recovery
permalink: /background-history/
description: Opt-in recovery limits, request ownership, and phone availability.
---

# Background history recovery

The opt-in recovery flow adapts [LisandroNahuelH's PR #194](https://github.com/crmne/zapfast/pull/194),
with request ownership, bounded budgets and coordination with reader and poll requests.

In Settings, enable **Recover older history in the background** for each account
that should request older messages. It is off by default. Your phone must be
online and may provide only some history, or none. The local encrypted archive
is preserved. This setting does not enable attachment downloads.

Recovery prioritizes the remembered open chat, pinned chats, and recent activity.
It visits at most ten eligible chats in a round, rotating fairly, with at least
twenty seconds between requests. Each process session permits at most five
requests per chat and fifty total requests per account. Failures consume these
budgets. Reconnecting or toggling does not reset them. Restarting starts a new
bounded session and retains the saved opt-in.

The worker waits for link-time sync and gives reader requests and poll results
priority. Locked chats, archived chats, channels, chats we left, chats the phone
declared exhausted, and chats with a clear/delete barrier are skipped. App lock
pauses new requests. Disabling recovery stops new requests but keeps ownership
of pages already requested. Archive deletion barriers still reject removed
messages when late history arrives.

Background pages update the archive and chat summary without moving the
reader's timeline, completing their history spinner, or displaying timeout
notices. A reader request made while the same chat's background request is
still waiting can use that request, avoiding a second transmission.

The protocol library returns a PDO request id, and the phone can include it as
`peerDataRequestSessionId` in its answer. Tagged responses are matched to the
exact attempt, including after reconnects. Answers that arrive before the send
callback are buffered in a bounded queue until ownership is registered.
Late or duplicate tagged pages cannot complete a newer reader request.

When the phone omits that id, correlation is limited to the chat. A timed-out
automatic request retains its silent owner. If no request id was returned, a
reader retry for that chat stops quietly until an answer or reconnect releases
the owner. When the old request has a known id, a new reader request may be
sent, but an id-less answer is archived silently because its owner is ambiguous.
After automatic recovery has run for a chat, newer reader requests require a
matching session id until reconnect; this also keeps duplicate id-less pages
from completing a newer spinner after the original request has finished.
Other chats continue. Poll retries retain automatic ownership and normal backoff.
Internal failures carry their request timestamp, so an old failure cannot
cancel a replacement reader request. These safeguards favor keeping the reader's
viewport and request status correct over guessing which request a page answers.

## Attended acceptance

Use a synthetic test account and phone. Enable recovery and verify requests
are at least twenty seconds apart and limited to five per chat, with recent
chats alternating. Keep a timeline open: background pages must not scroll it.
Turn off the phone, let a background request time out, then bring it online:
late pages must remain silent. Ask explicitly for another chat's history and
confirm its spinner and timeout notice belong only to that request. Repeat
with a poll awaiting results and a second account. Lock the app and lock a chat
on the phone; new requests must stop for the app or excluded chat. Clear a chat
and verify old pages do not restore it. Disable recovery while a page is
waiting, reconnect, and restart; confirm opt-in persistence and session bounds.

Actual phone delivery remains an attended acceptance step.
