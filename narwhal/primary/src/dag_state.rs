use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque},
    fmt,
    ops::Bound::{Excluded, Included, Unbounded},
    sync::Arc,
    time::Duration,
};

use config::{AuthorityIdentifier, Committee, Stake};
use mysten_metrics::monitored_scope;
use parking_lot::Mutex;
use storage::{ConsensusStore, HeaderStore};
use store::rocks::DBBatch;
use tokio::time::Instant;
use tracing::{debug, error, warn};
use types::{
    error::DagResult, Certificate, CertificateV2, CommittedSubDag, ConsensusCommitAPI, HeaderAPI,
    HeaderKey, ReputationScores, Round, SignedHeader, TimestampMs,
};

use crate::{consensus::LeaderSchedule, metrics::PrimaryMetrics};

/// Number of headers to cache per authority.
const HEADERS_CACHED_PER_AUTHORITY: usize = 1000;

/// Number of recent rounds to cache for header index.
const HEADER_ROUNDS_CACHED: usize = 100;

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
    header_store: HeaderStore,
    consensus_store: ConsensusStore,
}

impl DagState {
    pub(crate) fn new(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        leader_schedule: LeaderSchedule,
        header_store: HeaderStore,
        consensus_store: ConsensusStore,
        metrics: Arc<PrimaryMetrics>,
    ) -> Self {
        let mut accepted_by_author = vec![];
        accepted_by_author.resize_with(committee.size(), BTreeSet::default);
        let suspended_count = vec![0; committee.size()];
        let persisted = vec![0; committee.size()];
        let committed = vec![0; committee.size()];
        let genesis = SignedHeader::genesis(&committee)
            .into_iter()
            .map(|h| (h.key(), h))
            .collect();
        let mut inner = Inner {
            authority_id,
            committee,
            headers: Default::default(),
            accepted_by_author,
            accepted_by_round: Default::default(),
            suspended: Default::default(),
            suspended_count,
            genesis,
            persisted,
            highest_proposed_round: 0,
            pending_leaders: VecDeque::default(),
            leader_schedule,
            last_committed_sub_dag: None,
            recent_committed_sub_dags: VecDeque::default(),
            committed,
            highest_voting_leader_round: 0,
            metrics,
        };
        inner.recover(&header_store, &consensus_store);
        Self {
            inner: Arc::new(Mutex::new(inner)),
            header_store,
            consensus_store,
        }
    }

    /// Returns number of headers accepted.
    pub(crate) fn try_accept(&self, signed_headers: Vec<SignedHeader>) -> DagResult<usize> {
        let mut num_accepted = 0;
        let mut inner = self.inner.lock();
        for header in signed_headers {
            match self.try_accept_internal(&mut inner, header) {
                Ok(accepted) => {
                    if accepted {
                        num_accepted += 1;
                    }
                }
                Err(e) => {
                    warn!("Failure when accepting header: {:?}", e);
                }
            }
        }
        Ok(num_accepted)
    }

    // Tries to propose based on current DAG.
    // TODO(narwhalceti): keep track of byzantine validators, and do not include additional
    // headers from them.
    pub(crate) fn try_propose(&self) -> ProposeResult {
        let mut inner = self.inner.lock();
        inner.try_propose()
    }

    pub(crate) fn try_commit(&self) -> Vec<CommittedSubDag> {
        let _scope = monitored_scope("DagState::try_commit");
        let mut inner = self.inner.lock();
        let mut committed_sub_dags = Vec::new();
        loop {
            let (commits, leader_schedule_changed) = inner.try_commit();
            committed_sub_dags.extend(commits);
            if committed_sub_dags.is_empty() || !leader_schedule_changed {
                break;
            }
        }
        committed_sub_dags
    }

    pub(crate) fn read_headers(
        &self,
        exclusive_lower_bounds: Vec<(AuthorityIdentifier, Round)>,
        max_items: usize,
    ) -> DagResult<Vec<SignedHeader>> {
        let inner = self.inner.lock();

        // Use a min-queue for (round, authority) to keep track of the next certificate to fetch.
        //
        // Compared to fetching certificates iteratatively round by round, using a heap is simpler,
        // and avoids the pathological case of iterating through many missing rounds of a downed authority.
        let mut fetch_queue = BinaryHeap::new();
        let mut fetch_list = Vec::new();

        for (origin, lower_bound) in &exclusive_lower_bounds {
            let author_headers = &inner.accepted_by_author[origin.0 as usize];
            if *lower_bound < author_headers.first().unwrap().round() {
                fetch_queue.clear();
                fetch_list.clear();
                break;
            }
            let next_key = author_headers
                .range((
                    Included(HeaderKey::new(lower_bound + 1, *origin, Default::default())),
                    Unbounded,
                ))
                .next();
            if let Some(key) = next_key {
                fetch_queue.push(Reverse(*key));
                fetch_list.push(*key);
            }
        }

        if !fetch_queue.is_empty() {
            // Iteratively pop the next smallest (Round, Authority) pair, and push to min-heap the next
            // higher round of the same authority that should not be skipped.
            // The process ends when there are no more pairs in the min-heap.
            while let Some(Reverse(header_key)) = fetch_queue.pop() {
                // Allow the request handler to be stopped after timeout.
                let next_key = inner.accepted_by_author[header_key.author().0 as usize]
                    .range((
                        Included(HeaderKey::new(
                            header_key.round() + 1,
                            header_key.author(),
                            Default::default(),
                        )),
                        Unbounded,
                    ))
                    .next();
                if let Some(key) = next_key {
                    fetch_queue.push(Reverse(*key));
                    fetch_list.push(*key);
                }
                if fetch_list.len() == max_items {
                    debug!("Collected {} certificates, returning.", fetch_list.len(),);
                    break;
                }
            }

            return Ok(fetch_list
                .into_iter()
                .map(|key| inner.headers[&key].clone())
                .collect());
        }

        drop(inner);

        for (origin, lower_bound) in &exclusive_lower_bounds {
            let next_key = self
                .header_store
                .next_round_header_key(*origin, *lower_bound)?;
            if let Some(key) = next_key {
                fetch_queue.push(Reverse(key));
                fetch_list.push(key);
            }
        }

        // Iteratively pop the next smallest (Round, Authority) pair, and push to min-heap the next
        // higher round of the same authority that should not be skipped.
        // The process ends when there are no more pairs in the min-heap.
        while let Some(Reverse(header_key)) = fetch_queue.pop() {
            // Allow the request handler to be stopped after timeout.
            let next_key = self
                .header_store
                .next_round_header_key(header_key.author(), header_key.round())?;
            if let Some(key) = next_key {
                fetch_queue.push(Reverse(key));
                fetch_list.push(key);
            }
            if fetch_list.len() == max_items {
                debug!("Collected {} certificates, returning.", fetch_list.len(),);
                break;
            }
        }

        Ok(self
            .header_store
            .read_all(fetch_list)?
            .into_iter()
            .flatten()
            .collect())
    }

