//! Bounded archive-backed gallery queries, adapted from crmne/zapfast#264.
use super::{Archive, Result, SEARCH_COLUMNS, params, searched_message};
use crate::model::{ChatMedia, Content, MEDIA_LIST_LIMIT, Message};
use rusqlite::OptionalExtension;
const LINK_SCAN_LIMIT: i64 = 4096;
impl Archive {
    pub fn gallery(&self, chat: &str) -> Result<ChatMedia> {
        let (rows, partial_links) = self.media_docs_links(chat, MEDIA_LIST_LIMIT + 1)?;
        let mut listing = ChatMedia::collect(chat.into(), rows, MEDIA_LIST_LIMIT);
        listing.links_truncated |= partial_links;
        Ok(listing)
    }
    /// Bounded viewer page around the selected archive row, adapted from #189.
    /// The anchor can be outside the transcript and newest gallery page.
    pub fn viewer_listing(&self, chat: &str, id: &str) -> Result<ChatMedia> {
        let anchor = self
            .connection
            .query_row(
                "SELECT timestamp,rowid FROM messages WHERE chat=?1 AND id=?2",
                params![chat, id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let Some((timestamp, rowid)) = anchor else {
            return self.gallery(chat);
        };
        let mut media = Vec::new();
        for (comparison, order) in [("<=", "DESC"), (">", "ASC")] {
            let sql = format!("SELECT {SEARCH_COLUMNS} FROM messages WHERE chat=?1 AND json_valid(content)
                AND json_extract(content,'$.kind') IN ('image','video') AND (timestamp,rowid) {comparison} (?2,?3)
                ORDER BY timestamp {order},rowid {order} LIMIT ?4");
            media.extend(
                self.connection
                    .prepare(&sql)?
                    .query_map(
                        params![chat, timestamp, rowid, MEDIA_LIST_LIMIT as i64 / 2],
                        searched_message,
                    )?
                    .collect::<Result<Vec<_>>>()?,
            );
        }
        media.sort_by_key(|row| std::cmp::Reverse(row.timestamp));
        Ok(ChatMedia {
            chat: chat.into(),
            media_truncated: media.len() == MEDIA_LIST_LIMIT,
            media,
            ..Default::default()
        })
    }
    /// The rows the info panel files as media, documents and links: this
    /// chat's pictures and videos, its documents, and its text messages that
    /// hold a web link, each kind newest first and cut at `limit`.
    fn media_docs_links(&self, chat: &str, limit: usize) -> Result<(Vec<Message>, bool)> {
        let mut rows = self.rows_of_kind(chat, "IN ('image', 'video')", limit)?;
        rows.extend(self.rows_of_kind(chat, "= 'document'", limit)?);
        // Bound every inspected candidate, including rejected URLs. A long
        // archive must not keep the account worker parsing text indefinitely.
        let sql = format!(
            "SELECT {} FROM messages WHERE chat=?1
             ORDER BY timestamp DESC,rowid DESC LIMIT ?2",
            SEARCH_COLUMNS.replace("thumbnail", "NULL AS thumbnail")
        );
        let mut statement = self.connection.prepare(&sql)?;
        let mut scanned = 0;
        for row in statement.query_map(params![chat, LINK_SCAN_LIMIT + 1], searched_message)? {
            let mut row = row?;
            scanned += 1;
            if scanned > LINK_SCAN_LIMIT {
                break;
            }
            if matches!(row.content, Content::Text { .. }) && !row.content.web_links().is_empty() {
                row.thumbnail = None;
                rows.push(row);
            }
        }
        rows.sort_by_key(|row| std::cmp::Reverse(row.timestamp));
        Ok((rows, scanned > LINK_SCAN_LIMIT))
    }

    /// One chat's newest messages whose content kind matches the SQL
    /// comparison `kinds`, such as `= 'document'`.
    fn rows_of_kind(&self, chat: &str, kinds: &str, limit: usize) -> Result<Vec<Message>> {
        let sql = format!(
            "SELECT {SEARCH_COLUMNS}
             FROM messages
             WHERE chat = ?1 AND json_valid(content)
               AND json_extract(content, '$.kind') {kinds}
             ORDER BY timestamp DESC, rowid DESC
             LIMIT ?2"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(params![chat, limit as i64], searched_message)?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewer_page_includes_an_old_anchor_and_neighbors_outside_the_newest_page() {
        let archive = Archive::in_memory().unwrap();
        for n in 0..700 {
            let mut row = crate::archive::tests::message("one", &format!("photo-{n}"), n, false);
            row.content = Content::Image {
                caption: None,
                motion: None,
                media: crate::model::Media {
                    mime: "image/jpeg".into(),
                    size: 10,
                    width: None,
                    height: None,
                    album: None,
                    path: None,
                    state: Default::default(),
                },
            };
            archive.insert_message(&row, None).unwrap();
        }
        assert!(
            !archive
                .gallery("one")
                .unwrap()
                .media
                .iter()
                .any(|row| row.id == "photo-50")
        );
        let listing = archive.viewer_listing("one", "photo-50").unwrap();
        assert!(listing.media.len() <= MEDIA_LIST_LIMIT);
        for id in ["photo-49", "photo-50", "photo-51"] {
            assert!(listing.media.iter().any(|row| row.id == id));
        }
    }

    #[test]
    fn archive_gallery_is_scoped_filtered_and_marks_bounded_link_scans() {
        let archive = Archive::in_memory().unwrap();
        for (chat, id, text) in [
            ("one", "link", "https://example.com"),
            ("two", "other", "https://other.example"),
            ("one", "invalid", "http://x"),
        ] {
            let mut row = crate::archive::tests::message(chat, id, 1, false);
            row.content = Content::text(text);
            archive.insert_message(&row, None).unwrap();
        }
        let listing = archive.gallery("one").unwrap();
        assert_eq!(listing.links.len(), 1);
        assert_eq!(listing.links[0].message, "link");
        assert!(!listing.links_truncated);
        for n in 0..LINK_SCAN_LIMIT {
            let mut row = crate::archive::tests::message("one", &format!("invalid-{n}"), 2, false);
            row.content = Content::text("http://x");
            archive.insert_message(&row, None).unwrap();
        }
        let listing = archive.gallery("one").unwrap();
        assert!(listing.links.is_empty());
        assert!(listing.links_truncated);
    }
}
