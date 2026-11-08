//! Ownership and freshness of asynchronous UI results.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Query {
    Status,
    Projects,
    Certificate,
    Environment(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Ticket {
    id: u64,
    generation: u64,
}

struct PendingRequest {
    ticket: Ticket,
    query: Option<Query>,
}

#[derive(Default)]
pub(super) struct Requests {
    next_id: u64,
    generation: u64,
    pending: BTreeMap<u64, PendingRequest>,
    latest: BTreeMap<Query, u64>,
}

impl Requests {
    pub(super) fn begin(&mut self, query: Option<Query>) -> Ticket {
        if query.is_none() {
            self.generation += 1;
        }
        self.next_id += 1;
        let ticket = Ticket {
            id: self.next_id,
            generation: self.generation,
        };
        if let Some(query) = &query {
            self.latest.insert(query.clone(), ticket.id);
        }
        self.pending
            .insert(ticket.id, PendingRequest { ticket, query });
        ticket
    }

    /// Completes exactly one owned task. Background snapshots are accepted
    /// only if no mutation or newer query has superseded them.
    pub(super) fn complete(&mut self, ticket: Ticket) -> bool {
        if !self
            .pending
            .get(&ticket.id)
            .is_some_and(|request| request.ticket == ticket)
        {
            return false;
        }
        let request = self
            .pending
            .remove(&ticket.id)
            .expect("owned request exists");
        if let Some(query) = request.query {
            let latest = self.latest.get(&query) == Some(&ticket.id);
            if latest {
                self.latest.remove(&query);
            }
            latest && ticket.generation == self.generation
        } else {
            self.generation += 1;
            true
        }
    }

    pub(super) fn foreground_pending(&self) -> usize {
        self.pending
            .values()
            .filter(|request| request.query.is_none())
            .count()
    }

    pub(super) fn background_pending(&self) -> usize {
        self.pending.len() - self.foreground_pending()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_snapshots_cannot_undo_a_completed_stop() {
        let mut requests = Requests::default();
        let before = requests.begin(Some(Query::Status));
        let stop = requests.begin(None);
        let during = requests.begin(Some(Query::Projects));
        assert!(requests.complete(stop));
        assert!(!requests.complete(before));
        assert!(!requests.complete(during));
        let current = requests.begin(Some(Query::Status));
        assert!(requests.complete(current));
        assert_eq!(requests.foreground_pending(), 0);
        assert_eq!(requests.background_pending(), 0);
    }

    #[test]
    fn newer_queries_replace_older_results_and_completion_is_owned_once() {
        let mut requests = Requests::default();
        let old = requests.begin(Some(Query::Environment("demo".into())));
        let new = requests.begin(Some(Query::Environment("demo".into())));
        let other = requests.begin(Some(Query::Environment("other".into())));
        assert!(requests.complete(new));
        assert!(!requests.complete(new));
        assert!(!requests.complete(old));
        assert!(requests.complete(other));
        assert_eq!(requests.background_pending(), 0);
    }
}