    pub(crate) fn last_round_per_authority(&self) -> BTreeMap<AuthorityIdentifier, Round> {
        let mut keys = BTreeMap::new();
        let inner = self.inner.lock();
        for headers in &inner.accepted_by_author {
            if let Some(key) = headers.last() {
                keys.insert(key.author(), key.round());
            }
        }
        keys
    }

    pub(crate) fn flush(&self) {
        let mut batch = self.header_store.batch();
        {
            let mut inner = self.inner.lock();
            inner
                .flush(&self.header_store, &self.consensus_store, &mut batch)
                .unwrap();
        }
        batch.write().unwrap();
    }

    pub(crate) fn num_suspended(&self) -> usize {
        let inner = self.inner.lock();
        inner.suspended.len()
    }

    /// Return true when the header is accepted, false otherwise.
    /// Failure is only possible when reading from storage, which should be rare.
    fn try_accept_internal(
        &self,
        inner: &mut Inner,
        signed_header: SignedHeader,
    ) -> DagResult<bool> {
        let header_key = signed_header.key();
        if inner.headers.contains_key(&header_key) {
            debug!(key=?header_key, "try_accept_internal: already accepted.");
            return Ok(false);
        }
        // if let Some(suspended_header) = inner.suspended.get(&key) {
        //     if suspended_header.signed_header.is_some() {
        //         return Ok(false);
        //     }
        // }

        let mut missing = vec![];
        let mut to_check = vec![];
        for ancestor in signed_header.header().ancestors() {
            // No need to check cache for genesis headers.
            if inner.genesis.contains_key(ancestor) {
                continue;
            }

            // Look up all accepted headers from the ancestor author.
            let author_headers = &inner.accepted_by_author[ancestor.author().0 as usize];
            assert!(!author_headers.is_empty());
            if author_headers.contains(ancestor) {
                continue;
            }

            // Optimization: accepted header cache and its indexes have the invariant that they
            // contain the most recent (in round) headers from each authority.
            // If a header missing from cache is more recent than some cached headers from the same
            // author, it will be missing from storage too.
            // TODO(narwhalceti): handle byzantine headers.
            if author_headers.last().unwrap().round() < ancestor.round() {
                missing.push(*ancestor);
            } else {
                // Otherwise, storage has to be consulted to determine if the header has been accepted.
                to_check.push(*ancestor);
            }
        }
        debug!(
            key=?header_key, missing=?missing, to_check=?to_check,
            "try_accept_internal: checked missing."
        );

        // In general accessing rocksdb in a critical section should be avoided.
        // But this should be very rare, especially when no node is Byzantine.
        if !to_check.is_empty() {
            let result = self
                .header_store
                .multi_contains(to_check.clone().into_iter())?;
            for (checked_key, exists) in to_check.into_iter().zip(result.into_iter()) {
                if !exists {
                    missing.push(checked_key);
                    error!(
                        "Header {} has unexpected missing ancestor {}",
                        header_key, checked_key
                    );
                }
            }
        }

        if !missing.is_empty() {
            debug!(key=?header_key, "try_accept_internal: suspended");
            for ancestor in &missing {
                inner
                    .suspended
                    .entry(*ancestor)
                    .or_default()
                    .dependents
                    .insert(header_key);
            }
            let suspended_header = inner.suspended.entry(header_key).or_default();
            if suspended_header.signed_header.is_none() {
                inner.suspended_count[header_key.author().0 as usize] += 1;
                suspended_header.signed_header = Some(signed_header);
                suspended_header.missing_ancestors = missing.into_iter().collect();
            } else {
                assert_eq!(
                    suspended_header.missing_ancestors,
                    missing.into_iter().collect(),
                    "Suspended header {} has inconsistent missing ancestors",
                    header_key,
                );
            }
            inner
                .metrics
                .certificates_suspended
                .with_label_values(&["missing"])
                .inc();
            return Ok(false);
        }

        debug!(key=?header_key, "try_accept_internal: accepting");
        inner.accept_internal(signed_header);

        inner
            .metrics
            .certificates_currently_suspended
            .set(inner.suspended.len() as i64);

        Ok(true)
    }

    #[cfg(test)]
    fn get_headers_at_round(&self, round: Round) -> Vec<SignedHeader> {
        let inner = self.inner.lock();
        let Some(headers_by_round) = inner.accepted_by_round.get(&round) else {
            return vec![];
        };
        headers_by_round
            .headers
            .iter()
            .map(|key| inner.headers[key].clone())
            .collect()
    }
}

struct Inner {
    // Identifier of this primary.
    authority_id: AuthorityIdentifier,
    // Committee of the current epoch.
    committee: Committee,

    // Stores all genesis headers.
    genesis: BTreeMap<HeaderKey, SignedHeader>,

    // Caches accepted headers.
    headers: BTreeMap<HeaderKey, SignedHeader>,
    // Index headers from an author in each Vec element.
    accepted_by_author: Vec<BTreeSet<HeaderKey>>,
    // An index into the `accepted` structure, to allow looking up Headers by round.
    accepted_by_round: BTreeMap<Round, HeadersByRound>,
    // Maps keys of suspended headers to the header content and remaining missing ancestors.
    suspended: BTreeMap<HeaderKey, SuspendedHeader>,
    // Number of suspended headers per author.
    suspended_count: Vec<usize>,

    // Watermark of persisted headers per author.
    persisted: Vec<Round>,

    // Highest round of proposed headers.
    highest_proposed_round: Round,

    // TODO(mysticeti): recover.
    // Leaders that cannot commit yet.
    pending_leaders: VecDeque<(Round, AuthorityIdentifier, LeaderSelectionStatus)>,
    // Reference to avoid selecting bad nodes as leaders.
    leader_schedule: LeaderSchedule,
    // Last committed sub dag.
    last_committed_sub_dag: Option<CommittedSubDag>,
    // TODO: make the format more efficient
    recent_committed_sub_dags: VecDeque<CommittedSubDag>,
    // Watermark of committed headers per author.
    committed: Vec<Round>,
    // Highest round where leaders are voted on.
    highest_voting_leader_round: Round,

