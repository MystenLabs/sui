/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.GarbageCollection
import Mysticeti.Thresholds

namespace Mysticeti

/-! Safety of Mysticeti v3 transaction voting and finalization. -/

/-- The signed facts in one round `R + 1` block for one transaction in a round
`R` block. `transactionVoteCutoffRound` maps to the Rust signed
`transaction_votes_cutoff_round` field. The Rust mapping is
`ASM-SAFE-AUTHENTICATION` and `ASM-SAFE-EVIDENCE-REFINEMENT`. -/
structure TransactionVote where
  targetRound : Nat
  votingRound : Nat
  transactionVoteCutoffRound : Nat
  targetReferenced : Bool
  explicitlyRejects : Bool
  deriving DecidableEq, Repr

namespace TransactionVote

/-- The verifier and caller conditions for one v3 next-round vote. -/
def Valid (vote : TransactionVote) : Prop :=
  vote.votingRound = vote.targetRound + 1 ∧
    vote.transactionVoteCutoffRound < vote.votingRound

def targetAboveCutoff (vote : TransactionVote) : Bool :=
  decide (vote.transactionVoteCutoffRound < vote.targetRound)

/-- This is `PreparedVotingBlock::accepts_transaction`. -/
def accepts (vote : TransactionVote) : Bool :=
  vote.targetAboveCutoff && vote.targetReferenced && !vote.explicitlyRejects

/-- Every next-round block that does not accept is a reject vote. -/
def rejects (vote : TransactionVote) : Bool := !vote.accepts

/-- Each modeled transaction vote selects exactly one result. An accept vote and
a reject vote cannot occur together. -/
theorem exactly_one (vote : TransactionVote) :
    (vote.accepts = true ∧ vote.rejects = false) ∨
      (vote.accepts = false ∧ vote.rejects = true) := by
  cases accepted : vote.accepts <;> simp [rejects, accepted]

theorem accepts_implies_above_cutoff (vote : TransactionVote)
    (accepted : vote.accepts = true) :
    vote.transactionVoteCutoffRound < vote.targetRound := by
  have details :
      (vote.transactionVoteCutoffRound < vote.targetRound ∧
        vote.targetReferenced = true) ∧ vote.explicitlyRejects = false := by
    simpa [accepts, targetAboveCutoff] using accepted
  exact details.1.1

theorem cutoff_rejects (vote : TransactionVote)
    (collected : vote.targetRound <= vote.transactionVoteCutoffRound) :
    vote.rejects = true := by
  simp [rejects, accepts, targetAboveCutoff, Nat.not_lt.mpr collected]

/-- A valid next-round vote cannot carry a cleanup cutoff after its target. -/
theorem valid_cutoff_is_before_or_at_target (vote : TransactionVote)
    (valid : vote.Valid) :
    vote.transactionVoteCutoffRound < vote.targetRound ∨
      vote.transactionVoteCutoffRound = vote.targetRound := by
  unfold Valid at valid
  omega

/-- A present next-round block rejects a target that it does not reference. -/
theorem missing_reference_rejects (vote : TransactionVote)
    (missing : vote.targetReferenced = false) : vote.rejects = true := by
  simp [rejects, accepts, missing]

/-- An explicit rejection makes the modeled next-round vote reject. -/
theorem explicit_reject_rejects (vote : TransactionVote)
    (explicit : vote.explicitlyRejects = true) : vote.rejects = true := by
  simp [rejects, accepts, explicit]

end TransactionVote

/-- The two local GC boundaries that produce the signed transaction vote cutoff.
The causal-history boundary covers block GC. The vote-tracker boundary covers
explicit reject-vote GC. -/
structure TransactionVoteProduction where
  vote : TransactionVote
  causalHistoryGcRound : Nat
  voteTrackerGcRound : Nat
  cutoffFromGc :
    vote.transactionVoteCutoffRound =
      max causalHistoryGcRound voteTrackerGcRound

namespace TransactionVoteProduction

/-- A target removed by causal-history block GC is a reject vote. -/
theorem causal_history_gc_rejects (production : TransactionVoteProduction)
    (collected : production.vote.targetRound <= production.causalHistoryGcRound) :
    production.vote.rejects = true := by
  apply production.vote.cutoff_rejects
  rw [production.cutoffFromGc]
  exact Nat.le_trans collected (Nat.le_max_left _ _)

/-- A target removed by vote-tracker GC is a reject vote. -/
theorem vote_tracker_gc_rejects (production : TransactionVoteProduction)
    (collected : production.vote.targetRound <= production.voteTrackerGcRound) :
    production.vote.rejects = true := by
  apply production.vote.cutoff_rejects
  rw [production.cutoffFromGc]
  exact Nat.le_trans collected (Nat.le_max_right _ _)

