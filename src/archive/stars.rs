//! Messages the user starred, kept in the encrypted archive.
//!
//! Confirmed device state and durable local intents are stored separately. Pending
//! changes overlay confirmed state until acknowledgement. Mutation timestamps order
//! remote replay; history with no mutation timestamp only seeds unknown rows.
//! Whole message fields come from the archive and are never logged.

use std::collections::HashSet;

use rusqlite::params;

use super::{Archive, Result};
use crate::model::Content;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS stars (
    chat TEXT NOT NULL,
    id TEXT NOT NULL,
    starred INTEGER NOT NULL,
    starred_at INTEGER NOT NULL,
    PRIMARY KEY (chat, id)
);
CREATE TRIGGER IF NOT EXISTS delete_message_stars AFTER DELETE ON messages BEGIN
    DELETE FROM stars WHERE chat = OLD.chat AND id = OLD.id;
    DELETE FROM star_changes WHERE chat = OLD.chat AND id = OLD.id;
END;
CREATE INDEX IF NOT EXISTS stars_by_time ON stars (starred_at);
CREATE TABLE IF NOT EXISTS star_changes (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    chat TEXT NOT NULL,
    id TEXT NOT NULL,
    starred INTEGER NOT NULL,
    requested_at INTEGER NOT NULL,
    UNIQUE(chat, id)
);
";

/// A durable local intent. Sequence is never reused, including after deletion.
#[derive(Clone, Debug)]
pub struct StarChange {
    pub sequence: u64,
    pub chat: String,
    pub id: String,
    pub starred: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::tests::message;

    fn fixture() -> Archive {
        let archive = Archive::in_memory().unwrap();
        archive.ensure_chat("fixture", "Fixture").unwrap();
        archive
            .insert_message(&message("fixture", "one", 100, false), None)
            .unwrap();
        archive
    }

    #[test]
    fn stars_keep_newest_local_intent_across_failures_and_stale_acknowledgements() {
        let archive = fixture();
        archive.queue_star("fixture", "one", true, 1000).unwrap();
        let first = archive.next_star_change().unwrap().unwrap();
        // A worker restart reconstructs from the durable archive, not UI state.
        assert_eq!(
            archive.next_star_change().unwrap().unwrap().sequence,
            first.sequence
        );
        archive.queue_star("fixture", "one", false, 1001).unwrap();
        let second = archive.next_star_change().unwrap().unwrap();
        assert!(second.sequence > first.sequence);
        archive.finish_star_change(&first, 1002).unwrap();
        assert!(
            archive.starred(10).unwrap().is_empty(),
            "older ack cannot consume newer unstar"
        );
        assert_eq!(
            archive.next_star_change().unwrap().unwrap().sequence,
            second.sequence
        );
        archive.set_star("fixture", "one", true, 2000).unwrap();
        assert!(
            archive.starred(10).unwrap().is_empty(),
            "pending local intent overlays remote state"
        );
        archive.finish_star_change(&second, 2001).unwrap();
        assert!(archive.next_star_change().unwrap().is_none());
        archive.history_star("fixture", "one", true).unwrap();
        assert!(
            archive.starred(10).unwrap().is_empty(),
            "history cannot revive unstarred state"
        );
        assert!(
            !archive.set_star("fixture", "one", true, 2001).unwrap(),
            "a conflicting exact-time replay fails closed"
        );
    }

    #[test]
    fn stars_delete_and_revoke_win_over_pending_callbacks() {
        for revoke in [false, true] {
            let archive = fixture();
            archive.queue_star("fixture", "one", true, 1000).unwrap();
            let change = archive.next_star_change().unwrap().unwrap();
            if revoke {
                archive
                    .set_content("fixture", "one", &Content::Revoked, false)
                    .unwrap();
            } else {
                archive.delete_message("fixture", "one").unwrap();
            }
            assert!(
                archive.next_star_change().unwrap().is_none(),
                "cleanup cannot wait for an acknowledgement"
            );
            archive.finish_star_change(&change, 2000).unwrap();
            assert!(archive.starred(10).unwrap().is_empty());
            assert!(archive.next_star_change().unwrap().is_none());
            archive
                .insert_message(&message("fixture", "two", 200, false), None)
                .unwrap();
            archive.queue_star("fixture", "two", true, 3000).unwrap();
            assert!(
                archive.next_star_change().unwrap().unwrap().sequence > change.sequence,
                "deleted callback tokens are never reused"
            );
        }
    }