    metrics: Arc<PrimaryMetrics>,
}

impl Inner {
    fn recover(&mut self, header_store: &HeaderStore, consensus_store: &ConsensusStore) {
        let genesis: Vec<_> = self.genesis.values().cloned().collect();
        let last_committed_round = consensus_store.read_last_committed();

        for (i, genesis_header) in genesis.iter().enumerate() {
            let author = (i as u16).into();
            let recent_headers = header_store
                .read_recent(author, HEADERS_CACHED_PER_AUTHORITY)
                .unwrap();
            self.persisted[i] = recent_headers.last().map_or(0, |h| h.round());
            if author == self.authority_id {
                self.highest_proposed_round = self.persisted[i];
            }

            if recent_headers.len() < HEADERS_CACHED_PER_AUTHORITY {
                self.accept_internal(genesis_header.clone());
            }
            for signed_header in recent_headers {
                self.accept_internal(signed_header);
            }

            self.committed[i] = last_committed_round.get(&author).map_or(0, |r| *r);
        }

        if let Some(last_consensus_commit) = consensus_store.get_latest_sub_dag() {
            let committed = header_store
                .read_all(last_consensus_commit.headers())
                .unwrap()
                .into_iter()
                .map(|h| self.make_certificate(&h.unwrap()))
                .collect();
            let leader = header_store
                .read(last_consensus_commit.leader())
                .unwrap()
                .unwrap();
            let leader = self.make_certificate(&leader);
            self.highest_voting_leader_round = leader.round();

            let committed_sub_dag =
                CommittedSubDag::from_commit(last_consensus_commit, committed, leader);
            self.last_committed_sub_dag = Some(committed_sub_dag);
        }

        let min_round = std::cmp::min(
            self.highest_proposed_round,
            self.last_committed_sub_dag
                .as_ref()
                .map_or(0, |c| c.leader_round()),
        );
        for header in header_store.read_after_round(min_round) {
            self.accept_internal(header);
        }
    }

