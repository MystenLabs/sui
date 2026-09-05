/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.GarbageCollection
import Mysticeti.Thresholds

namespace Mysticeti

/-! Safety of one Mysticeti v3 selected leader slot. -/

/-- Voting evidence for one leader block in one selected leader slot. The mapping from signed
Rust blocks to these sets is `ASM-SAFE-AUTHENTICATION` and
`ASM-SAFE-EVIDENCE-REFINEMENT`. -/
structure LeaderEvidence (authorityCount : Nat) (stake : Nat → Nat)
    (thresholds : Thresholds authorityCount stake) where
  /-- The GC boundary that Core reads before this decision. `ASM-SAFE-GC`. -/
  coreGc : CoreGcState
  /-- The anchor used by the indirect rule. -/
  anchorRound : Nat
  /-- The anchor is at least two rounds above the decision slot.
  `ASM-SAFE-GC`. -/
  anchorDepth :
    coreGc.decisionRound + indirectCommitDepth <= anchorRound
  faulty : VoterSet
  commitVotes : VoterSet
  skipVotes : VoterSet
  anchorVotes : VoterSet
  certificateVotes : VoterSet
  /-- `ASM-SAFE-FAULT-BOUND`. -/
  faultBounded : FaultBounded thresholds faulty
  /-- A correct authority does not both link and omit the same leader block.
  `ASM-SAFE-VOTE-SET-OVERLAP`. -/
  commitSkipOverlap :
    OnlyFaultyOverlap authorityCount faulty commitVotes skipVotes
  /-- A correct skip voter does not occur in a certificate for the same block.
  `ASM-SAFE-VOTE-SET-OVERLAP` and `ASM-SAFE-PARENT-QUORUM`. -/
  skipCertificateOverlap :
    OnlyFaultyOverlap authorityCount faulty skipVotes certificateVotes
  /-- When the decision-to-anchor window is above GC, correct direct voters in
  the anchor quorum occur in the certificate. `ASM-SAFE-GC` and
  `ASM-SAFE-COMMITTED-PREFIX`. -/
  correctCommitAnchorInCertificate :
    coreGc.EvidenceRetained anchorRound →
      VoterSet.SubsetAt authorityCount
        (VoterSet.diff (VoterSet.inter commitVotes anchorVotes) faulty)
        certificateVotes

namespace LeaderEvidence

variable {authorityCount : Nat} {stake : Nat → Nat}
variable {thresholds : Thresholds authorityCount stake}

def DirectCommit (evidence : LeaderEvidence authorityCount stake thresholds) : Prop :=
  thresholds.quorum ≤ weight authorityCount stake evidence.commitVotes

def DirectSkip (evidence : LeaderEvidence authorityCount stake thresholds) : Prop :=
  thresholds.quorum ≤ weight authorityCount stake evidence.skipVotes

def IndirectCommit (evidence : LeaderEvidence authorityCount stake thresholds) : Prop :=
  thresholds.certificate ≤ weight authorityCount stake evidence.certificateVotes

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
  have retained := evidence.coreGc.evidence_retained evidence.anchorDepth
  have included := weight_mono stake
    (evidence.correctCommitAnchorInCertificate retained)
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

end LeaderEvidence

end Mysticeti