    #[test]
    fn starred_pagination_crosses_two_hundred_without_private_or_missing_rows() {
        let archive = fixture();
        archive.ensure_chat("locked", "Locked").unwrap();
        archive.set_locked("locked", true).unwrap();
        for i in 0..450 {
            let id = format!("row{i:03}");
            archive
                .insert_message(&message("fixture", &id, 100, false), None)
                .unwrap();
            archive.set_star("fixture", &id, true, i).unwrap();
        }
        archive
            .insert_message(&message("locked", "secret", 100, false), None)
            .unwrap();
        archive.set_star("locked", "secret", true, 10000).unwrap();
        archive.set_star("fixture", "absent", true, 20000).unwrap();
        let first = archive.starred_page(200, 0).unwrap();
        let second = archive.starred_page(200, 200).unwrap();
        let last = archive.starred_page(200, 400).unwrap();
        assert_eq!((first.len(), second.len(), last.len()), (200, 200, 50));
        assert_eq!(first[0].message.id, "row449");
        assert_eq!(last[49].message.id, "row000");
        let all: HashSet<_> = first
            .into_iter()
            .chain(second)
            .chain(last)
            .map(|row| row.message.id)
            .collect();
        assert_eq!(all.len(), 450);
        assert!(
            Archive::in_memory()
                .unwrap()
                .starred(200)
                .unwrap()
                .is_empty(),
            "account archives isolate lists"
        );
    }
}

/// One starred message, as the list shows it.
#[derive(Clone, Debug)]
pub struct Starred {
    /// The message itself, whole. Personal data: never logged.
    pub message: crate::model::Message,
    /// Unix milliseconds of the star mutation, or zero when history supplies no time.
    pub starred_at: i64,
}

