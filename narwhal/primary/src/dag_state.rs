use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    ops::Bound::{Excluded, Unbounded},
    sync::Arc,
    time::Duration,
};

use config::{AuthorityIdentifier, Committee, Stake};
use fastcrypto::ed25519::Ed25519SignatureAsBytes;
use parking_lot::Mutex;
use storage::HeaderStore;
use tokio::time::Instant;
use types::{
    error::DagResult, Header, HeaderAPI, HeaderKey, HeaderSignatureBytes, HeaderV3, Round,
    SignedHeader, TimestampMs,
};

/// Keeps track of suspended certificates and their missing parents.
/// The digest keys in `suspended` and `missing` can overlap, but a digest can exist in one map
/// but not the other.
///
/// They can be combined into a single map, but it seems more complex to differentiate between
/// suspended certificates that is not a missing parent of another, from a missing parent without
/// the actual certificate.
///
/// Traversal of certificates that can be accepted should start from the missing map, i.e.
/// 1. If a certificate exists in `missing`, remove its entry.
/// 2. Find children of the certificate, update their missing parents.
/// 3. If a child certificate no longer has missing parent, traverse from it with step 1.
///
/// Synchronizer should access this struct via its methods, to avoid making inconsistent changes.
pub struct DagState {
    inner: Arc<Mutex<Inner>>,
}

impl DagState {
    pub(crate) fn new(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        header_store: HeaderStore,
    ) -> Self {
        let mut accepted_by_author = vec![];
        accepted_by_author.resize_with(committee.size(), BTreeMap::default);
        let suspended_count = vec![0; committee.size()];
        let persisted = vec![0; committee.size()];
        let committed = vec![0; committee.size()];
        let mut inner = Inner {
            authority_id,
            committee,
            accepted_by_author,
            accepted_by_round: Default::default(),
            suspended: Default::default(),
            suspended_count,
            persisted,
            committed,
            header_store: header_store.clone(),
        };
        inner.recover();
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    /// Return true when at least one header is accepted, false otherwise.
    pub(crate) fn try_accept(&self, signed_headers: Vec<SignedHeader>) -> DagResult<bool> {
        let mut any_accepted = false;
        let mut inner = self.inner.lock();
        for header in signed_headers {
            any_accepted = any_accepted || inner.try_accept(header)?;
        }
        Ok(any_accepted)
    }

    pub(crate) fn try_propose(&self) -> ProposeResult {
        let mut inner = self.inner.lock();
        inner.try_propose()
    }
}

struct Inner {
    // Identifier of this primary.
    authority_id: AuthorityIdentifier,
    // Committee of the current epoch.
    committee: Committee,
    // Caches accepted headers by author in each Vec element.
    // The invariant is that each element contains at least all uncommitted headers, or the last
    // MAX_CACHED_PER_AUTHOR headers, whichever is more.
    // So in the common case, each author has the most recent MAX_CACHED_PER_AUTHOR headers cached.
    accepted_by_author: Vec<BTreeMap<HeaderKey, SignedHeader>>,
    // An index into the `accepted` structure, to allow looking up Headers by round.
    accepted_by_round: BTreeMap<Round, HeadersByRound>,
    // Maps keys of suspended headers to the header content and remaining missing ancestors.
    suspended: BTreeMap<HeaderKey, SuspendedHeader>,
    // Number of suspended headers per author.
    suspended_count: Vec<usize>,
    // Watermark of persisted headers per author.
    persisted: Vec<Round>,
    // Watermark of committed headers per author.
    committed: Vec<Round>,
    // TODO: keep track of byzantine validators, and do not include additional headers from them.
    // Stores headers from all validators in the network.
    // Should only be used for occasional lookups.
    header_store: HeaderStore,
}

impl Inner {
    const MAX_CACHED_PER_AUTHOR: usize = 1_000;
    const MAX_SUSPENSION_PER_AUTHOR: usize = 1_000;

    fn recover(&mut self) {
        let genesis_headers = SignedHeader::genesis(&self.committee);
        for signed_header in genesis_headers {
            self.accept_internal(signed_header);
        }
    }

    /// Return true when the header is accepted, false otherwise.
    /// Failure is only possible when reading from storage, which should be rare.
    fn try_accept(&mut self, signed_header: SignedHeader) -> DagResult<bool> {
        let key = signed_header.key();

        let mut missing = vec![];
        let mut to_check = vec![];
        for ancestor in signed_header.header().ancestors() {
            // Look up all accepted headers from the ancestor author.
            let ancestor_accepted = &self.accepted_by_author[ancestor.author().0 as usize];
            if ancestor_accepted.contains_key(ancestor) {
                continue;
            }
            // Based on the invariant of `accepted``, recent headers (>= MAX_CACHED_PER_AUTHOR)
            // from the ancestor author must be in the `ancestor_accepted` map.
            if ancestor_accepted.is_empty() {
                missing.push(*ancestor);
                continue;
            }
            if ancestor_accepted.first_key_value().unwrap().0.round() <= key.round() {
                missing.push(*ancestor);
                continue;
            }
            to_check.push(*ancestor);
        }

        // In general accessing rocksdb in a critical section should be avoided.
        // But this should be very rare, especially when no node is Byzantine.
        if !to_check.is_empty() {
            let result = self
                .header_store
                .multi_contains(to_check.clone().into_iter())?;
            for (key, exists) in to_check.into_iter().zip(result.into_iter()) {
                if !exists {
                    missing.push(key);
                }
            }
        }

        if !missing.is_empty() {
            for ancestor in &missing {
                self.suspended
                    .entry(*ancestor)
                    .or_default()
                    .dependents
                    .insert(key);
            }
            let suspended_header = self.suspended.entry(key).or_default();
            if suspended_header.signed_header.is_none() {
                self.suspended_count[key.author().0 as usize] += 1;
                suspended_header.signed_header = Some(signed_header);
            }
            suspended_header.missing_ancestors.extend(missing.iter());
            return Ok(false);
        }

        self.accept_internal(signed_header);
        Ok(true)
    }