/-- An accept vote is above both GC boundaries that produced its signed cutoff. -/
theorem accepts_only_above_gc (production : TransactionVoteProduction)
    (accepted : production.vote.accepts = true) :
    production.causalHistoryGcRound < production.vote.targetRound ∧
      production.voteTrackerGcRound < production.vote.targetRound := by
  have aboveCutoff := production.vote.accepts_implies_above_cutoff accepted
  rw [production.cutoffFromGc] at aboveCutoff
  constructor
  · exact Nat.lt_of_le_of_lt (Nat.le_max_left _ _) aboveCutoff
  · exact Nat.lt_of_le_of_lt (Nat.le_max_right _ _) aboveCutoff

end TransactionVoteProduction

/-- Voting evidence for one transaction in one committed block. The Rust mapping is
`ASM-SAFE-EVIDENCE-REFINEMENT`. -/
structure TransactionEvidence (authorityCount : Nat) (stake : Nat → Nat)
    (thresholds : Thresholds authorityCount stake) where
  /-- The two-case v3 sub-DAG GC window for this target. `ASM-SAFE-GC`. -/
  gcWindow : TransactionGcWindow
  /-- Live DAG blocks and buffered committed-prefix blocks are separate stores.
  Later block GC changes only `dag`. `ASM-SAFE-GC`. -/
  blockEvidence : BlockEvidenceStore VoterSet
  faulty : VoterSet
  directAcceptVotes : VoterSet
  directRejectVotes : VoterSet
  anchorVotes : VoterSet
  committedAcceptVotes : VoterSet
  /-- One signed accepting block witness for each direct accept voter. -/
  directAcceptVote : Nat → TransactionVoteProduction
  /-- The same authority's signed block as found in the committed prefix. -/
  committedVote : Nat → TransactionVote
  /-- `ASM-SAFE-FAULT-BOUND`. -/
  faultBounded : FaultBounded thresholds faulty
  /-- A correct authority has one transaction vote. Byzantine equivocation can count
  once on each side. `ASM-SAFE-NON-EQUIVOCATION`. -/
  acceptRejectOverlap :
    OnlyFaultyOverlap authorityCount faulty directAcceptVotes directRejectVotes
  /-- A correct reject voter cannot be in an accept certificate for this transaction.
  `ASM-SAFE-NON-EQUIVOCATION`. -/
  rejectCertificateOverlap :
    OnlyFaultyOverlap authorityCount faulty directRejectVotes committedAcceptVotes
  /-- When the selected sub-DAG boundary retains the voting round, the anchor
  voting blocks occur in the buffered commit prefix. `ASM-SAFE-COMMITTED-PREFIX`
  and `ASM-SAFE-GC`. -/
  anchorInCommittedPrefix :
    ∀ triggerPreviousLeaderRound,
      Retained (gcWindow.evidenceGcRound triggerPreviousLeaderRound)
        gcWindow.votingRound →
        VoterSet.SubsetAt authorityCount anchorVotes
          blockEvidence.committedPrefix
  /-- Every counted direct accept witness is a valid next-round vote for this
  target and passes the signed cutoff classifier.
  `ASM-SAFE-EVIDENCE-REFINEMENT`. -/
  directAcceptVoteEvidence :
    ∀ authority, authority < authorityCount →
      directAcceptVotes authority = true →
        (directAcceptVote authority).vote.targetRound = gcWindow.targetRound ∧
          (directAcceptVote authority).vote.Valid ∧
            (directAcceptVote authority).vote.accepts = true
  /-- A correct authority's signed accepting block is the same block when v3
  sub-DAG construction puts it in the committed prefix.
  `ASM-SAFE-NON-EQUIVOCATION`. -/
  correctAcceptVoteStable :
    ∀ authority, authority < authorityCount → faulty authority = false →
      (directAcceptVote authority).vote = committedVote authority
  /-- The prefix classifier includes each committed block that passes the same
  signed cutoff and explicit-reject rule. `ASM-SAFE-EVIDENCE-REFINEMENT`. -/
  committedAcceptVotesComplete :
    ∀ authority, authority < authorityCount →
      blockEvidence.committedPrefix authority = true →
        (committedVote authority).accepts = true →
          committedAcceptVotes authority = true

namespace TransactionEvidence

variable {authorityCount : Nat} {stake : Nat → Nat}
variable {thresholds : Thresholds authorityCount stake}

def DirectAccept
    (evidence : TransactionEvidence authorityCount stake thresholds) : Prop :=
  thresholds.quorum ≤
    weight authorityCount stake evidence.directAcceptVotes

def DirectReject
    (evidence : TransactionEvidence authorityCount stake thresholds) : Prop :=
  thresholds.quorum ≤
    weight authorityCount stake evidence.directRejectVotes