impl Archive {
    pub fn queue_star(&self, chat: &str, id: &str, starred: bool, at: i64) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM star_changes WHERE chat = ?1 AND id = ?2",
            params![chat, id],
        )?;
        tx.execute(
            "INSERT INTO star_changes(chat,id,starred,requested_at) VALUES (?1,?2,?3,?4)",
            params![chat, id, starred, at],
        )?;
        tx.commit()
    }

    pub fn next_star_change(&self) -> Result<Option<StarChange>> {
        use rusqlite::OptionalExtension;
        self.connection
            .query_row(
                "SELECT sequence,chat,id,starred FROM star_changes ORDER BY sequence LIMIT 1",
                [],
                |row| {
                    Ok(StarChange {
                        sequence: row.get::<_, i64>(0)? as u64,
                        chat: row.get(1)?,
                        id: row.get(2)?,
                        starred: row.get(3)?,
                    })
                },
            )
            .optional()
    }

    pub fn finish_star_change(&self, change: &StarChange, at: i64) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        // Deletion wins over a callback and must not resurrect the mark.
        let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM messages WHERE chat=?1 AND id=?2 AND json_extract(content,'$.kind') != 'revoked')", params![change.chat,change.id], |row| row.get(0))?;
        if exists {
            self.set_star(&change.chat, &change.id, change.starred, at)?;
        }
        tx.execute(
            "DELETE FROM star_changes WHERE sequence = ?1",
            params![change.sequence as i64],
        )?;
        tx.commit()
    }

    /// History has no star mutation timestamp. It can seed, never replace, state.
    pub fn history_star(&self, chat: &str, id: &str, starred: bool) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO stars(chat,id,starred,starred_at) VALUES(?1,?2,?3,0)",
            params![chat, id, starred],
        )?;
        Ok(())
    }
    /// Records a star or an unstar at `at` (Unix milliseconds).
    ///
    /// An older event loses to the time already stored, so a replay or a
    /// response that finished late cannot put the row back.
    pub fn set_star(&self, chat: &str, id: &str, starred: bool, at: i64) -> Result<bool> {
        let changed = self.connection.execute(
            "INSERT INTO stars (chat, id, starred, starred_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(chat, id) DO UPDATE SET
                starred = excluded.starred,
                starred_at = excluded.starred_at
             WHERE excluded.starred_at > stars.starred_at
                OR (excluded.starred_at = stars.starred_at AND excluded.starred = 0)",
            params![chat, id, starred, at],
        )?;
        Ok(changed > 0)
    }

    /// Marks a message as starred, remembering when.
    pub fn star(&self, chat: &str, id: &str, at: i64) -> Result<()> {
        self.set_star(chat, id, true, at)?;
        Ok(())
    }

    pub fn unstar(&self, chat: &str, id: &str, at: i64) -> Result<()> {
        self.set_star(chat, id, false, at)?;
        Ok(())
    }

    /// Drops the star row of one message. A deleted message must not leave a
    /// row that a reused id could inherit.
    pub fn delete_star(&self, chat: &str, id: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM star_changes WHERE chat=?1 AND id=?2",
            params![chat, id],
        )?;
        self.connection.execute(
            "DELETE FROM stars WHERE chat = ?1 AND id = ?2",
            params![chat, id],
        )?;
        Ok(())
    }

    /// The starred messages of one chat, for the mark in the conversation.
    pub fn starred_ids(&self, chat: &str) -> Result<HashSet<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT m.id FROM messages m LEFT JOIN stars s ON s.chat=m.chat AND s.id=m.id LEFT JOIN star_changes p ON p.chat=m.chat AND p.id=m.id WHERE m.chat=?1 AND COALESCE(p.starred,s.starred,0)=1 AND json_extract(m.content,'$.kind')!='revoked'")?;
        let rows = statement.query_map(params![chat], |row| row.get::<_, String>(0))?;
        let mut ids = HashSet::new();
        for row in rows {
            ids.insert(row?);
        }
        Ok(ids)
    }

    /// Every starred message, newest star first, for the list. The message is
    /// read from its own row, so the row has its current words and its menu
    /// every field an in-chat menu has; a message deleted here leaves the list
    /// on its own. The join on `chats` keeps a locked chat out until the folder
    /// is open.
    pub fn starred(&self, limit: usize) -> Result<Vec<Starred>> {
        self.starred_page(limit, 0)
    }

    pub fn starred_page(&self, limit: usize, offset: usize) -> Result<Vec<Starred>> {
        let rows: Vec<(i64, String, String)> = {
            let mut statement = self.connection.prepare(
                "SELECT COALESCE(p.requested_at,s.starred_at,0),m.chat,m.id
                 FROM messages m JOIN chats c ON c.id=m.chat AND c.locked=0
                 LEFT JOIN stars s ON s.chat=m.chat AND s.id=m.id
                 LEFT JOIN star_changes p ON p.chat=m.chat AND p.id=m.id
                 WHERE COALESCE(p.starred,s.starred,0)=1 AND json_extract(m.content,'$.kind')!='revoked'
                 ORDER BY 1 DESC, COALESCE(p.sequence,0) DESC, m.chat, m.id LIMIT ?1 OFFSET ?2",
            )?;
            let rows = statement.query_map(params![limit as i64, offset as i64], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
            rows.collect::<Result<_>>()?
        };
        let mut list = Vec::new();
        for (starred_at, chat, id) in rows {
            if let Some(message) = self.message(&chat, &id)? {
                // A message revoked for everyone keeps its row, with the
                // revoked content in it. It has no words and no Unstar left,
                // so it is not a row the list can offer.
                if matches!(message.content, Content::Revoked) {
                    continue;
                }
                list.push(Starred {
                    message,
                    starred_at,
                });
            }
        }
        Ok(list)
    }
}
