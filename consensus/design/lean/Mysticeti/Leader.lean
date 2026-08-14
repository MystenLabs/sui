/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.Thresholds

namespace Mysticeti

/-! Safety of one Mysticeti v3 leader slot. -/

/-- Voting evidence for one leader block in one leader slot. The mapping from signed
Rust blocks to these sets is `ASM-SAFE-AUTHENTICATION` and
`ASM-SAFE-EVIDENCE-REFINEMENT`. -/
structure LeaderEvidence (authorityCount : Nat) (stake : Nat → Nat)
    (thresholds : Thresholds authorityCount stake) where
  faulty : VoterSet
  commitVotes : VoterSet
  skipVotes : VoterSet
  anchorVotes : VoterSet
  certificateVotes : VoterSet
  /-- `ASM-SAFE-FAULT-BOUND`. -/
  faultBounded : FaultBounded thresholds faulty
  /-- A correct authority does not both link and omit the same leader block.
  `ASM-SAFE-NON-EQUIVOCATION`. -/
  commitSkipOverlap :
    OnlyFaultyOverlap authorityCount faulty commitVotes skipVotes
  /-- A correct skip voter does not occur in a certificate for the same block.
  `ASM-SAFE-NON-EQUIVOCATION`. -/
  skipCertificateOverlap :
    OnlyFaultyOverlap authorityCount faulty skipVotes certificateVotes
  /-- Correct direct voters in an anchor quorum occur in the committed certificate.
  `ASM-SAFE-COMMITTED-PREFIX`. -/
  correctCommitAnchorInCertificate :
    VoterSet.SubsetAt authorityCount
      (VoterSet.diff (VoterSet.inter commitVotes anchorVotes) faulty)
      certificateVotes

namespace LeaderEvidence

variable {authorityCount : Nat} {stake : Nat → Nat}
variable {thresholds : Thresholds authorityCount stake}

def DirectCommit (evidence : LeaderEvidence authorityCount stake thresholds) : Prop :=
  thresholds.quorum ≤
    weight authorityCount stake evidence.commitVotes

def DirectSkip (evidence : LeaderEvidence authorityCount stake thresholds) : Prop :=
  thresholds.quorum ≤
    weight authorityCount stake evidence.skipVotes

def IndirectCommit (evidence : LeaderEvidence authorityCount stake thresholds) : Prop :=
  thresholds.certificate ≤
    weight authorityCount stake evidence.certificateVotes

/-- The indirect rule skips when the first depth-two anchor has no certificate. -/
def IndirectSkip (evidence : LeaderEvidence authorityCount stake thresholds) : Prop :=
  thresholds.quorum ≤ weight authorityCount stake evidence.anchorVotes ∧
    weight authorityCount stake evidence.certificateVotes < thresholds.certificate

theorem direct_commit_not_direct_skip
    (evidence : LeaderEvidence authorityCount stake thresholds)
    (committed : evidence.DirectCommit) (skipped : evidence.DirectSkip) : False := by
  exact incompatible_quorums_impossible evidence.faultBounded
    evidence.commitSkipOverlap committed skipped

theorem direct_skip_not_indirect_commit
    (evidence : LeaderEvidence authorityCount stake thresholds)
    (skipped : evidence.DirectSkip) (committed : evidence.IndirectCommit) : False := by
  exact incompatible_quorum_certificate_impossible evidence.faultBounded
    evidence.skipCertificateOverlap skipped committed

/-- A direct quorum leaves certificate stake in every depth-two anchor quorum. -/
theorem direct_commit_forces_anchor_certificate
    (evidence : LeaderEvidence authorityCount stake thresholds)
    (committed : evidence.DirectCommit)
    (anchorQuorum :
      thresholds.quorum ≤ weight authorityCount stake evidence.anchorVotes) :
    thresholds.certificate ≤
      weight authorityCount stake evidence.certificateVotes := by
  have preserved := quorum_intersection_preserves_certificate
    evidence.faultBounded committed anchorQuorum
  have included := weight_mono stake evidence.correctCommitAnchorInCertificate
  omega

theorem direct_commit_not_indirect_skip
    (evidence : LeaderEvidence authorityCount stake thresholds)
    (committed : evidence.DirectCommit) (skipped : evidence.IndirectSkip) : False := by
  have certificate :=
    evidence.direct_commit_forces_anchor_certificate committed skipped.1
  have noCertificate := skipped.2
  omega

theorem indirect_commit_not_indirect_skip
    (evidence : LeaderEvidence authorityCount stake thresholds)
    (committed : evidence.IndirectCommit) (skipped : evidence.IndirectSkip) : False := by
  unfold IndirectCommit at committed
  have noCertificate := skipped.2
  omega

inductive Outcome where
  | commit
  | skip
  deriving DecidableEq

/-- All ways in which `LeaderSlotDecider` can produce an outcome. -/
def CanDecide (evidence : LeaderEvidence authorityCount stake thresholds) :
    Outcome → Prop
  | .commit => evidence.DirectCommit ∨ evidence.IndirectCommit
  | .skip => evidence.DirectSkip ∨ evidence.IndirectSkip

/-- No valid evidence can decide the same leader block both ways. -/
theorem safety (evidence : LeaderEvidence authorityCount stake thresholds)
    (commitDecision : evidence.CanDecide .commit)
    (skipDecision : evidence.CanDecide .skip) : False := by
  rcases commitDecision with directCommit | indirectCommit
  · rcases skipDecision with directSkip | indirectSkip
    · exact evidence.direct_commit_not_direct_skip directCommit directSkip
    · exact evidence.direct_commit_not_indirect_skip directCommit indirectSkip
  · rcases skipDecision with directSkip | indirectSkip
    · exact evidence.direct_skip_not_indirect_commit directSkip indirectCommit
    · exact evidence.indirect_commit_not_indirect_skip indirectCommit indirectSkip

/-- A direct commit quorum excludes an incompatible certificate. -/
theorem direct_commit_excludes_other_certificate
    (faulty directVotes otherCertificate : VoterSet)
    (faultBounded : FaultBounded thresholds faulty)
    (onlyFaulty :
      OnlyFaultyOverlap authorityCount faulty directVotes otherCertificate)
    (directQuorum :
      thresholds.quorum ≤ weight authorityCount stake directVotes)
    (certificate :
      thresholds.certificate ≤ weight authorityCount stake otherCertificate) : False := by
  exact incompatible_quorum_certificate_impossible faultBounded onlyFaulty
    directQuorum certificate

end LeaderEvidence

end Mysticeti
