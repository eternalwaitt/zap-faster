//! Durable ownership of local sends, inside the account's encrypted archive.
use super::{Archive, Result, params};

pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS outgoing_queue (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    position INTEGER NOT NULL DEFAULT 0,
    chat TEXT NOT NULL,
    id TEXT NOT NULL,
    owner TEXT NOT NULL,
    raw BLOB NOT NULL,
    expiration INTEGER,
    phase INTEGER NOT NULL DEFAULT 0,
    retry_at INTEGER NOT NULL DEFAULT 0,
    UNIQUE(chat, id)
);";

pub(crate) struct StoredSend {
    pub chat: String,
    pub id: String,
    pub raw: Vec<u8>,
    pub expiration: Option<u32>,
    pub retry_at: i64,
    pub position: i64,
}

impl Archive {
    /// Records an unsent job before it can enter the transport queue.
    pub(crate) fn retain_send(
        &self,
        chat: &str,
        id: &str,
        owner: &str,
        raw: &[u8],
        expiration: Option<u32>,
        position: i64,
    ) -> Result<()> {
        if self.connection.query_row(
            "SELECT COUNT(*) FROM outgoing_queue WHERE phase=0",
            [],
            |row| row.get::<_, i64>(0),
        )? >= 1000
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let changed = self.connection.execute(
            "INSERT INTO outgoing_queue(chat,id,owner,raw,expiration,position)
             SELECT ?1,?2,?3,?4,?5,?6 WHERE EXISTS (
                 SELECT 1 FROM messages WHERE chat=?1 AND id=?2 AND from_me=1 AND status IN (-1,1))",
            params![chat,id,owner,raw,expiration,position],
        )?;
        if changed != 1 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }

    pub(crate) fn next_send_position(&self) -> Result<i64> {
        self.connection.query_row(
            "SELECT COALESCE(MAX(position),0) FROM outgoing_queue",
            [],
            |row| row.get(0),
        )
    }
    /// Claim persists before spawning. A crash after this point is uncertain.
    pub(crate) fn claim_send(&self, chat: &str, id: &str) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE outgoing_queue SET phase=1 WHERE chat=?1 AND id=?2 AND phase=0
             AND EXISTS(SELECT 1 FROM messages WHERE chat=?1 AND id=?2 AND from_me=1 AND status IN(-1,1))",
            params![chat, id],
        )?;
        if changed != 1 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }

    /// Only a typed pre-transmission refusal returns ownership to waiting.
    pub(crate) fn defer_send(&self, chat: &str, id: &str, retry_at: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE outgoing_queue SET phase=0,retry_at=?3 WHERE chat=?1 AND id=?2",
            params![chat, id, retry_at],
        )?;
        Ok(())
    }

    /// A terminal result or cancellation revokes retry eligibility first.
    pub(crate) fn end_send(&self, chat: &str, id: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE outgoing_queue SET phase=2 WHERE chat=?1 AND id=?2",
            params![chat, id],
        )?;
        Ok(())
    }

    pub(crate) fn forget_send(&self, chat: &str, id: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM outgoing_queue WHERE chat=?1 AND id=?2",
            params![chat, id],
        )?;
        Ok(())
    }

    /// Never recover another identity's jobs, confirmed rows, or missing rows.
    pub(crate) fn waiting_sends(&self, owner: &str) -> Result<Vec<StoredSend>> {
        self.connection
            .prepare(
                "SELECT q.chat,q.id,q.raw,q.expiration,q.retry_at,q.position FROM outgoing_queue q
             JOIN messages m ON m.chat=q.chat AND m.id=q.id
             WHERE q.owner=?1 AND q.phase=0 AND m.from_me=1 AND m.status IN (-1,1)
             ORDER BY q.position,q.sequence LIMIT 1000",
            )?
            .query_map([owner], |row| {
                Ok(StoredSend {
                    chat: row.get(0)?,
                    id: row.get(1)?,
                    raw: row.get(2)?,
                    expiration: row.get(3)?,
                    retry_at: row.get(4)?,
                    position: row.get(5)?,
                })
            })?
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Delivery;

    #[test]
    fn restart_recovers_only_unclaimed_jobs_for_the_same_identity_in_order() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixture.db");
        let key = [7; 32];
        {
            let archive = Archive::open_with_key(&path, &key).unwrap();
            for id in ["first", "second", "claimed", "cancelled", "other-account"] {
                let row = crate::archive::tests::message("fixture", id, 100, true);
                archive.insert_message(&row, None).unwrap();
                archive
                    .retain_send(
                        "fixture",
                        id,
                        if id == "other-account" { "other" } else { "me" },
                        &[1, 2, 3],
                        None,
                        0,
                    )
                    .unwrap();
                archive
                    .set_outgoing_state("fixture", id, Delivery::Queued)
                    .unwrap();
            }
            archive.claim_send("fixture", "claimed").unwrap();
            archive.end_send("fixture", "cancelled").unwrap();
            archive.defer_send("fixture", "first", 1234).unwrap();
        }
        let archive = Archive::open_with_key(&path, &key).unwrap();
        let jobs = archive.waiting_sends("me").unwrap();
        assert_eq!(
            jobs.iter().map(|job| job.id.as_str()).collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(jobs[0].retry_at, 1234);
        assert_eq!(
            archive
                .message("fixture", "claimed")
                .unwrap()
                .unwrap()
                .status,
            Delivery::Unconfirmed
        );
        assert_eq!(
            archive
                .message("fixture", "cancelled")
                .unwrap()
                .unwrap()
                .status,
            Delivery::Failed
        );
        archive.clear().unwrap();
        assert!(archive.waiting_sends("me").unwrap().is_empty());
    }

    #[test]
    fn late_receipt_or_removed_row_never_reenters_the_queue() {
        let archive = Archive::in_memory().unwrap();
        for id in ["receipt", "removed"] {
            let row = crate::archive::tests::message("fixture", id, 100, true);
            archive.insert_message(&row, None).unwrap();
            archive
                .retain_send("fixture", id, "me", &[1], None, 0)
                .unwrap();
        }
        archive
            .set_status("fixture", "receipt", Delivery::Delivered, 200)
            .unwrap();
        archive.delete_message("fixture", "removed").unwrap();
        assert!(archive.waiting_sends("me").unwrap().is_empty());
    }
}
