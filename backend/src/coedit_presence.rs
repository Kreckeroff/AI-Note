use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use uuid::Uuid;

use crate::db::{PageAddress, coedit_presence::ActiveSession};

// Observers refresh every four seconds. Leave room in the visible 15-second limit.
const LIFETIME: Duration = Duration::from_secs(10);
const MAX_TABS_PER_SESSION: usize = 64;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct PageKey {
    tenant: Uuid,
    space: Uuid,
    page: Uuid,
}

impl From<&PageAddress> for PageKey {
    fn from(address: &PageAddress) -> Self {
        Self {
            tenant: address.tenant,
            space: address.space,
            page: address.page,
        }
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct TabKey {
    account: Uuid,
    session: [u8; 32],
    tab: Uuid,
}

struct Tab {
    seen: Instant,
    editing: Option<bool>,
}

#[derive(Default)]
struct Pages {
    tabs: HashMap<PageKey, HashMap<TabKey, Tab>>,
    pruned: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct PresenceTracker(Mutex<Pages>);

impl PresenceTracker {
    pub(crate) fn active(&self, address: &PageAddress, now: Instant) -> Vec<ActiveSession> {
        let mut pages = self
            .0
            .lock()
            .expect("presence lock is not held across application code");
        if pages
            .pruned
            .is_none_or(|at| now.saturating_duration_since(at) >= LIFETIME)
        {
            pages.tabs.retain(|_, tabs| {
                tabs.retain(|_, tab| now.saturating_duration_since(tab.seen) < LIFETIME);
                !tabs.is_empty()
            });
            pages.pruned = Some(now);
        }
        let Some(tabs) = pages.tabs.get(&PageKey::from(address)) else {
            return Vec::new();
        };
        let mut sessions = HashMap::new();
        for (key, tab) in tabs {
            if key.account != address.account
                && now.saturating_duration_since(tab.seen) < LIFETIME
                && let Some(editing) = tab.editing
            {
                *sessions.entry((key.account, key.session)).or_insert(false) |= editing;
            }
        }
        sessions
            .into_iter()
            .map(|((account, digest), edit)| ActiveSession {
                account,
                digest,
                edit,
            })
            .collect()
    }

    pub(crate) fn record(
        &self,
        address: &PageAddress,
        session: [u8; 32],
        tab: Uuid,
        editing: Option<bool>,
        now: Instant,
    ) {
        let mut pages = self
            .0
            .lock()
            .expect("presence lock is not held across application code");
        let tabs = pages.tabs.entry(address.into()).or_default();
        let key = TabKey {
            account: address.account,
            session,
            tab,
        };
        if tabs.get(&key).is_some_and(|entry| entry.seen > now) {
            return;
        }
        if !tabs.contains_key(&key) {
            let mut own = tabs
                .iter()
                .filter(|(key, _)| key.account == address.account && key.session == session);
            let Some(mut earliest) = own.next() else {
                tabs.insert(key, Tab { seen: now, editing });
                return;
            };
            let mut count = 1;
            for next in own {
                count += 1;
                if next.1.seen < earliest.1.seen {
                    earliest = next;
                }
            }
            if count >= MAX_TABS_PER_SESSION {
                let expired = *earliest.0;
                tabs.remove(&expired);
            }
        }
        tabs.insert(key, Tab { seen: now, editing });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_deduplicate_and_expire_without_crossing_page_scope() {
        let tracker = PresenceTracker::default();
        let mut owner = PageAddress {
            tenant: Uuid::new_v4(),
            space: Uuid::new_v4(),
            page: Uuid::new_v4(),
            account: Uuid::new_v4(),
        };
        let actor = owner;
        owner.account = Uuid::new_v4();
        let now = Instant::now();
        let read = Uuid::new_v4();
        let edit = Uuid::new_v4();
        tracker.record(&actor, [1; 32], read, Some(false), now);
        tracker.record(&actor, [1; 32], edit, Some(true), now);
        tracker.record(&actor, [3; 32], read, Some(false), now);
        let active = tracker.active(&owner, now);
        assert_eq!(active.len(), 2);
        assert_eq!(active.iter().filter(|peer| peer.edit).count(), 1);
        tracker.record(&actor, [1; 32], edit, None, now + Duration::from_secs(1));
        tracker.record(&actor, [1; 32], edit, Some(true), now); // an earlier request completed late
        assert!(
            tracker
                .active(&owner, now + Duration::from_secs(1))
                .iter()
                .all(|peer| !peer.edit)
        );
        assert!(tracker.active(&owner, now + LIFETIME).is_empty());
        owner.page = Uuid::new_v4();
        assert!(tracker.active(&owner, now).is_empty());
    }
}
