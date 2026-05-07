// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, marker::PhantomData};

use consensus_config::{AuthorityIndex, Committee, Stake};
use vstd::prelude::*;

verus! {

// Spec function names from the shims module exist only when Verus is the compiler.
#[cfg(verus_only)]
use crate::verus_shims::{
    authority_index_value_spec, committee_quorum_spec, committee_stake_seq,
    committee_validity_spec,
};


// ---------------------------------------------------------------------------
// CommitteeThreshold
// ---------------------------------------------------------------------------
//
// We add an extra `threshold_spec` ghost function so that the aggregator can
// state post-conditions in terms of "did we reach the threshold?" without
// caring which threshold (quorum vs. validity).

pub trait CommitteeThreshold {
    // Abstract threshold value used by callers' postconditions; concrete impls below pin it down.
    spec fn threshold_spec(committee: &Committee) -> u64;

    fn is_threshold(committee: &Committee, amount: Stake) -> (out: bool)
        // Caller can rely on: result is true exactly when `amount` reaches the threshold.
        ensures out == (amount >= Self::threshold_spec(committee));

    fn threshold(committee: &Committee) -> (out: Stake)
        // Caller can rely on: this returns the same number used in `threshold_spec`.
        ensures out == Self::threshold_spec(committee);
}

#[derive(Default)]
pub struct QuorumThreshold;

#[cfg(test)]
#[derive(Default)]
pub struct ValidityThreshold;

impl CommitteeThreshold for QuorumThreshold {
    // For quorum mode, the abstract threshold is the committee's quorum_spec.
    open spec fn threshold_spec(committee: &Committee) -> u64 {
        committee_quorum_spec(committee)
    }

    fn is_threshold(committee: &Committee, amount: Stake) -> (out: bool)
        // Result reflects whether amount reached committee quorum.
        ensures out == (amount >= Self::threshold_spec(committee))
    {
        committee.reached_quorum(amount)
    }

    fn threshold(committee: &Committee) -> (out: Stake)
        // Returns the runtime quorum number, which equals the spec value.
        ensures out == Self::threshold_spec(committee)
    {
        committee.quorum_threshold()
    }
}

#[cfg(test)]
impl CommitteeThreshold for ValidityThreshold {
    // For validity mode, the abstract threshold is committee's validity_spec.
    open spec fn threshold_spec(committee: &Committee) -> u64 {
        committee_validity_spec(committee)
    }

    fn is_threshold(committee: &Committee, amount: Stake) -> (out: bool)
        // Result reflects whether amount reached committee validity.
        ensures out == (amount >= Self::threshold_spec(committee))
    {
        committee.reached_validity(amount)
    }

    fn threshold(committee: &Committee) -> (out: Stake)
        // Returns runtime validity number, equal to the spec value.
        ensures out == Self::threshold_spec(committee)
    {
        committee.validity_threshold()
    }
}

// ---------------------------------------------------------------------------
// VoteSet
// ---------------------------------------------------------------------------
//
// A thin wrapper around `BTreeSet<AuthorityIndex>` that exposes a Verus
// `Set<int>` view. Methods are `external_body`: their bodies are not verified
// — we trust that `BTreeSet` does what its docs say.

pub struct VoteSet {
    pub inner: BTreeSet<AuthorityIndex>,
}

impl View for VoteSet {
    type V = Set<int>;
    // Ghost projection of the runtime BTreeSet to a mathematical Set<int>.
    uninterp spec fn view(&self) -> Set<int>;
}

impl VoteSet {
    #[verifier::external_body]
    pub fn new() -> (out: Self)
        // A freshly-constructed VoteSet has the empty set as its view.
        ensures out@ =~= Set::<int>::empty(),
    {
        Self { inner: BTreeSet::new() }
    }

    #[verifier::external_body]
    pub fn insert(&mut self, vote: AuthorityIndex) -> (out: bool)
        ensures
            // After insert, the view is the old view plus this vote.
            self@ =~= old(self)@.insert(authority_index_value_spec(vote)),
            // Returned bool matches BTreeSet semantics: true iff the element was new.
            out == !old(self)@.contains(authority_index_value_spec(vote)),
    {
        self.inner.insert(vote)
    }

    #[verifier::external_body]
    pub fn clear(&mut self)
        // After clear, the view is empty regardless of what was there before.
        ensures self@ =~= Set::<int>::empty(),
    {
        self.inner.clear();
    }
}

impl Default for VoteSet {
    #[verifier::external_body]
    fn default() -> (out: Self)
        // Default == new(): empty set.
        ensures out@ =~= Set::<int>::empty(),
    {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Sum-of-stakes spec
// ---------------------------------------------------------------------------
//
// `sum_stakes(c, votes)` is the total stake of every authority `i` in
// `votes`, where `c[i]` is authority i's stake. We define it iteratively
// over indices 0..n to keep the recursion deterministic and to give Verus
// trigger-friendly definitions.

pub open spec fn sum_stakes_le(c: Seq<u64>, votes: Set<int>, n: int) -> int
    // Recursion on n; Verus needs this hint to prove termination.
    decreases n,
{
    // Empty prefix has zero total stake.
    if n <= 0 {
        0
    // If index n-1 is in the voter set, count its stake plus the sum over the smaller prefix.
    } else if votes.contains(n - 1) {
        c[n - 1] as int + sum_stakes_le(c, votes, n - 1)
    // Otherwise just recurse on the smaller prefix without adding anything.
    } else {
        sum_stakes_le(c, votes, n - 1)
    }
}

pub open spec fn sum_stakes(c: Seq<u64>, votes: Set<int>) -> int {
    // Total over the full range of authority indices.
    sum_stakes_le(c, votes, c.len() as int)
}

/// Inserting a fresh authority adds its stake to the partial sum.
pub proof fn lemma_sum_insert(c: Seq<u64>, votes: Set<int>, x: int, n: int)
    requires
        // n must index into a valid prefix of the stake table.
        0 <= n <= c.len(),
        // x must be a valid authority index.
        0 <= x < c.len(),
        // x is genuinely new — otherwise inserting would be a no-op and the lemma trivializes.
        !votes.contains(x),
    ensures
        // Adding x to the voter set bumps the partial sum by c[x] iff x falls within the prefix 0..n.
        sum_stakes_le(c, votes.insert(x), n) ==
            sum_stakes_le(c, votes, n) + (if x < n { c[x] as int } else { 0int }),
    // Induction on n; matches the recursive definition of sum_stakes_le.
    decreases n,
{
    // Base case: prefix is empty, both sides are 0; nothing to prove.
    if n <= 0 {
    } else {
        // Inductive case: assume the property at n-1 and lift to n.
        lemma_sum_insert(c, votes, x, n - 1);
    }
}

pub proof fn lemma_sum_insert_full(c: Seq<u64>, votes: Set<int>, x: int)
    requires
        // x is a valid index into the stake table.
        0 <= x < c.len(),
        // x is not already a voter.
        !votes.contains(x),
    ensures
        // Inserting x grows the total stake by exactly c[x].
        sum_stakes(c, votes.insert(x)) == sum_stakes(c, votes) + c[x] as int,
{
    // Specialize the prefix lemma to the full range of authority indices.
    lemma_sum_insert(c, votes, x, c.len() as int);
}

/// The empty vote set has zero total stake at every prefix length.
pub proof fn lemma_sum_stakes_empty(c: Seq<u64>, n: int)
    // n must be non-negative; matches the recursion guard in sum_stakes_le.
    requires 0 <= n,
    // For any prefix length, an empty voter set sums to 0.
    ensures sum_stakes_le(c, Set::<int>::empty(), n) == 0,
    // Induction on n.
    decreases n,
{
    // Base case: empty prefix is trivially 0.
    if n <= 0 {
    } else {
        // Inductive case: recurse on n-1 and the result for n follows.
        lemma_sum_stakes_empty(c, n - 1);
    }
}

pub broadcast proof fn lemma_sum_stakes_empty_full(c: Seq<u64>)
    // Broadcast variant: callers don't need to invoke this lemma manually; Verus pulls it in via the trigger.
    ensures #[trigger] sum_stakes(c, Set::<int>::empty()) == 0,
{
    // Just specialize the prefix lemma to the full range.
    lemma_sum_stakes_empty(c, c.len() as int);
}

// ---------------------------------------------------------------------------
// StakeAggregator
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct StakeAggregator<T> {
    pub votes: VoteSet,
    pub stake: Stake,
    pub _phantom: PhantomData<T>,
}

impl<T: CommitteeThreshold> StakeAggregator<T> {
    /// Sum invariant: `stake` equals the total stake of the authorities
    /// recorded in `votes`. Every voter is also a valid authority index.
    pub open spec fn invariant_against(&self, c: &Committee) -> bool {
        // Every recorded voter must reference a real authority within bounds.
        (forall|i: int| #[trigger] self.votes@.contains(i)
            ==> 0 <= i < committee_stake_seq(c).len())
        // The running stake field exactly matches the sum over the recorded voters.
        && self.stake as int == sum_stakes(committee_stake_seq(c), self.votes@)
    }

    /// Whether an authority index has been recorded in this aggregator.
    /// Used as the key predicate for reasoning about duplicate-vote behaviour.
    pub open spec fn has_voted(&self, v: AuthorityIndex) -> bool {
        self.votes@.contains(authority_index_value_spec(v))
    }

    pub fn new() -> (out: Self)
        ensures
            // Fresh aggregator has zero stake.
            out.stake == 0,
            // Fresh aggregator has no voters.
            out.votes@ =~= Set::<int>::empty(),
            // The invariant holds against every possible committee — caller picks `c` later.
            forall|c: &Committee| out.invariant_against(c),
    {
        broadcast use lemma_sum_stakes_empty_full;
        Self {
            votes: VoteSet::default(),
            stake: 0,
            _phantom: PhantomData,
        }
    }

    /// Adds a vote for the specified authority index to the aggregator. It is
    /// guaranteed to count the vote only once for an authority. The method
    /// returns true when the required threshold has been reached.
    pub fn add(
        &mut self,
        vote: AuthorityIndex,
        committee: &Committee,
    ) -> (out: bool)
        requires
            // Aggregator was already in a valid state for this committee.
            old(self).invariant_against(committee),
            // Vote refers to a real authority index in bounds.
            0 <= authority_index_value_spec(vote) < committee_stake_seq(committee).len(),
            // Caller guarantees no overflow when we add this authority's stake.
            old(self).stake as int +
                committee_stake_seq(committee)[authority_index_value_spec(vote)] as int
                <= u64::MAX as int,
        ensures
            // Invariant is preserved for the same committee.
            self.invariant_against(committee),
            // Returned bool reflects whether we now meet the threshold.
            out == (self.stake >= T::threshold_spec(committee)),
            // Idempotence: stake either unchanged, or grew by exactly this
            // authority's stake.
            self.stake == old(self).stake
                || self.stake as int == old(self).stake as int +
                    committee_stake_seq(committee)[authority_index_value_spec(vote)] as int,
            // Behavioral: the vote is now recorded regardless of whether it was new.
            self.has_voted(vote),
            // Behavioral: the voter set only grows — no vote is evicted.
            forall|i: int| #[trigger] old(self).votes@.contains(i)
                ==> self.votes@.contains(i),
    {
        if self.votes.insert(vote) {
            self.stake = self.stake + committee.stake(vote);
            proof {
                // Bind x to the spec-level index of the new voter for clarity.
                let x = authority_index_value_spec(vote);
                // Snapshot the pre-insert voter set; needed to apply the sum lemma.
                let before = old(self).votes@;
                // Now sum_stakes(c, before.insert(x)) == sum_stakes(c, before) + c[x].
                lemma_sum_insert_full(committee_stake_seq(committee), before, x);
                // The invariant follows: new stake == old stake + c[x] == new total sum.
                assert(self.invariant_against(committee));
            }
        } else {
            proof {
                // Insert was a no-op (vote was already present), so the view didn't change.
                assert(self.votes@ =~= old(self).votes@);
                // Stake field wasn't modified either.
                assert(self.stake == old(self).stake);
                // Hence the old invariant carries over unchanged.
                assert(self.invariant_against(committee));
            }
        }
        T::is_threshold(committee, self.stake)
    }

    /// Adds a vote for the specified authority index to the aggregator. It is
    /// guaranteed to count the vote only once for an authority. The method
    /// returns true when the vote comes from a new authority and is counted.
    pub fn add_unique(
        &mut self,
        vote: AuthorityIndex,
        committee: &Committee,
    ) -> (out: bool)
        requires
            // Aggregator was already valid for this committee.
            old(self).invariant_against(committee),
            // Vote indexes a real authority.
            0 <= authority_index_value_spec(vote) < committee_stake_seq(committee).len(),
            // No overflow when this authority's stake is added.
            old(self).stake as int +
                committee_stake_seq(committee)[authority_index_value_spec(vote)] as int
                <= u64::MAX as int,
        ensures
            // Invariant preserved.
            self.invariant_against(committee),
            // Returned bool says whether this vote was newly counted (vs. duplicate).
            out == !old(self).has_voted(vote),
            // Same idempotence guarantee as `add`.
            self.stake == old(self).stake
                || self.stake as int == old(self).stake as int +
                    committee_stake_seq(committee)[authority_index_value_spec(vote)] as int,
            // Behavioral: the vote is now recorded regardless of whether it was new.
            self.has_voted(vote),
            // Behavioral: the voter set only grows — no vote is evicted.
            forall|i: int| #[trigger] old(self).votes@.contains(i)
                ==> self.votes@.contains(i),
    {
        if self.votes.insert(vote) {
            self.stake = self.stake + committee.stake(vote);
            proof {
                // Spec-level index of the newly inserted vote.
                let x = authority_index_value_spec(vote);
                // Pre-insert voter set, used by the lemma.
                let before = old(self).votes@;
                // Sum grew by exactly c[x], matching how stake grew.
                lemma_sum_insert_full(committee_stake_seq(committee), before, x);
            }
            return true;
        }
        proof {
            // Duplicate vote: voter set unchanged.
            assert(self.votes@ =~= old(self).votes@);
            // And stake unchanged.
            assert(self.stake == old(self).stake);
        }
        false
    }

    pub fn stake(&self) -> (out: Stake)
        // Trivial getter: result equals the field.
        ensures out == self.stake,
    {
        self.stake
    }

    pub fn reached_threshold(&self, committee: &Committee) -> (out: bool)
        // Threshold-reached predicate matches the abstract comparison.
        ensures out == (self.stake >= T::threshold_spec(committee)),
    {
        T::is_threshold(committee, self.stake)
    }

    pub fn threshold(&self, committee: &Committee) -> (out: Stake)
        // Returns the abstract threshold value.
        ensures out == T::threshold_spec(committee),
    {
        T::threshold(committee)
    }

    pub fn clear(&mut self)
        ensures
            // Stake field reset to zero.
            self.stake == 0,
            // Voter set reset to empty.
            self.votes@ =~= Set::<int>::empty(),
            // Invariant holds against any committee, exactly like `new()`.
            forall|c: &Committee| self.invariant_against(c),
    {
        broadcast use lemma_sum_stakes_empty_full;
        self.votes.clear();
        self.stake = 0;
    }
}

/// If an authority has already voted, `add_unique` returns `false` (duplicate).
///
/// The postcondition of `add_unique` is `out == !old(self).has_voted(vote)`.
/// If `has_voted` is already true, that simplifies to `out == false`.
///
/// Usage: after any prior `add` or `add_unique` call, the postcondition
/// guarantees `post.has_voted(vote)`. Supplying that here proves a second
/// `add_unique` call returns `false`.
pub proof fn lemma_voted_authority_add_unique_is_duplicate(
    pre_has_voted: bool,
    out: bool,
)
    requires
        pre_has_voted,
        // Postcondition of add_unique: out == !old(self).has_voted(vote)
        out == !pre_has_voted,
    ensures
        !out  // the call returned false — it was a duplicate
{}

} // verus!

#[cfg(test)]
mod tests {
    use consensus_config::{AuthorityIndex, local_committee_and_keys};

    use super::*;

    #[test]
    fn test_aggregator_quorum_threshold() {
        let committee = local_committee_and_keys(0, vec![1, 1, 1, 1]).0;
        let mut aggregator = StakeAggregator::<QuorumThreshold>::new();

        assert!(!aggregator.add(AuthorityIndex::new_for_test(0), &committee));
        assert!(!aggregator.add(AuthorityIndex::new_for_test(1), &committee));
        assert!(aggregator.add(AuthorityIndex::new_for_test(2), &committee));
        assert!(aggregator.add(AuthorityIndex::new_for_test(3), &committee));
    }

    #[test]
    fn test_add_unique_quorum_threshold() {
        let committee = local_committee_and_keys(0, vec![1, 1, 1, 1]).0;
        let mut aggregator = StakeAggregator::<QuorumThreshold>::new();

        assert!(aggregator.add_unique(AuthorityIndex::new_for_test(0), &committee));
        assert!(!aggregator.reached_threshold(&committee));

        assert!(aggregator.add_unique(AuthorityIndex::new_for_test(1), &committee));
        assert!(!aggregator.reached_threshold(&committee));

        assert!(!aggregator.add_unique(AuthorityIndex::new_for_test(1), &committee));
        assert!(!aggregator.reached_threshold(&committee));

        assert!(aggregator.add_unique(AuthorityIndex::new_for_test(2), &committee));
        assert!(aggregator.reached_threshold(&committee));

        assert!(aggregator.add_unique(AuthorityIndex::new_for_test(3), &committee));
        assert!(aggregator.reached_threshold(&committee));
    }

    #[test]
    fn test_aggregator_validity_threshold() {
        let committee = local_committee_and_keys(0, vec![1, 1, 1, 1]).0;
        let mut aggregator = StakeAggregator::<ValidityThreshold>::new();

        assert!(!aggregator.add(AuthorityIndex::new_for_test(0), &committee));
        assert!(aggregator.add(AuthorityIndex::new_for_test(1), &committee));
    }

    #[test]
    fn test_aggregator_clear() {
        let committee = local_committee_and_keys(0, vec![1, 1, 1, 1]).0;
        let mut aggregator = StakeAggregator::<ValidityThreshold>::new();

        assert!(!aggregator.add(AuthorityIndex::new_for_test(0), &committee));
        assert!(aggregator.add(AuthorityIndex::new_for_test(1), &committee));

        aggregator.clear();

        assert!(!aggregator.add(AuthorityIndex::new_for_test(0), &committee));
        assert!(aggregator.add(AuthorityIndex::new_for_test(1), &committee));
    }
}
