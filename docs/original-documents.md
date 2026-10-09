---
title: Original documents
permalink: /original-documents/
---

The original-document flow adapts [FelipeFMedeiros's PR #469](https://github.com/crmne/zapfast/pull/469),
with account-bound refusal recovery and preservation of the fork's durable queue.

Before sending an attachment, click **Media** on its tile to select **Document**. Click again to send it as normal media. Photo files sent as documents retain their original bytes and filename, including transparency and any metadata already in the file. They appear as downloadable documents rather than photo bubbles.

Pasted clipboard images sent as documents use lossless PNG. This preserves the available pixels, including transparency. Clipboard pixels do not include an original filename or EXIF metadata; the document is named `clipboard.png`.

Mixed attachments send in the order shown in the composer. The caption and reply quote accompany the first attachment. A send refused before transmission returns to the account and chat that initiated it, preserving its document choice, original caption and quote alongside newer drafts. Uploads that already reached the durable send queue retain the existing failure, cancellation and unconfirmed-send behavior.

International phone labels use each number's own country code and offline formatting metadata. The number used to address a message does not change. Unrecognized numbers display every digit with a leading plus sign.

Manual acceptance: stage a PNG file with transparency and metadata, a pasted transparent clipboard image, and a PDF in that order. Send the first two as documents to a test account. Save the received files and compare the PNG file's bytes and filename, and the clipboard PNG's decoded RGBA pixels. Repeat a pre-transmission offline refusal while switching accounts and while composing a newer caption. Confirm the original attachments and quote return only to their originating account, and the newer draft remains available.