    fn accept_internal(&mut self, signed_header: SignedHeader) {
        let mut to_accept = vec![signed_header];
        while let Some(header) = to_accept.pop() {
            // TODO(narwhalceti): carry out additional validations on the header, e.g. parent link.
            let header_key = header.key();
            debug!(key=?header_key, "try_accept_internal: accepted");
            let author_index = header_key.author().0 as usize;

            // GC is done in flush().
            self.headers.insert(header_key, header);
            let author_headers = &mut self.accepted_by_author[author_index];
            author_headers.insert(header_key);

            let insert_to_accepted_by_round =
                if let Some((round, _)) = self.accepted_by_round.first_key_value() {
                    header_key.round() >= *round
                } else {
                    true
                };
            if insert_to_accepted_by_round {
                let header_by_round = self
                    .accepted_by_round
                    .entry(header_key.round())
                    .or_default();
                header_by_round.headers.insert(header_key);
                if header_by_round.authors.insert(header_key.author()) {
                    header_by_round.total_stake += self.committee.stake_by_id(header_key.author());
                    if header_by_round.quorum_time.is_none()
                        && header_by_round.total_stake >= self.committee.quorum_threshold()
                    {
                        header_by_round.quorum_time = Some(Instant::now());
                    }
                }
            }

            // Try to accept dependents of the accepted header.
            let Some(suspended_header) = self.suspended.remove(&header_key) else {
                debug!(key=?header_key, "try_accept_internal: not suspended before");
                continue;
            };
            if !suspended_header.missing_ancestors.is_empty() {
                panic!(
                    "Suspended header {} should no longer have missing ancestors: {:?}",
                    header_key, suspended_header,
                );
            }
            debug!(key=?header_key, dependents=?suspended_header.dependents, "try_accept_internal: looping over dependents");
            for child in suspended_header.dependents {
                debug!(key=?header_key, child=?child, "try_accept_internal: removing missing ancestor link at child");
                let suspended_child = self
                    .suspended
                    .get_mut(&child)
                    .expect("missing_ancestors should exist!");
                suspended_child.missing_ancestors.remove(&header_key);
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
        let highest_proposed_round = self.highest_proposed_round;
        let mut parent_round = None;
        let mut next_check_delay = Duration::from_millis(100);
        let max_wait_threshold = Duration::from_millis(200);
        'search: for r in (highest_proposed_round..=self.highest_known_round()).rev() {
            let headers_by_round = &self.accepted_by_round[&r];
            let Some(quorum_time) = headers_by_round.quorum_time else {
                continue;
            };
            let quorum_elapsed = Instant::now() - quorum_time;
            let leaders = self.leader_schedule.leader_sequence(r);
            let wait_interval = max_wait_threshold / leaders.len() as u32;
            for (i, leader) in leaders.iter().enumerate() {
                if !headers_by_round.authors.contains(leader) {
                    continue;
                }
                let leader_wait_threshold = wait_interval * i as u32;
                if quorum_elapsed >= leader_wait_threshold {
                    parent_round = Some(r);
                    self.metrics
                        .headers_proposed
                        .with_label_values(&["support"])
                        .inc();
                    break 'search;
                } else {
                    next_check_delay = next_check_delay.min(leader_wait_threshold - quorum_elapsed);
                }
            }
            if quorum_elapsed >= max_wait_threshold {
                parent_round = Some(r);
                self.metrics
                    .headers_proposed
                    .with_label_values(&["reject"])
                    .inc();
                break 'search;
            } else {
                next_check_delay = next_check_delay.min(max_wait_threshold - quorum_elapsed);
            }
        }
        // There is no round above previously highest proposed round that has a quorum.
        if parent_round.is_none() {
            return ProposeResult {
                header_proposal: None,
                next_check_delay,
            };
        }

        let header_round = parent_round.unwrap() + 1;
        let mut ancestors = vec![];
        let mut ancestor_max_ts_ms = 0;
        for index in 0..self.committee.size() {
            let headers = &self.accepted_by_author[index];
            // TODO(narwhalceti): handle byzantine case, where a round can have multiple headers from the same author.
            let key = headers
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
            ancestor_max_ts_ms =
                std::cmp::max(ancestor_max_ts_ms, *self.headers[key].header().created_at());
        }
        assert_eq!(ancestors.len(), self.committee.size());
        self.highest_proposed_round = header_round;

        ProposeResult {
            header_proposal: Some((header_round, ancestors, ancestor_max_ts_ms)),
            next_check_delay,
        }
    }

    fn try_commit(&mut self) -> (Vec<CommittedSubDag>, bool) {
        if self.highest_known_round() == 0 {
            return (vec![], false);
        }

        // Create pending_leaders entries for potential leaders.
        for round in
            self.highest_voting_leader_round + 1..=self.highest_known_round().saturating_sub(2)
        {
            let leaders = self.leader_schedule.leader_sequence(round);
            for leader in leaders.into_iter() {
                self.pending_leaders
                    .push_back((round, leader, LeaderSelectionStatus::Undecided));
            }
            self.highest_voting_leader_round = round;
        }

        // Check if more pending leaders can have their status decided.
        for i in (0..self.pending_leaders.len()).rev() {
            let (round, author, status) = self.pending_leaders[i];
            // Status never changes after it is decided.
            if status != LeaderSelectionStatus::Undecided {
                continue;
            }
            let mut status = self.check_strong_certification(round, author);
            if status == LeaderSelectionStatus::Undecided {
                status = self.check_weak_certification(round, author, i)
            }
            self.pending_leaders[i].2 = status;
        }

        // Commit a prefix of leaders with support.
        let mut selected_leaders = Vec::new();
        while let Some((_round, _author, status)) = self.pending_leaders.front() {
            match status {
                LeaderSelectionStatus::StrongReject | LeaderSelectionStatus::WeakReject => {}
                LeaderSelectionStatus::StrongSupport(key) => {
                    selected_leaders.push(*key);
                }
                LeaderSelectionStatus::WeakSupport(key) => {
                    selected_leaders.push(*key);
                }
                LeaderSelectionStatus::Undecided => break,
            };
            self.metrics
                .leader_election_outcome
                .with_label_values(&[&status.to_string()])
                .inc();
            self.pending_leaders.pop_front();
        }

        // Generates commits for the selected leaders.
        // TODO: apply leader schedule.
        let mut commits = Vec::new();
        for leader in selected_leaders {
            let commit = self.commit_leader(leader);
            commits.push(commit.clone());
            self.last_committed_sub_dag = Some(commit.clone());
            self.recent_committed_sub_dags.push_back(commit);
        }

        (commits, false)
    }

    fn try_certifiy(&self, target: HeaderKey, certifier: HeaderKey) -> CertificationStatus {
        assert_eq!(target.round() + 2, certifier.round());
        let certifier_header = &self.headers[&certifier];
        let certifier_ancestor_headers: Vec<_> = certifier_header
            .header()
            .ancestors()
            .iter()
            .filter_map(|key| {
                if key.round() != target.round() + 1 {
                    return None;
                }
                Some(self.headers[key].clone())
            })
            .collect();
        let target_index = target.author().0 as usize;

        let mut support_vote = 0;
        let mut reject_vote = 0;
        for voter in certifier_ancestor_headers {
            if voter.round() != target.round() + 1 {
                continue;
            }
            let voter_stake = self.committee.stake_by_id(voter.author());
            if voter.header().ancestors()[target_index].round() == target.round() {
                support_vote += voter_stake;
            } else {
                reject_vote += voter_stake;
            }
        }

        if support_vote >= self.committee.quorum_threshold() {
            CertificationStatus::Support
        } else if reject_vote >= self.committee.quorum_threshold() {
            CertificationStatus::Reject
        } else {
            CertificationStatus::None
        }
    }

    fn check_strong_certification(
        &self,
        round: Round,
        author: AuthorityIdentifier,
    ) -> LeaderSelectionStatus {
        if round + 2 > self.highest_known_round() {
            return LeaderSelectionStatus::Undecided;
        }
        let headers_by_round = &self.accepted_by_round[&(round + 2)];
        if headers_by_round.total_stake < self.committee.quorum_threshold() {
            return LeaderSelectionStatus::Undecided;
        }

        let target_keys = self.accepted_by_author[author.0 as usize]
            .range((
                Included(HeaderKey::new(
                    round,
                    Default::default(),
                    Default::default(),
                )),
                Excluded(HeaderKey::new(
                    round + 1,
                    Default::default(),
                    Default::default(),
                )),
            ))
            .collect::<Vec<_>>();
        if target_keys.is_empty() {
            return LeaderSelectionStatus::StrongReject;
        }

        for target in target_keys {
            let mut support_certification = 0;
            let mut reject_certification = 0;
            let mut seen = BTreeSet::new();
            // TODO(narwhalceti): handle byzantine case, avoid checking recent headers from already
            // known byzantine authors.
            for key in &headers_by_round.headers {
                if !seen.insert(key.author()) {
                    continue;
                }
                let certifcation_status =
                    self.try_certifiy(HeaderKey::new(round, author, Default::default()), *key);
                match certifcation_status {
                    CertificationStatus::Support => {
                        support_certification += self.committee.stake_by_id(key.author());
                    }
                    CertificationStatus::Reject => {
                        reject_certification += self.committee.stake_by_id(key.author());
                    }
                    CertificationStatus::None => {}
                }
            }
            if support_certification >= self.committee.quorum_threshold() {
                return LeaderSelectionStatus::StrongSupport(*target);
            } else if reject_certification >= self.committee.quorum_threshold() {
                return LeaderSelectionStatus::StrongReject;
            }
        }

        LeaderSelectionStatus::Undecided
    }

    fn check_weak_certification(
        &self,
        target_round: Round,
        target_author: AuthorityIdentifier,
        target_index: usize,
    ) -> LeaderSelectionStatus {
        let min_anchor_round = target_round + 3;
        if min_anchor_round > self.highest_known_round() {
            return LeaderSelectionStatus::Undecided;
        }

        for i in target_index + self.leader_schedule.num_leaders_per_round() * 2
            ..self.pending_leaders.len()
        {
            let (r, _author, status) = &self.pending_leaders[i];
            // TODO(narwhalceti): instead of skipping, calculate the min_anchor_round boundary from self.pending_leaders.
            if *r < min_anchor_round {
                continue;
            }

            let anchor_key = match *status {
                // Do not use rejected leaders as anchors.
                LeaderSelectionStatus::StrongReject | LeaderSelectionStatus::WeakReject => continue,
                // Cannot skip over Undecided leader when choosing anchor.
                LeaderSelectionStatus::Undecided => break,
                // Use anchor with any type of support.
                LeaderSelectionStatus::StrongSupport(key) => key,
                LeaderSelectionStatus::WeakSupport(key) => key,
            };

            // After picking an anchor, determine if the target has weak support or reject.

            // Find all ancestors of the anchor in the target+2 (certification) round.
            // TODO(narwhalceti): rewrite this to use actual graph traversal.
            let mut certification_ancestors = Vec::new();
            let anchor_header = &self.headers[&anchor_key];
            for ancestor in anchor_header.header().ancestors() {
                let author_index = ancestor.author().0 as usize;
                let mut ancestor_key = *ancestor;
                // Find the ancestor that may certify target and is committed by the anchor.
                loop {
                    if ancestor_key.round() == target_round + 2 {
                        certification_ancestors.push(ancestor_key);
                    }
                    if ancestor_key.round() <= target_round + 2 {
                        break;
                    }
                    let ancestor_header = self.headers[&ancestor_key].header();
                    ancestor_key = ancestor_header.ancestors()[author_index];
                }
            }

            // Find all possible target leader keys.
            let target_keys = self.accepted_by_author[target_author.0 as usize]
                .range((
                    Included(HeaderKey::new(
                        target_round,
                        Default::default(),
                        Default::default(),
                    )),
                    Excluded(HeaderKey::new(
                        target_round + 1,
                        Default::default(),
                        Default::default(),
                    )),
                ))
                .collect::<Vec<_>>();
            assert!(!target_keys.is_empty(), "There should be at least one target key for author {} round {}, otherwise target should have been strongly rejected!", target_author, target_round);

            for target in target_keys {
                for certifer in &certification_ancestors {
                    match self.try_certifiy(*target, *certifer) {
                        CertificationStatus::Support => {
                            return LeaderSelectionStatus::WeakSupport(*target);
                        }
                        CertificationStatus::Reject => return LeaderSelectionStatus::WeakReject,
                        CertificationStatus::None => continue,
                    }
                }
            }

            return LeaderSelectionStatus::WeakReject;
        }

        LeaderSelectionStatus::Undecided
    }

    fn commit_leader(&mut self, leader: HeaderKey) -> CommittedSubDag {
        let mut commit_headers = vec![];
        let leader_header = &self.headers[&leader];
        commit_headers.push(leader_header);
        // TODO(narwhalceti): actually use graph traversal.
        for ancestor in leader_header.header().ancestors() {
            let author = ancestor.author().0 as usize;
            // TODO(narwhalceti): this tries to avoid recommitting, but byzantine failure needs to be handled.
            if self.committed[author] >= ancestor.round() {
                continue;
            }
            let author_headers = &self.accepted_by_author[author];
            let commit_iter = author_headers.range((
                Included(HeaderKey::new(
                    self.committed[author] + 1,
                    Default::default(),
                    Default::default(),
                )),
                Excluded(HeaderKey::new(
                    ancestor.round() + 1,
                    Default::default(),
                    Default::default(),
                )),
            ));
            for key in commit_iter {
                commit_headers.push(&self.headers[key]);
            }
        }
        for header in &commit_headers {
            let author = header.author().0 as usize;
            self.committed[author] = std::cmp::max(self.committed[author], header.round());
        }

        let certificates: Vec<_> = commit_headers
            .into_iter()
            .map(|h| self.make_certificate(h))
            .collect();
        let leader_certificate = certificates[0].clone();
        let reputation_score = self.compute_reputation_score(&certificates);
        CommittedSubDag::new_narwhalceti(
            certificates,
            leader_certificate,
            reputation_score,
            self.last_committed_sub_dag.as_ref(),
        )
    }

    /// Calculates the reputation score for the current commit by taking into account the reputation
    /// scores from the previous commit (assuming that exists). It returns the updated reputation score.
    fn compute_reputation_score(&self, committed_sequence: &[Certificate]) -> ReputationScores {
        // we reset the scores for every schedule change window, or initialise when it's the first
        // sub dag we are going to create.
        // TODO: when schedule change is implemented we should probably change a little bit
        // this logic here.
        const NUM_SUB_DAGS_PER_SCHEDULE: u64 = 50;
        let Some(last_committed_sub_dag) = self.last_committed_sub_dag.as_ref() else {
            return ReputationScores::new(&self.committee);
        };

        let sub_dag_index = last_committed_sub_dag.sub_dag_index + 1;
        let mut reputation_score = if sub_dag_index % NUM_SUB_DAGS_PER_SCHEDULE == 0 {
            ReputationScores::new(&self.committee)
        } else {
            last_committed_sub_dag.reputation_score.clone()
        };

        // update the score for the previous leader. If no previous leader exists,
        // then this is the first time we commit a leader, so no score update takes place
        for certificate in committed_sequence {
            reputation_score.add_score(certificate.origin(), 1);
        }

        // we check if this is the last sub dag of the current schedule. If yes then we mark the
        // scores as final_of_schedule = true so any downstream user can now that those are the last
        // ones calculated for the current schedule.
        reputation_score.final_of_schedule = (sub_dag_index + 1) % NUM_SUB_DAGS_PER_SCHEDULE == 0;

        // Always ensure that all the authorities are present in the reputation scores - even
        // when score is zero.
        assert_eq!(
            reputation_score.total_authorities() as usize,
            self.committee.size()
        );

        reputation_score
    }

    fn highest_known_round(&self) -> Round {
        self.accepted_by_round
            .last_key_value()
            .map(|(r, _)| *r)
            .unwrap()
    }

    fn last_committed_leader_round(&self) -> Round {
        self.last_committed_sub_dag
            .as_ref()
            .map(|commit| commit.leader_round())
            .unwrap_or(0)
    }

    fn flush(
        &mut self,
        header_store: &HeaderStore,
        consensus_store: &ConsensusStore,
        batch: &mut DBBatch,
    ) -> DagResult<()> {
        for (i, accepted) in self.accepted_by_author.iter().enumerate() {
            let author = AuthorityIdentifier(i as u16);
            let persisted = self.persisted[i];
            let headers = accepted
                .range((
                    Included(HeaderKey::new(persisted + 1, author, Default::default())),
                    Unbounded,
                ))
                .map(|key| self.headers.get(key).unwrap());
            header_store.write_all(headers, batch).unwrap();
            self.persisted[i] = std::cmp::max(self.persisted[i], accepted.last().unwrap().round());
        }
        // Clear cached headers that are no longer needed.
        for (author_index, accepted) in &mut self.accepted_by_author.iter_mut().enumerate() {
            // Keep a minimum number of headers per authority, or more if some headers need to be
            // kept for propose and commit.
            while accepted.len() > HEADERS_CACHED_PER_AUTHORITY {
                let key = accepted.first().unwrap();
                if key.round() >= self.persisted[author_index]
                    || key.round() >= self.committed[author_index]
                {
                    break;
                }
                self.headers.remove(key);
                accepted.pop_first().unwrap();
            }
        }
        // Clear per-round indexes that are no longer needed.
        while let Some((round, _)) = self.accepted_by_round.first_key_value() {
            if self.accepted_by_round.len() <= HEADER_ROUNDS_CACHED {
                break;
            }
            if *round >= self.highest_proposed_round {
                break;
            }
            if *round >= self.last_committed_leader_round() {
                break;
            }
            self.accepted_by_round.pop_first();
        }

        consensus_store
            .update(
                self.committed
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (AuthorityIdentifier(i as u16), *r)),
                self.recent_committed_sub_dags.iter(),
                batch,
            )
            .unwrap();
        // Clear recent_committed_sub_dag.
        self.recent_committed_sub_dags.clear();

        Ok(())
    }

