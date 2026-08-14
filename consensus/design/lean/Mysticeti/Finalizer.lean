/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.Thresholds

namespace Mysticeti

/-! Safety of Mysticeti v3 transaction voting and finalization. -/

/-- The facts in one round `R + 1` block for one transaction in a round `R` block.
The Rust mapping is `ASM-SAFE-AUTHENTICATION` and
`ASM-SAFE-EVIDENCE-REFINEMENT`. -/
structure TransactionVote where
  targetAboveCutoff : Bool
  targetReferenced : Bool
  explicitlyRejects : Bool
  deriving DecidableEq, Repr

namespace TransactionVote

/-- This is `PreparedVotingBlock::accepts_transaction`. -/
def accepts (vote : TransactionVote) : Bool :=
  vote.targetAboveCutoff && vote.targetReferenced && !vote.explicitlyRejects

/-- Every next-round block that does not accept is a reject vote. -/
def rejects (vote : TransactionVote) : Bool := !vote.accepts

theorem exactly_one (vote : TransactionVote) :
    (vote.accepts = true ∧ vote.rejects = false) ∨
      (vote.accepts = false ∧ vote.rejects = true) := by
  cases above : vote.targetAboveCutoff <;>
    cases referenced : vote.targetReferenced <;>
    cases rejected : vote.explicitlyRejects <;>
    simp [accepts, rejects, above, referenced, rejected]

theorem old_cutoff_rejects (vote : TransactionVote)
    (notAbove : vote.targetAboveCutoff = false) : vote.rejects = true := by
  simp [rejects, accepts, notAbove]

theorem missing_reference_rejects (vote : TransactionVote)
    (missing : vote.targetReferenced = false) : vote.rejects = true := by
  simp [rejects, accepts, missing]

theorem explicit_reject_rejects (vote : TransactionVote)
    (explicit : vote.explicitlyRejects = true) : vote.rejects = true := by
  simp [rejects, accepts, explicit]

end TransactionVote

/-- Voting evidence for one transaction in one committed block. The Rust mapping is
`ASM-SAFE-EVIDENCE-REFINEMENT`. -/
structure TransactionEvidence (authorityCount : Nat) (stake : Nat → Nat)
    (thresholds : Thresholds authorityCount stake) where
  faulty : VoterSet
  directAcceptVotes : VoterSet
  directRejectVotes : VoterSet
  anchorVotes : VoterSet
  committedAcceptVotes : VoterSet
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
  /-- Correct accept voters in the depth-two anchor quorum occur in the commit prefix.
  `ASM-SAFE-COMMITTED-PREFIX` and `ASM-SAFE-GC`. -/
  correctAcceptAnchorInCommittedCertificate :
    VoterSet.SubsetAt authorityCount
      (VoterSet.diff (VoterSet.inter directAcceptVotes anchorVotes) faulty)
      committedAcceptVotes

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

def IndirectAccept
    (evidence : TransactionEvidence authorityCount stake thresholds) : Prop :=
  thresholds.certificate ≤
    weight authorityCount stake evidence.committedAcceptVotes

/-- The first depth-two trigger rejects only if the common committed prefix has no certificate. -/
def IndirectReject
    (evidence : TransactionEvidence authorityCount stake thresholds) : Prop :=
  thresholds.quorum ≤ weight authorityCount stake evidence.anchorVotes ∧
    weight authorityCount stake evidence.committedAcceptVotes < thresholds.certificate

theorem direct_accept_not_direct_reject
    (evidence : TransactionEvidence authorityCount stake thresholds)
    (accepted : evidence.DirectAccept) (rejected : evidence.DirectReject) : False := by
  exact incompatible_quorums_impossible evidence.faultBounded
    evidence.acceptRejectOverlap accepted rejected

theorem direct_reject_not_indirect_accept
    (evidence : TransactionEvidence authorityCount stake thresholds)
    (rejected : evidence.DirectReject) (accepted : evidence.IndirectAccept) : False := by
  exact incompatible_quorum_certificate_impossible evidence.faultBounded
    evidence.rejectCertificateOverlap rejected accepted

/-- A direct accept quorum leaves an accept certificate in every depth-two anchor quorum. -/
theorem direct_accept_forces_committed_certificate
    (evidence : TransactionEvidence authorityCount stake thresholds)
    (accepted : evidence.DirectAccept)
    (anchorQuorum :
      thresholds.quorum ≤ weight authorityCount stake evidence.anchorVotes) :
    thresholds.certificate ≤
      weight authorityCount stake evidence.committedAcceptVotes := by
  have preserved := quorum_intersection_preserves_certificate
    evidence.faultBounded accepted anchorQuorum
  have included :=
    weight_mono stake evidence.correctAcceptAnchorInCommittedCertificate
  omega

theorem direct_accept_not_indirect_reject
    (evidence : TransactionEvidence authorityCount stake thresholds)
    (accepted : evidence.DirectAccept) (rejected : evidence.IndirectReject) : False := by
  have certificate :=
    evidence.direct_accept_forces_committed_certificate accepted rejected.1
  have noCertificate := rejected.2
  omega

theorem indirect_accept_not_indirect_reject
    (evidence : TransactionEvidence authorityCount stake thresholds)
    (accepted : evidence.IndirectAccept) (rejected : evidence.IndirectReject) : False := by
  unfold IndirectAccept at accepted
  have noCertificate := rejected.2
  omega

inductive Outcome where
  | accept
  | reject
  deriving DecidableEq, Repr

/-- All ways in which `CommitFinalizerV3` can produce an outcome. -/
def CanDecide (evidence : TransactionEvidence authorityCount stake thresholds) :
    Outcome → Prop
  | .accept => evidence.DirectAccept ∨ evidence.IndirectAccept
  | .reject => evidence.DirectReject ∨ evidence.IndirectReject

/-- No valid evidence can accept and reject the same transaction. -/
theorem safety (evidence : TransactionEvidence authorityCount stake thresholds)
    (acceptDecision : evidence.CanDecide .accept)
    (rejectDecision : evidence.CanDecide .reject) : False := by
  rcases acceptDecision with directAccept | indirectAccept
  · rcases rejectDecision with directReject | indirectReject
    · exact evidence.direct_accept_not_direct_reject directAccept directReject
    · exact evidence.direct_accept_not_indirect_reject directAccept indirectReject
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

theorem indirectDecision_accept_iff
    (thresholds : Thresholds authorityCount stake) (acceptStake : Nat) :
    indirectDecision thresholds acceptStake = .accept ↔
      thresholds.certificate ≤ acceptStake := by
  simp [indirectDecision]

theorem indirectDecision_reject_iff
    (thresholds : Thresholds authorityCount stake) (acceptStake : Nat) :
    indirectDecision thresholds acceptStake = .reject ↔
      acceptStake < thresholds.certificate := by
  simp [indirectDecision]

end TransactionEvidence

end Mysticeti