/-- The preceding commit is still below the first depth-two trigger. -/
def IndirectEvidenceReady
    (evidence : TransactionEvidence authorityCount stake thresholds)
    (triggerPreviousLeaderRound : Nat) : Prop :=
  triggerPreviousLeaderRound <
    evidence.gcWindow.firstCommitLeaderRound + indirectCommitDepth

def IndirectAccept
    (evidence : TransactionEvidence authorityCount stake thresholds) : Prop :=
  thresholds.certificate ≤ weight authorityCount stake evidence.committedAcceptVotes

/-- The first depth-two trigger rejects only if the common committed prefix has no certificate. -/
def IndirectReject
    (evidence : TransactionEvidence authorityCount stake thresholds) : Prop :=
  thresholds.quorum ≤ weight authorityCount stake evidence.anchorVotes ∧
    weight authorityCount stake evidence.committedAcceptVotes < thresholds.certificate

/-- A counted direct accept target is above both proposer-side GC boundaries.
The first boundary is block GC. The second boundary is vote-tracker GC. -/
theorem direct_accept_vote_above_proposer_gc
    (evidence : TransactionEvidence authorityCount stake thresholds)
    {authority : Nat} (inRange : authority < authorityCount)
    (accepted : evidence.directAcceptVotes authority = true) :
    (evidence.directAcceptVote authority).causalHistoryGcRound <
        evidence.gcWindow.targetRound ∧
      (evidence.directAcceptVote authority).voteTrackerGcRound <
        evidence.gcWindow.targetRound := by
  have voteEvidence :=
    evidence.directAcceptVoteEvidence authority inRange accepted
  have aboveGc :=
    (evidence.directAcceptVote authority).accepts_only_above_gc voteEvidence.2.2
  rw [voteEvidence.1] at aboveGc
  exact aboveGc

/-- A correct direct accept witness stays an accept witness after v3 sub-DAG
construction puts its block in the committed prefix. The acceptance test includes
the signed transaction vote cutoff. -/
theorem correct_accept_vote_in_committed_prefix
    (evidence : TransactionEvidence authorityCount stake thresholds) :
    VoterSet.SubsetAt authorityCount
      (VoterSet.diff
        (VoterSet.inter evidence.directAcceptVotes
          evidence.blockEvidence.committedPrefix)
        evidence.faulty)
      evidence.committedAcceptVotes := by
  intro authority inRange included
  have includedParts :
      (evidence.directAcceptVotes authority = true ∧
        evidence.blockEvidence.committedPrefix authority = true) ∧
          evidence.faulty authority = false := by
    simpa [VoterSet.diff, VoterSet.inter] using included
  have voteEvidence := evidence.directAcceptVoteEvidence authority inRange
    includedParts.1.1
  have stable := evidence.correctAcceptVoteStable authority inRange includedParts.2
  have committedAccept : (evidence.committedVote authority).accepts = true := by
    rw [←stable]
    exact voteEvidence.2.2
  exact evidence.committedAcceptVotesComplete authority inRange
    includedParts.1.2 committedAccept

/-- The arithmetic GC theorem and the sub-DAG contract produce the inclusion used
by the direct-accept against indirect-reject proof. -/
theorem correct_accept_anchor_in_committed_certificate
    (evidence : TransactionEvidence authorityCount stake thresholds)
    {triggerPreviousLeaderRound : Nat}
    (ready : evidence.IndirectEvidenceReady triggerPreviousLeaderRound) :
    VoterSet.SubsetAt authorityCount
      (VoterSet.diff
        (VoterSet.inter evidence.directAcceptVotes evidence.anchorVotes)
        evidence.faulty)
      evidence.committedAcceptVotes := by
  have retained := evidence.gcWindow.voting_round_retained ready
  have anchorInPrefix :=
    evidence.anchorInCommittedPrefix triggerPreviousLeaderRound retained
  intro authority inRange included
  have includedParts :
      (evidence.directAcceptVotes authority = true ∧
        evidence.anchorVotes authority = true) ∧
          evidence.faulty authority = false := by
    simpa [VoterSet.diff, VoterSet.inter] using included
  have committed := anchorInPrefix authority inRange includedParts.1.2
  apply evidence.correct_accept_vote_in_committed_prefix authority inRange
  simp [VoterSet.diff, VoterSet.inter, includedParts.1.1, includedParts.2,
    committed]

theorem direct_accept_not_direct_reject
    (evidence : TransactionEvidence authorityCount stake thresholds)
    (accepted : evidence.DirectAccept) (rejected : evidence.DirectReject) : False := by
  exact incompatible_quorums_impossible evidence.faultBounded
    evidence.acceptRejectOverlap accepted rejected

