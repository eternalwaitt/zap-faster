//! Durable archive/unarchive intentions in this account's encrypted archive.
use super::{Archive, Result};
use rusqlite::{OptionalExtension, params};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS archive_changes (
 chat TEXT PRIMARY KEY REFERENCES chats(id) ON DELETE CASCADE,
 archived INTEGER NOT NULL, requested_at INTEGER NOT NULL
);
CREATE TRIGGER IF NOT EXISTS delete_chat_archive_changes AFTER DELETE ON chats BEGIN
 DELETE FROM archive_changes WHERE chat=OLD.id;
END;
";

impl Archive {
    pub fn request_archived(&self, chat: &str, archived: bool, now: i64) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        let previous: Option<i64> = tx
            .query_row(
                "SELECT archive_updated_at FROM chats WHERE id=?1",
                [chat],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        // Requests for aliases of a chat may precede their privacy mapping.
        // A unique queue ordering lets the later local intention win that merge.
        let newest: Option<i64> =
            tx.query_row("SELECT MAX(requested_at) FROM archive_changes", [], |row| {
                row.get(0)
            })?;
        let at = now
            .max(previous.unwrap_or(0).saturating_add(1))
            .max(newest.unwrap_or(0).saturating_add(1));
        if tx.execute(
            "UPDATE chats SET archived=?2, archive_updated_at=?3 WHERE id=?1",
            params![chat, archived, at],
        )? != 1
        {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        tx.execute("INSERT INTO archive_changes VALUES (?1,?2,?3) ON CONFLICT(chat) DO UPDATE SET archived=excluded.archived,requested_at=excluded.requested_at",params![chat, archived, at])?;
        tx.commit()
    }

    pub fn next_archive_change(&self) -> Result<Option<(String, bool, i64)>> {
        self.connection.query_row("SELECT chat,archived,requested_at FROM archive_changes ORDER BY requested_at,chat LIMIT 1", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()
    }

    pub fn finish_archive_change(&self, chat: &str, at: i64) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        let desired: Option<bool> = tx
            .query_row(
                "SELECT archived FROM archive_changes WHERE chat=?1 AND requested_at=?2",
                params![chat, at],
                |row| row.get(0),
            )
            .optional()?;
        let Some(desired) = desired else {
            return tx.commit();
        };
        // A recovered snapshot may be newer than the queued request. Acceptance
        // applies our durable intention after that snapshot, so retain a fresh
        // watermark before releasing the overlay.
        tx.execute(
            "UPDATE chats SET archived=?2, archive_updated_at=MAX(?3,
                COALESCE(archive_updated_at, 0)+1, ?4) WHERE id=?1",
            params![chat, desired, jiff::Timestamp::now().as_millisecond(), at],
        )?;
        tx.execute(
            "DELETE FROM archive_changes WHERE chat=?1 AND requested_at=?2",
            params![chat, at],
        )?;
        tx.commit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn privacy_mapping_keeps_a_pending_intention_over_the_canonical_remote_state() {
        let a = Archive::in_memory().unwrap();
        a.ensure_chat("9@lid", "Synthetic").unwrap();
        a.ensure_chat("1@s.whatsapp.net", "Synthetic").unwrap();
        a.set_archived_at("1@s.whatsapp.net", false, 200).unwrap();
        a.request_archived("9@lid", true, 100).unwrap();
        a.put_lid("9", "1").unwrap();
        assert!(a.chat("1@s.whatsapp.net").unwrap().unwrap().archived);
        assert_eq!(
            a.next_archive_change().unwrap(),
            Some(("1@s.whatsapp.net".into(), true, 100))
        );
        a.finish_archive_change("1@s.whatsapp.net", 100).unwrap();
        assert!(a.chat("1@s.whatsapp.net").unwrap().unwrap().archived);
    }
    #[test]
    fn remote_snapshots_overlay_pending_intent_and_cannot_undo_accepted_completion() {
        let a = Archive::in_memory().unwrap();
        let chat = "synthetic@s.whatsapp.net";
        a.ensure_chat(chat, "Synthetic").unwrap();
        a.request_archived(chat, true, 100).unwrap();
        a.set_archived_at(chat, false, 200).unwrap();
        assert!(a.chat(chat).unwrap().unwrap().archived);
        assert_eq!(
            a.next_archive_change().unwrap(),
            Some((chat.into(), true, 100))
        );
        a.finish_archive_change(chat, 100).unwrap();
        a.set_archived_at(chat, false, 200).unwrap();
        assert!(a.chat(chat).unwrap().unwrap().archived);
        assert!(a.next_archive_change().unwrap().is_none());
        let watermark: i64 = a
            .connection
            .query_row(
                "SELECT archive_updated_at FROM chats WHERE id=?1",
                [chat],
                |row| row.get(0),
            )
            .unwrap();
        a.set_archived_at(chat, false, watermark + 1).unwrap();
        assert!(
            !a.chat(chat).unwrap().unwrap().archived,
            "a genuinely newer remote action applies after completion"
        );
    }

    #[test]
    fn encrypted_restart_keeps_pending_intent_over_remote_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic-archive.db");
        let key = [42; 32];
        let chat = "synthetic@s.whatsapp.net";
        {
            let a = Archive::open_with_key(&path, &key).unwrap();
            a.ensure_chat(chat, "Synthetic").unwrap();
            a.request_archived(chat, false, 100).unwrap();
            a.set_archived_at(chat, true, 200).unwrap();
        }
        let a = Archive::open_with_key(&path, &key).unwrap();
        assert!(!a.chat(chat).unwrap().unwrap().archived);
        assert_eq!(
            a.next_archive_change().unwrap(),
            Some((chat.into(), false, 100))
        );
        a.set_archived_at(chat, true, 300).unwrap();
        assert!(!a.chat(chat).unwrap().unwrap().archived);
        a.finish_archive_change(chat, 100).unwrap();
        assert!(a.next_archive_change().unwrap().is_none());
        a.set_archived_at(chat, true, 300).unwrap();
        assert!(!a.chat(chat).unwrap().unwrap().archived);
    }
    #[test]
    fn newer_intent_survives_an_old_completion_and_history_replay() {
        let a = Archive::in_memory().unwrap();
        a.ensure_chat("synthetic@s.whatsapp.net", "Synthetic")
            .unwrap();
        a.request_archived("synthetic@s.whatsapp.net", true, 100)
            .unwrap();
        a.request_archived("synthetic@s.whatsapp.net", false, 100)
            .unwrap();
        a.finish_archive_change("synthetic@s.whatsapp.net", 100)
            .unwrap();
        let pending = a.next_archive_change().unwrap().unwrap();
        assert!(!pending.1);
        assert_eq!(pending.2, 101);
        a.set_archived_at(&pending.0, true, 99).unwrap();
        assert!(!a.chat(&pending.0).unwrap().unwrap().archived);
        a.finish_archive_change(&pending.0, 101).unwrap();
        assert!(a.next_archive_change().unwrap().is_none());
    }
    #[test]
    fn failed_storage_does_not_claim_the_intent_and_delete_cleans_the_queue() {
        let a = Archive::in_memory().unwrap();
        assert!(a.request_archived("missing", true, 100).is_err());
        assert!(a.next_archive_change().unwrap().is_none());
        a.ensure_chat("synthetic@s.whatsapp.net", "Synthetic")
            .unwrap();
        a.request_archived("synthetic@s.whatsapp.net", true, 100)
            .unwrap();
        a.connection.execute("DELETE FROM chats", []).unwrap();
        assert!(a.next_archive_change().unwrap().is_none());
    }
}
