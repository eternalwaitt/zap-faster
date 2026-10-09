//! Bounded account-local history recovery and ownership of unsolicited pages.
//!
//! Match the phone's optional PDO session id to the library's returned id.
//! Keep a conservative chat owner for responses that omit that id.

use crate::model::ChatId;
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

const PACE: Duration = Duration::from_secs(20);
const MAX_CHATS: usize = 10;
const MAX_PAGES: u8 = 5;
const MAX_REQUESTS: u8 = 50;

#[derive(Default)]
pub(super) struct Recovery {
    pub enabled: bool,
    pub paused: bool,
    pub focused: Option<ChatId>,
    due: Option<Instant>,
    pages: HashMap<ChatId, u8>,
    requests: u8,
    cursor: usize,
    /// Automatic requests, including timed-out ones still owned by the phone.
    silent: HashSet<ChatId>,
    stopped: HashSet<ChatId>,
    polls: HashSet<ChatId>,
    started: HashMap<ChatId, Instant>,
    sessions: HashMap<String, (ChatId, Instant)>,
    early: HashMap<String, Vec<(ChatId, usize, Option<bool>)>>,
}

impl Recovery {
    pub fn canonicalize(&mut self, old: &str, new: &str) {
        if let Some(pages) = self.pages.remove(old) {
            let total = self.pages.entry(new.to_owned()).or_default();
            *total = total.saturating_add(pages);
        }
        if self.silent.remove(old) {
            self.silent.insert(new.to_owned());
        }
        if self.polls.remove(old) {
            self.polls.insert(new.to_owned());
        }
        if self.stopped.remove(old) {
            self.stopped.insert(new.to_owned());
        }
        if let Some(started) = self.started.remove(old) {
            let target = self.started.entry(new.to_owned()).or_insert(started);
            *target = (*target).max(started);
        }
        for (chat, _) in self.sessions.values_mut() {
            if chat == old {
                *chat = new.to_owned();
            }
        }
        for filed in self.early.values_mut() {
            for (chat, _, _) in filed {
                if chat == old {
                    *chat = new.to_owned();
                }
            }
        }
        if self.focused.as_deref() == Some(old) {
            self.focused = Some(new.to_owned());
        }
    }
    pub fn started(&mut self, chat: &str, requested: Instant) {
        self.started.insert(chat.to_owned(), requested);
    }
    pub fn session(&self, id: &str) -> Option<&(ChatId, Instant)> {
        self.sessions.get(id)
    }
    pub fn sent(
        &mut self,
        chat: ChatId,
        requested: Instant,
        id: String,
    ) -> Option<Vec<(ChatId, usize, Option<bool>)>> {
        // Dropping old identities is safe: unrecognized pages never own UI.
        if self.sessions.len() >= 128
            && let Some(oldest) = self
                .sessions
                .iter()
                .min_by_key(|(_, (_, at))| *at)
                .map(|(id, _)| id.clone())
        {
            self.sessions.remove(&oldest);
        }
        let early = self.early.remove(&id);
        self.sessions.insert(id, (chat, requested));
        early
    }
    pub fn early(&mut self, id: String, filed: Vec<(ChatId, usize, Option<bool>)>) {
        if self.early.len() < 8 {
            self.early.insert(id, filed);
        }
    }
    pub fn owns_attempt(&self, chat: &str, requested: Instant) -> bool {
        self.started.get(chat) == Some(&requested)
    }
    pub fn attempted(&self, chat: &str) -> bool {
        self.started.contains_key(chat)
    }
    pub fn correlated(&self, chat: &str) -> bool {
        self.started.get(chat).is_some_and(|at| {
            self.sessions
                .values()
                .any(|(owner, started)| owner == chat && started == at)
        })
    }
    pub fn owns(&self, chat: &str) -> bool {
        self.silent.contains(chat)
    }
    pub fn claim(&mut self, chat: ChatId) {
        self.silent.insert(chat);
    }
    pub fn claim_poll(&mut self, chat: ChatId) {
        self.polls.insert(chat.clone());
        self.claim(chat);
    }
    pub fn poll_owned(&self, chat: &str) -> bool {
        self.polls.contains(chat)
    }
    pub fn answered(&mut self, chat: &str, progress: bool) -> bool {
        self.polls.remove(chat);
        let owned = self.silent.remove(chat);
        if owned && !progress {
            self.stopped.insert(chat.to_owned());
        }
        owned
    }
    pub fn stop(&mut self, chat: &str) {
        self.stopped.insert(chat.to_owned());
    }
    /// Reconnect releases uncorrelated leases but never renews session budgets.
    pub fn reconnect(&mut self) {
        self.silent.clear();
        self.polls.clear();
        self.started.clear();
        self.early.clear();
    }
    pub fn next(&mut self, candidates: &[ChatId], now: Instant) -> Option<ChatId> {
        if !self.enabled
            || self.paused
            || self.requests >= MAX_REQUESTS
            || self.due.is_some_and(|due| now < due)
        {
            return None;
        }
        let len = candidates.len().min(MAX_CHATS);
        for step in 0..len {
            let index = (self.cursor + step) % len;
            let chat = &candidates[index];
            if self.owns(chat)
                || self.stopped.contains(chat)
                || self.pages.get(chat).copied().unwrap_or_default() >= MAX_PAGES
            {
                continue;
            }
            self.cursor = (index + 1) % len;
            *self.pages.entry(chat.clone()).or_default() += 1;
            self.requests += 1;
            self.due = Some(now + PACE);
            self.claim(chat.clone());
            return Some(chat.clone());
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_privacy_mapping_preserves_budget_and_correlated_ownership() {
        let now = Instant::now();
        let mut recovery = Recovery {
            enabled: true,
            ..Default::default()
        };
        assert_eq!(
            recovery.next(&["9@lid".into()], now).as_deref(),
            Some("9@lid")
        );
        recovery.started("9@lid", now);
        recovery.sent("9@lid".into(), now, "session".into());
        recovery.canonicalize("9@lid", "1@s.whatsapp.net");
        assert_eq!(
            recovery.session("session"),
            Some(&("1@s.whatsapp.net".into(), now))
        );
        assert!(recovery.owns("1@s.whatsapp.net"));
        assert!(!recovery.owns("9@lid"));
        assert!(recovery.answered("1@s.whatsapp.net", true));
        for attempt in 1..5 {
            let chat = recovery
                .next(&["1@s.whatsapp.net".into()], now + PACE * attempt)
                .unwrap();
            recovery.answered(&chat, true);
        }
        assert!(
            recovery
                .next(&["1@s.whatsapp.net".into()], now + PACE * 5)
                .is_none()
        );
    }
    #[test]
    fn changing_chat_candidates_cannot_exceed_the_account_session_budget() {
        let mut recovery = Recovery {
            enabled: true,
            ..Default::default()
        };
        let now = Instant::now();
        for attempt in 0..50 {
            let candidate = vec![format!("chat-{attempt}")];
            let chat = recovery.next(&candidate, now + PACE * attempt).unwrap();
            recovery.answered(&chat, true);
        }
        recovery.reconnect();
        assert!(
            recovery
                .next(&["another".into()], now + PACE * 51)
                .is_none()
        );
    }
    #[test]
    fn early_response_registration_is_bounded_and_correlated_to_its_attempt() {
        let now = Instant::now();
        let mut recovery = Recovery::default();
        recovery.early("early".into(), vec![("chat".into(), 1, Some(true))]);
        assert_eq!(
            recovery.sent("chat".into(), now, "early".into()),
            Some(vec![("chat".into(), 1, Some(true))])
        );
        recovery.claim("chat".into());
        recovery.started("chat", now);
        assert!(recovery.correlated("chat"));
        assert!(recovery.owns_attempt("chat", now));
        assert!(!recovery.owns_attempt("chat", now + PACE));
        recovery.reconnect();
        assert!(!recovery.owns_attempt("chat", now));
        assert_eq!(recovery.session("early"), Some(&("chat".into(), now)));
        for index in 0..20 {
            recovery.early(index.to_string(), Vec::new());
        }
        assert_eq!(recovery.early.len(), 8);
    }
    #[test]
    fn opt_in_pacing_fairness_and_budget_survive_reconnect_and_cancellation() {
        let now = Instant::now();
        let chats = vec!["a".into(), "b".into()];
        let mut recovery = Recovery::default();
        assert!(recovery.next(&chats, now).is_none());
        recovery.enabled = true;
        assert_eq!(recovery.next(&chats, now).as_deref(), Some("a"));
        recovery.answered("a", true);
        assert!(
            recovery
                .next(&chats, now + Duration::from_secs(19))
                .is_none()
        );
        assert_eq!(recovery.next(&chats, now + PACE).as_deref(), Some("b"));
        recovery.enabled = false;
        assert!(
            recovery.owns("b"),
            "cancel keeps ownership of transmitted requests"
        );
        assert!(recovery.next(&chats, now + PACE * 2).is_none());
        recovery.enabled = true;
        recovery.reconnect();
        for attempt in 2..10 {
            let chat = recovery.next(&chats, now + PACE * attempt).unwrap();
            recovery.answered(&chat, true);
        }
        assert!(recovery.next(&chats, now + PACE * 10).is_none());
        recovery.reconnect();
        assert!(recovery.next(&chats, now + PACE * 11).is_none());
    }
    #[test]
    fn timeout_late_response_no_progress_lock_pause_and_account_isolation() {
        let now = Instant::now();
        let chats = vec!["a".into()];
        let mut first = Recovery {
            enabled: true,
            ..Default::default()
        };
        let mut second = Recovery {
            enabled: true,
            ..Default::default()
        };
        first.paused = true;
        assert!(first.next(&chats, now).is_none());
        first.paused = false;
        assert!(first.next(&chats, now).is_some());
        assert!(first.next(&chats, now + PACE * 10).is_none());
        assert!(second.next(&chats, now).is_some());
        assert!(first.answered("a", false));
        assert!(
            !first.answered("a", true),
            "stale duplicate cannot release another owner"
        );
        assert!(second.owns("a"));
        assert!(first.next(&chats, now + PACE * 11).is_none());
        assert!(
            !Recovery::default().enabled,
            "restart retains opt-in policy via account settings"
        );
    }
}