theorem direct_reject_not_indirect_accept
    (evidence : TransactionEvidence authorityCount stake thresholds)
    (rejected : evidence.DirectReject)
    (accepted : evidence.IndirectAccept) : False := by
  exact incompatible_quorum_certificate_impossible evidence.faultBounded
    evidence.rejectCertificateOverlap rejected accepted

/-- A direct accept quorum leaves an accept certificate in every depth-two anchor quorum. -/
theorem direct_accept_forces_committed_certificate
    (evidence : TransactionEvidence authorityCount stake thresholds)
    {triggerPreviousLeaderRound : Nat}
    (accepted : evidence.DirectAccept)
    (ready : evidence.IndirectEvidenceReady triggerPreviousLeaderRound)
    (anchorQuorum :
      thresholds.quorum ≤ weight authorityCount stake evidence.anchorVotes) :
    thresholds.certificate ≤
      weight authorityCount stake evidence.committedAcceptVotes := by
  have preserved := quorum_intersection_preserves_certificate
    evidence.faultBounded accepted anchorQuorum
  have included := weight_mono stake
    (evidence.correct_accept_anchor_in_committed_certificate ready)
  omega

theorem direct_accept_not_indirect_reject
    (evidence : TransactionEvidence authorityCount stake thresholds)
    {triggerPreviousLeaderRound : Nat}
    (ready : evidence.IndirectEvidenceReady triggerPreviousLeaderRound)
    (accepted : evidence.DirectAccept)
    (rejected : evidence.IndirectReject) : False := by
  have certificate :=
    evidence.direct_accept_forces_committed_certificate accepted ready rejected.1
  have noCertificate := rejected.2
  omega

theorem indirect_accept_not_indirect_reject
    (evidence : TransactionEvidence authorityCount stake thresholds)
    (accepted : evidence.IndirectAccept)
    (rejected : evidence.IndirectReject) : False := by
  unfold IndirectAccept at accepted
  have noCertificate := rejected.2
  omega

inductive Outcome where
  | accept
  | reject
  deriving DecidableEq, Repr

/-- All ways in which the modeled v3 transaction finalizer can produce an
outcome. -/
def CanDecide (evidence : TransactionEvidence authorityCount stake thresholds) :
    Outcome → Prop
  | .accept => evidence.DirectAccept ∨ evidence.IndirectAccept
  | .reject => evidence.DirectReject ∨ evidence.IndirectReject

/-- No valid evidence can accept and reject the same transaction. -/
theorem safety (evidence : TransactionEvidence authorityCount stake thresholds)
    {triggerPreviousLeaderRound : Nat}
    (ready : evidence.IndirectEvidenceReady triggerPreviousLeaderRound)
    (acceptDecision : evidence.CanDecide .accept)
    (rejectDecision : evidence.CanDecide .reject) : False := by
  rcases acceptDecision with directAccept | indirectAccept
  · rcases rejectDecision with directReject | indirectReject
    · exact evidence.direct_accept_not_direct_reject directAccept directReject
    · exact evidence.direct_accept_not_indirect_reject ready directAccept indirectReject
  · rcases rejectDecision with directReject | indirectReject
    · exact evidence.direct_reject_not_indirect_accept directReject indirectAccept
    · exact evidence.indirect_accept_not_indirect_reject indirectAccept indirectReject

/-- The executable direct decision rule used in the proof model. -/
def directDecision (thresholds : Thresholds authorityCount stake)
    (acceptStake rejectStake : Nat) : Option Outcome :=
  if thresholds.quorum ≤ acceptStake then
    some .accept
  else if thresholds.quorum ≤ rejectStake then
    some .reject
  else
    none

/-- The executable indirect certificate-or-reject rule used in the proof model. -/
def indirectDecision (thresholds : Thresholds authorityCount stake)
    (acceptStake : Nat) : Outcome :=
  if thresholds.certificate ≤ acceptStake then .accept else .reject

/-- The indirect rule accepts exactly when accept stake reaches the certificate
threshold. -/
theorem indirectDecision_accept_iff
    (thresholds : Thresholds authorityCount stake) (acceptStake : Nat) :
    indirectDecision thresholds acceptStake = .accept ↔
      thresholds.certificate ≤ acceptStake := by
  simp [indirectDecision]

/-- The indirect rule rejects exactly when accept stake is below the certificate
threshold. -/
theorem indirectDecision_reject_iff
    (thresholds : Thresholds authorityCount stake) (acceptStake : Nat) :
    indirectDecision thresholds acceptStake = .reject ↔
      acceptStake < thresholds.certificate := by
  simp [indirectDecision]

end TransactionEvidence

end Mysticeti