    fn accept_internal(&mut self, signed_header: SignedHeader) {
        let mut to_accept = vec![signed_header];
        while let Some(signed_header) = to_accept.pop() {
            // TODO: carry out additional validations on the header, e.g. parent link.
            let key = signed_header.key();
            let author_index = key.author().0 as usize;
            let author_accepted = &mut self.accepted_by_author[author_index];
            // TODO: enforce size limit on author_accepted.
            author_accepted.insert(key, signed_header);

            let header_by_round = self.accepted_by_round.entry(key.round()).or_default();
            header_by_round.headers.insert(key);
            if header_by_round.authors.insert(key.author()) {
                header_by_round.total_stake += self.committee.stake_by_id(key.author());
                if header_by_round.total_stake >= self.committee.quorum_threshold() {
                    header_by_round.quorum_time = Some(Instant::now());
                }
            }

            // Try to accept children of the accepted header.
            let Some(suspended_header) = self.suspended.remove(&key) else {
                continue;
            };
            assert!(suspended_header.missing_ancestors.is_empty());
            for child in suspended_header.dependents {
                let suspended_child = self
                    .suspended
                    .get_mut(&child)
                    .expect("missing_ancestors should exist!");
                suspended_child.missing_ancestors.remove(&key);
                if suspended_child.missing_ancestors.is_empty() {
                    self.suspended_count[child.author().0 as usize] -= 1;
                    to_accept.push(
                        suspended_child
                            .signed_header
                            .take()
                            .expect("signed_header should exist!"),
                    );
                }
            }
        }
    }

    fn try_propose(&mut self) -> ProposeResult {
        let last_proposed = self.last_proposed_round();
        let mut parent_round = self
            .accepted_by_round
            .last_key_value()
            .map(|(r, _)| *r)
            .unwrap();

        while parent_round >= last_proposed {
            let headers_by_round = &self.accepted_by_round[&parent_round];
            // TODO: wait for round leader(s).
            if headers_by_round.quorum_time.is_some() {
                break;
            }
            parent_round -= 1;
        }
        if parent_round < last_proposed {
            return ProposeResult {
                header_proposal: None,
                next_check_delay: Duration::from_millis(100),
            };
        }

        let header_round = parent_round + 1;
        let mut ancestors = vec![];
        let mut ancestor_max_ts_ms = 0;
        for index in 0..self.committee.size() {
            let headers = &self.accepted_by_author[index];
            // TODO: handle byzantine case, where a round can have multiple headers from the same author.
            let (key, ancestor) = headers
                .range((
                    Unbounded,
                    Excluded(HeaderKey::new(
                        header_round,
                        Default::default(),
                        Default::default(),
                    )),
                ))
                .next_back()
                .unwrap();
            ancestors.push(*key);
            ancestor_max_ts_ms = std::cmp::max(ancestor_max_ts_ms, *ancestor.header().created_at());
        }

        ProposeResult {
            header_proposal: Some((header_round, ancestors, ancestor_max_ts_ms)),
            next_check_delay: Duration::from_millis(100),
        }
    }

    fn own_index(&self) -> usize {
        self.authority_id.0 as usize
    }

    fn last_proposed_round(&self) -> Round {
        let own_headers = &self.accepted_by_author[self.own_index()];
        own_headers
            .last_key_value()
            .map(|(key, _)| key.round())
            .unwrap()
    }

    fn num_suspended(&self) -> usize {
        self.suspended.len()
    }
}

// Suspended header with missing dependency and dependent info.
#[derive(Debug, Default)]
struct SuspendedHeader {
    signed_header: Option<SignedHeader>,
    missing_ancestors: BTreeSet<HeaderKey>,
    dependents: BTreeSet<HeaderKey>,
}

/// Information to generate the next header.
pub(crate) struct ProposeResult {
    // When not None, contains the round, ancestors and ancestor timestamp of the next header.
    pub(crate) header_proposal: Option<(Round, Vec<HeaderKey>, TimestampMs)>,
    // try_propose() should be called again after the next_check_delay,
    // when it is likely to succeed.
    pub(crate) next_check_delay: Duration,
}

/// Headers in the same round and their aggregated information.
#[derive(Debug, Default)]
struct HeadersByRound {
    headers: BTreeSet<HeaderKey>,
    authors: BTreeSet<AuthorityIdentifier>,
    total_stake: Stake,
    quorum_time: Option<Instant>,
}
