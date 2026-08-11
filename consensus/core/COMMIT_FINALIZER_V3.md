# Mysticeti v3 transaction finalization

## Scope

`CommitFinalizerV3` finalizes transaction votes for Mysticeti v3. It does not
change transaction validation. It does not change the order of consensus
commits.

The v2 finalizer stays active when `enable_v3` is false.

## Vote meaning

Let `B` be a block at round `R`. A block at round `R + 1` votes on each
transaction in `B`.

The voting block accepts a transaction only when all these conditions are
true:

- The voting block has `B` as an ancestor.
- `R` is greater than the voting block's `transaction_votes_cutoff_round`.
- The voting block does not contain an explicit reject vote for the
  transaction.

If one condition is false, the voting block rejects the transaction. Thus, a
block that does not include `B` rejects the transactions in `B`. A block also
rejects all transactions at or below its signed cutoff round.

An authority can equivocate. Its stake can count once for accept and once for
reject. Its stake cannot count more than once on one side.

## Direct rule

The finalizer reads all cached blocks at round `R + 1` from the local DAG. It
computes accept stake and reject stake for each transaction in `B`.

- Quorum accept stake finalizes the transaction as accepted.
- Quorum reject stake finalizes the transaction as rejected.
- If neither side has quorum stake, the transaction stays pending.

Both sides cannot have quorum stake when the v3 fault limit holds. The code
stops if both sides have quorum stake. This result means that the fault limit
does not hold.

Let `N` be total stake, `q` be quorum stake, `a` be certification stake, and
`f` be malicious stake. The v3 committee checks these inequalities:

- `q + a >= N + f + 1`
- `2q >= N + f + a`

`CommitFinalizerV3` checks both inequalities when it starts. This check
prevents a v2 committee from running the v3 finalization rule.

Thus, two direct quorums overlap by at least `f + a` stake. This overlap is
more than `f`. Up to `f` stake can equivocate, but honest stake cannot vote on
both sides. Thus, both direct decisions cannot exist.

This rule does not assume that round `R + 2` has total certification stake. It
uses the actual next-round blocks in the local DAG.

## Indirect rule

Pending commits stay in commit-index order. A later committed leader is an
anchor for the first pending commit when its round is at least two rounds above
the pending commit leader round.

For each pending transaction, the finalizer finds its round `R + 1` voting
blocks in the buffered committed prefix. This prefix includes the causal
histories of all committed leaders, not only the commit's named leader.

- Certification-threshold accept stake finalizes the transaction as accepted.
- If there is no accept certificate, the transaction is rejected.

The depth-two anchor has quorum stake in its causal history at the voting
round. This fact follows from block verification and does not require a local
DAG traversal. An accept voter directly references `B`. Thus, it cannot commit
before `B`. An accept voter that commits is in the buffered prefix.

The linearizer uses the GC round from the previous commit. If the target block
is far below its commit leader, that leader's causal history commits the
accept-certificate intersection before the new commit can move the GC round
past the voting round. If the target is near the commit leader, the depth-two
anchor commits this intersection while the voting round is still above GC.
This proof depends on the linearizer reading the previous commit's GC round
before it records the current commit. `CommitFinalizerV3` also requires
`gc_depth > 2` to keep one round of margin above the depth-two rule.

The indirect depth is two. This is the same depth as the v3 leader indirect
rule.

The second inequality means that a direct accept quorum and the anchor-history
quorum overlap by at least `f + a` stake. After up to `f` equivocating stake is
removed, this history still has at least `a` accept stake. These accept voters
cannot commit before `B`. The GC argument above places them in the buffered
prefix. Thus, if the buffered prefix has no accept certificate, a direct accept
quorum cannot exist.

The first inequality means that an accept certificate and a direct reject
quorum overlap by more than `f` stake. Thus, they cannot both exist.

## Cutoff production

A v3 proposer reads `DagState::gc_round()` while it links the new causal
history. The transaction vote tracker also returns its GC round while it reads
the explicit votes. The proposer writes the larger value to
`transaction_votes_cutoff_round` in the new `BlockV3`.

The proposer links the complete causal history, but it carries explicit reject
votes only for blocks in the previous round. Other votes cannot contribute to
the one-round v3 rule.

Smart ancestor selection can omit an accepted previous-round block. This
omission is a reject vote under the v3 rule. Thus, transactions from an
excluded or slow authority can be rejected and retried. The v3 finalizer does
not change ancestor selection.

This rule prevents a race between proposal and vote-tracker GC. If the vote
tracker removes votes after causal-history linking, its newer GC round becomes
the signed cutoff. A later GC update cannot change votes that the proposer has
already read.

The cutoff is part of the signed block. The finalizer does not estimate a
remote proposer's GC round.

The block verifier requires `BlockV3` in a v3 epoch, including v3 genesis
blocks. It also requires the signed cutoff to be lower than the block round.
As a defensive rule, `CommitFinalizerV3` treats an old block version as a
reject voter. This rule prevents an old block version from creating an
implicit accept vote.

## Buffer and recovery

The finalizer buffers commits until it can finalize the first commit. It emits
only a complete prefix. It keeps the blocks of pending commits so it can find
indirect vote paths.

The normal commit-recovery path restores unfinalized commits and their blocks.
The v3 finalizer applies the same direct and indirect rules after restart.
Finalized reject maps use the existing durable storage path.

## Test plan

Focused tests cover these cases:

- Direct accept and direct reject from next-round votes.
- Reject votes caused by block omission and by the cutoff.
- The effective cutoff when causal-history GC and vote-tracker GC differ.
- Block-version and signed-cutoff validation.
- The v3 previous-round filter for carried reject votes.
- Unique-authority stake accounting on each side during equivocation.
- A direct decision when one authority votes on both sides.
- Indirect accept with certification stake.
- Indirect accept from the causal histories of several committed leaders.
- Indirect reject without certification stake.
- No indirect decision at depth one and a decision at depth two.
- Production selection of `CommitFinalizerV3` and creation of `BlockV3` with
  the current cutoff.