    fn make_certificate(&self, header: &SignedHeader) -> Certificate {
        CertificateV2::new_unsigned(&self.committee, header.header().clone(), Vec::new()).unwrap()
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
#[derive(Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeaderSelectionStatus {
    Undecided,
    StrongSupport(HeaderKey),
    WeakSupport(HeaderKey),
    StrongReject,
    WeakReject,
}

impl fmt::Display for LeaderSelectionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LeaderSelectionStatus::Undecided => write!(f, "Undecided"),
            LeaderSelectionStatus::StrongSupport(_) => write!(f, "StrongSupport"),
            LeaderSelectionStatus::WeakSupport(_) => write!(f, "WeakSupport"),
            LeaderSelectionStatus::StrongReject => write!(f, "StrongReject"),
            LeaderSelectionStatus::WeakReject => write!(f, "WeakReject"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CertificationStatus {
    None,
    Support,
    Reject,
}

#[cfg(test)]
mod test {
    use std::num::NonZeroUsize;

    use fastcrypto::serde_helpers::BytesRepresentation;
    use itertools::Itertools;
    use prometheus::Registry;
    use storage::NodeStorage;
    use test_utils::{temp_dir, CommitteeFixture};
    use types::{Header, HeaderV3};

    use crate::consensus::LeaderSwapTable;

    use super::*;

    fn create_header_with_ancestors(
        round: Round,
        author: AuthorityIdentifier,
        ancestors: Vec<HeaderKey>,
    ) -> SignedHeader {
        let header: Header =
            HeaderV3::new(author, round, 0, Default::default(), vec![], ancestors).into();
        let signature_bytes = BytesRepresentation::<64>([0u8; 64]);
        SignedHeader::new(header, signature_bytes)
    }

    fn compare_headers(a: &[SignedHeader], b: &[SignedHeader]) {
        let a_set = a.iter().map(|h| h.key()).collect::<BTreeSet<_>>();
        let b_set = b.iter().map(|h| h.key()).collect::<BTreeSet<_>>();
        for key in a_set.difference(&b_set) {
            println!("a has but b does not: {}", key);
        }
        for key in b_set.difference(&a_set) {
            println!("b has but a does not: {}", key);
        }
        assert_eq!(a_set, b_set);
    }

    fn check_commits(
        commits: &[CommittedSubDag],
        expected: &[(Round, AuthorityIdentifier)],
        mut last_sub_dag_index: u64,
    ) {
        assert_eq!(
            commits.len(),
            expected.len(),
            "commits: {:?}, expected: {:?}",
            commits,
            expected
        );
        for (commit, (round, author)) in commits.iter().zip(expected.iter()) {
            last_sub_dag_index += 1;
            assert_eq!(
                commit.sub_dag_index, last_sub_dag_index,
                "{commit:?}, {last_sub_dag_index}, {round}, {author}"
            );
            assert_eq!(
                commit.leader.round(),
                *round,
                "{commit:?}, {last_sub_dag_index}, {round}, {author}"
            );
            assert_eq!(
                commit.leader.origin(),
                *author,
                "{commit:?}, {last_sub_dag_index}, {round}, {author}"
            );
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_basics() {
        // This committee has equi-sized stakes
        let committee_size = 4;
        let fixture = CommitteeFixture::builder()
            .committee_size(NonZeroUsize::new(committee_size).unwrap())
            .build();
        // TODO(narwhalceti): create dedicated test util for db.
        let leader_schedule = LeaderSchedule::new(
            fixture.committee().clone(),
            LeaderSwapTable::new_empty(&fixture.committee()),
        );
        let store = NodeStorage::reopen(temp_dir(), None);
        let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
        let dag_state = DagState::new(
            AuthorityIdentifier(0),
            fixture.committee(),
            leader_schedule,
            store.header_store,
            store.consensus_store.as_ref().clone(),
            metrics,
        );
        let mut round_headers = vec![dag_state.get_headers_at_round(0)];
        let mut round_keys = vec![round_headers[0].iter().map(|h| h.key()).collect_vec()];

        // Round 1: accept headers one at a time, connected to all parents, no suspension.
        let round = 1;
        let headers = (0..committee_size as u16)
            .map(|i| {
                create_header_with_ancestors(
                    round,
                    AuthorityIdentifier(i),
                    round_keys[round as usize - 1].clone(),
                )
            })
            .collect_vec();
        round_headers.push(headers.clone());
        let keys = headers.iter().map(|h| h.key()).collect_vec();
        round_keys.push(keys.clone());
        for h in &headers {
            assert_eq!(dag_state.try_accept(vec![h.clone()]).unwrap(), 1);
        }
        let got_headers = dag_state.get_headers_at_round(round);
        compare_headers(&headers, &got_headers);

        // 1st header proposal.
        let propose_result = dag_state.try_propose();
        assert_eq!(propose_result.header_proposal.unwrap().0, 2);

        // Round 2: accept headers together, connected to all parents, no suspension.
        let round = 2;
        let headers = (0..committee_size as u16)
            .map(|i| {
                create_header_with_ancestors(
                    round,
                    AuthorityIdentifier(i),
                    round_keys[round as usize - 1].clone(),
                )
            })
            .collect_vec();
        round_headers.push(headers.clone());
        let keys = headers.iter().map(|h| h.key()).collect_vec();
        round_keys.push(keys.clone());
        assert_eq!(dag_state.try_accept(headers.clone()).unwrap(), 4);
        let got_headers = dag_state.get_headers_at_round(round);
        compare_headers(&headers, &got_headers);

        // 2nd header proposal.
        let propose_result = dag_state.try_propose();
        assert_eq!(propose_result.header_proposal.unwrap().0, 3);

        // Round 3 and 4, first suspend round 4, then accept round 3 which will accept round 4.
        let round = 3;
        let round_3_headers = (0..(committee_size - 1) as u16)
            .map(|i| {
                create_header_with_ancestors(
                    round,
                    AuthorityIdentifier(i),
                    round_keys[round as usize - 1].clone(),
                )
            })
            .collect_vec();
        round_headers.push(round_3_headers.clone());
        let round_3_keys = round_3_headers.iter().map(|h| h.key()).collect_vec();
        round_keys.push(round_3_keys.clone());
        assert!(dag_state.get_headers_at_round(3).is_empty());

        let round = 4;
        let mut round_4_ancestors = round_3_keys.clone();
        round_4_ancestors.push(round_keys[2][committee_size - 1]);
        let round_4_headers = (0..(committee_size - 1) as u16)
            .map(|i| {
                create_header_with_ancestors(
                    round,
                    AuthorityIdentifier(i),
                    round_4_ancestors.clone(),
                )
            })
            .collect_vec();
        round_headers.push(round_4_headers.clone());
        let round_4_keys = round_4_headers.iter().map(|h| h.key()).collect_vec();
        round_keys.push(round_4_keys.clone());

        assert_eq!(dag_state.try_accept(round_4_headers.clone()).unwrap(), 0);
        assert!(dag_state.get_headers_at_round(4).is_empty());

        for h in &round_3_headers {
            assert_eq!(dag_state.try_accept(vec![h.clone()]).unwrap(), 1);
        }

        compare_headers(&round_3_headers, &dag_state.get_headers_at_round(3));
        compare_headers(&round_4_headers, &dag_state.get_headers_at_round(4));

        // 3rd header proposal.
        let propose_result = dag_state.try_propose();
        assert_eq!(propose_result.header_proposal.unwrap().0, 5);

        // Try commit
        let commits = dag_state.try_commit();
        check_commits(
            &commits,
            &[(1, AuthorityIdentifier(2)), (2, AuthorityIdentifier(1))],
            0,
        );

        // No header proposal.
        let propose_result = dag_state.try_propose();
        assert!(propose_result.header_proposal.is_none());
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_strong_reject() {
        // This committee has equi-sized stakes
        let committee_size = 4;
        let fixture = CommitteeFixture::builder()
            .committee_size(NonZeroUsize::new(committee_size).unwrap())
            .build();
        // TODO(narwhalceti): create dedicated test util for db.
        let leader_schedule = LeaderSchedule::new(
            fixture.committee().clone(),
            LeaderSwapTable::new_empty(&fixture.committee()),
        );
        let store = NodeStorage::reopen(temp_dir(), None);
        let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
        let dag_state = DagState::new(
            AuthorityIdentifier(0),
            fixture.committee(),
            leader_schedule,
            store.header_store,
            store.consensus_store.as_ref().clone(),
            metrics,
        );
        let mut all_headers = dag_state.get_headers_at_round(0);
        let mut round_keys = vec![all_headers.iter().map(|h| h.key()).collect_vec()];

        // Round 1: leader is 2. all headers connect to genesis parents.
        let round_1_headers = (0..4_u16)
            .map(|i| create_header_with_ancestors(1, AuthorityIdentifier(i), round_keys[0].clone()))
            .collect_vec();
        let round_1_keys = round_1_headers.iter().map(|h| h.key()).collect_vec();
        all_headers.extend(round_1_headers.into_iter());
        round_keys.push(round_1_keys);

        // Round 2: validators do not support round 1 leader 2.
        let ancestors = vec![
            round_keys[1][0],
            round_keys[1][1],
            round_keys[0][2],
            round_keys[1][3],
        ];
        let mut round_2_headers = vec![0, 1, 3]
            .into_iter()
            .map(|i| create_header_with_ancestors(2, AuthorityIdentifier(i), ancestors.clone()))
            .collect_vec();
        round_2_headers.insert(
            2,
            create_header_with_ancestors(
                2,
                AuthorityIdentifier(2),
                vec![
                    round_keys[1][0],
                    round_keys[1][1],
                    round_keys[1][2],
                    round_keys[1][3],
                ],
            ),
        );
        let round_2_keys = round_2_headers.iter().map(|h| h.key()).collect_vec();
        all_headers.extend(round_2_headers.into_iter());
        round_keys.push(round_2_keys);

        // Round 3: all validators include round 2 validators 0,1,3 as ancestors.
        let ancestors = vec![
            round_keys[2][0],
            round_keys[2][1],
            round_keys[1][2],
            round_keys[2][3],
        ];
        let mut round_3_headers = vec![0, 1, 3]
            .into_iter()
            .map(|i| create_header_with_ancestors(3, AuthorityIdentifier(i), ancestors.clone()))
            .collect_vec();
        round_3_headers.insert(
            2,
            create_header_with_ancestors(
                3,
                AuthorityIdentifier(2),
                vec![
                    round_keys[2][0],
                    round_keys[2][1],
                    round_keys[2][2],
                    round_keys[2][3],
                ],
            ),
        );
        let round_3_keys = round_3_headers.iter().map(|h| h.key()).collect_vec();
        all_headers.extend(round_3_headers.into_iter());
        round_keys.push(round_3_keys);

        // Round 4: all headers connect to all parents.
        let round_4_headers = (0..4_u16)
            .map(|i| create_header_with_ancestors(4, AuthorityIdentifier(i), round_keys[3].clone()))
            .collect_vec();
        let round_4_keys = round_4_headers.iter().map(|h| h.key()).collect_vec();
        all_headers.extend(round_4_headers.into_iter());
        round_keys.push(round_4_keys);

        // Reorder headers before trying to accept them, to test DagState's ability to handle
        // out of order headers.
        all_headers.sort_by_key(|h| h.key().digest());
        dag_state.try_accept(all_headers).unwrap();
        assert_eq!(dag_state.num_suspended(), 0);

        // Check only round 2 leader 1 is committed, which implies round 1 leader 2
        // is strongly rejected.
        let commits = dag_state.try_commit();
        check_commits(&commits, &[(2, AuthorityIdentifier(1))], 0);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_weak_support_and_reject() {
        // This committee has equi-sized stakes
        let committee_size = 4;
        let fixture = CommitteeFixture::builder()
            .committee_size(NonZeroUsize::new(committee_size).unwrap())
            .build();
        // TODO(narwhalceti): create dedicated test util for db.
        let leader_schedule = LeaderSchedule::new(
            fixture.committee().clone(),
            LeaderSwapTable::new_empty(&fixture.committee()),
        );
        let store = NodeStorage::reopen(temp_dir(), None);
        let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
        let dag_state = DagState::new(
            AuthorityIdentifier(0),
            fixture.committee(),
            leader_schedule,
            store.header_store,
            store.consensus_store.as_ref().clone(),
            metrics,
        );
        let mut all_headers = dag_state.get_headers_at_round(0);
        let mut round_keys = vec![all_headers.iter().map(|h| h.key()).collect_vec()];

        // Round 1 (leader 2): all headers connect to genesis parents.
        let round_1_headers = (0..4_u16)
            .map(|i| create_header_with_ancestors(1, AuthorityIdentifier(i), round_keys[0].clone()))
            .collect_vec();
        let round_1_keys = round_1_headers.iter().map(|h| h.key()).collect_vec();
        all_headers.extend(round_1_headers.into_iter());
        round_keys.push(round_1_keys);

        // Round 2 (leader 1): validators 0~2 support round 1 leader 2, to give it weak support later.
        let ancestors = vec![
            round_keys[1][0],
            round_keys[1][1],
            round_keys[1][2],
            round_keys[0][3],
        ];
        let mut round_2_headers = (0..3_u16)
            .map(|i| create_header_with_ancestors(2, AuthorityIdentifier(i), ancestors.clone()))
            .collect_vec();
        round_2_headers.push(create_header_with_ancestors(
            2,
            AuthorityIdentifier(3),
            vec![
                round_keys[1][0],
                round_keys[1][1],
                round_keys[0][2],
                round_keys[1][3],
            ],
        ));
        let round_2_keys = round_2_headers.iter().map(|h| h.key()).collect_vec();
        all_headers.extend(round_2_headers.into_iter());
        round_keys.push(round_2_keys);

        // Round 3: Only 1 validator (3) has all of round 2 validators 0~2 as ancestors.
        // Also, 2 validators should support and reject round 2 leader 1, to give it weak reject.
        let mut round_3_headers = vec![
            create_header_with_ancestors(
                3,
                AuthorityIdentifier(0),
                vec![
                    round_keys[2][0],
                    round_keys[2][1],
                    round_keys[2][2],
                    round_keys[2][3],
                ],
            ),
            create_header_with_ancestors(
                3,
                AuthorityIdentifier(1),
                vec![
                    round_keys[2][0],
                    round_keys[2][1],
                    round_keys[1][2],
                    round_keys[2][3],
                ],
            ),
        ];
        let ancestors = vec![
            round_keys[2][0],
            round_keys[1][1],
            round_keys[2][2],
            round_keys[2][3],
        ];
        round_3_headers.extend(
            (2..4_u16).map(|i| {
                create_header_with_ancestors(3, AuthorityIdentifier(i), ancestors.clone())
            }),
        );
        let round_3_keys = round_3_headers.iter().map(|h| h.key()).collect_vec();
        all_headers.extend(round_3_headers.into_iter());
        round_keys.push(round_3_keys);

        // Try to accept the headers in dag store.
        all_headers.sort_by_key(|h| h.key().digest());
        dag_state.try_accept(all_headers.clone()).unwrap();
        assert_eq!(dag_state.num_suspended(), 0);
        // Cannot commit round 1 because it should be undecided.
        assert_eq!(dag_state.try_commit().len(), 0);

        // Round 4: all headers connect to all parents.
        let round_4_headers = (0..4_u16)
            .map(|i| create_header_with_ancestors(4, AuthorityIdentifier(i), round_keys[3].clone()))
            .collect_vec();
        let round_4_keys = round_4_headers.iter().map(|h| h.key()).collect_vec();
        all_headers.extend(round_4_headers.into_iter());
        round_keys.push(round_4_keys);

        // Try to accept the headers in dag store, with duplicates.
        all_headers.sort_by_key(|h| h.key().digest());
        dag_state.try_accept(all_headers.clone()).unwrap();
        assert_eq!(dag_state.num_suspended(), 0);
        // Cannot commit round 1 or round 2 because they should be undecided.
        assert_eq!(dag_state.try_commit().len(), 0);

        // Round 5: all headers connect to all parents.
        let round = 5;
        let headers = (0..4_u16)
            .map(|i| {
                create_header_with_ancestors(
                    round,
                    AuthorityIdentifier(i),
                    round_keys[(round - 1) as usize].clone(),
                )
            })
            .collect_vec();
        let keys = headers.iter().map(|h| h.key()).collect_vec();
        all_headers.extend(headers.into_iter());
        round_keys.push(keys);

        // Try to accept the headers in dag store, with duplicates.
        all_headers.sort_by_key(|h| h.key().digest());
        dag_state.try_accept(all_headers.clone()).unwrap();
        assert_eq!(dag_state.num_suspended(), 0);
        // Even if round 3 leader is strongly accepted now, consensus cannot commit
        // because round 1 and 2 leaders are still undecided.
        assert_eq!(dag_state.try_commit().len(), 0);

        // Round 6: all headers connect to all parents.
        // This should give round 4 leader strong support.
        let round = 6;
        let headers = (0..4_u16)
            .map(|i| {
                create_header_with_ancestors(
                    round,
                    AuthorityIdentifier(i),
                    round_keys[(round - 1) as usize].clone(),
                )
            })
            .collect_vec();
        let keys = headers.iter().map(|h| h.key()).collect_vec();
        all_headers.extend(headers.into_iter());
        round_keys.push(keys);

        // Round 7: all headers connect to all parents.
        // This should give round 5 leader strong support.
        let round = 7;
        let headers = (0..4_u16)
            .map(|i| {
                create_header_with_ancestors(
                    round,
                    AuthorityIdentifier(i),
                    round_keys[(round - 1) as usize].clone(),
                )
            })
            .collect_vec();
        let keys = headers.iter().map(|h| h.key()).collect_vec();
        all_headers.extend(headers.into_iter());
        round_keys.push(keys);

        // Try to accept the headers in dag store, with duplicates.
        all_headers.sort_by_key(|h| h.key().digest());
        dag_state.try_accept(all_headers.clone()).unwrap();
        assert_eq!(dag_state.num_suspended(), 0);

        // Check only round 1, 3, 4, 5 leaders get committed.
        // TODO(narwhalceti): pass strong/weak support/reject status via commit.
        let commits = dag_state.try_commit();
        check_commits(
            &commits,
            &[
                (1, AuthorityIdentifier(2)),
                (3, AuthorityIdentifier(2)),
                (4, AuthorityIdentifier(1)),
                (5, AuthorityIdentifier(1)),
            ],
            0,
        );
    }
}
