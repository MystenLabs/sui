/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorProcess

namespace Mysticeti

/-! A local safe-resume action for one validator.

The action uses one locked, accepted child block as evidence for a proposal round.
It checks the child's immediate-parent quorum above the local GC boundary. It then
persists one legal proposal before any send action and returns to exact-next mode.

This file does not assume or prove that different validators select the same
child, reach the same round, or form a quorum block layer. Those distributed
convergence results are separate work.
-/

/-- A block with the complete ancestor list used by the Rust block format. The
list can contain the immediate parents and an older own ancestor. -/
structure SafeResumeBlock (BlockId : Type) where
  reference : ValidatorBlockRef BlockId
  ancestors : List (ValidatorBlockRef BlockId)

namespace SafeResumeBlock

/-- The ancestors from the round immediately before this block. -/
def immediateParents {BlockId : Type} (block : SafeResumeBlock BlockId) :
    List (ValidatorBlockRef BlockId) :=
  block.ancestors.filter fun ancestor =>
    ancestor.round + 1 == block.reference.round

/-- The authors represented in the immediate-parent set. -/
def immediateParentAuthors {BlockId : Type}
    (block : SafeResumeBlock BlockId) : VoterSet :=
  fun authority =>
    block.immediateParents.any fun parent => parent.author == authority

/-- The block contains at most one ancestor branch for each author. -/
def ParentAuthorsNodup {BlockId : Type}
    (block : SafeResumeBlock BlockId) : Prop :=
  (block.ancestors.map ValidatorBlockRef.author).Nodup

/-- Every ancestor is from an earlier round. -/
def AncestorsBeforeTarget {BlockId : Type}
    (block : SafeResumeBlock BlockId) : Prop :=
  ∀ ancestor, ancestor ∈ block.ancestors →
    ancestor.round < block.reference.round

/-- The first ancestor belongs to the proposal author. No later ancestor belongs
to that author. This is the current Rust ancestor-order rule. -/
def OwnParentFirst {BlockId : Type} (own : Nat)
    (block : SafeResumeBlock BlockId) : Prop :=
  ∃ ownParent rest,
    block.ancestors = ownParent :: rest ∧
      ownParent.author = own ∧
      ∀ ancestor, ancestor ∈ rest → ancestor.author ≠ own

/-- Local structural and stake checks for one block parent set. -/
structure ValidParentSet {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (block : SafeResumeBlock BlockId) : Prop where
  oneBranchPerAuthor : block.ParentAuthorsNodup
  ancestorsBeforeTarget : block.AncestorsBeforeTarget
  quorumImmediateParents :
    config.thresholds.quorum ≤
      weight config.authorityCount config.stake block.immediateParentAuthors

end SafeResumeBlock

/-- The recovery state names the same absolute target and alignment witness as
the locked child. The child data stays in the local DAG because the recovery
state stores only its reference. -/
def MatchesRecoveryTarget {BlockId CommitId : Type}
    (recovery : ValidatorRecoveryState BlockId CommitId)
    (child : SafeResumeBlock BlockId) : Prop :=
  recovery.targetRound = child.reference.round ∧
    recovery.alignmentWitness = some child.reference

/-- A matching recovery lock fixes the target round. A later observation cannot
change this equality without a separate recovery-state update. -/
theorem matches_recovery_target_round
    {BlockId CommitId : Type}
    {recovery : ValidatorRecoveryState BlockId CommitId}
    {child : SafeResumeBlock BlockId}
    (aligned : MatchesRecoveryTarget recovery child) :
    recovery.targetRound = child.reference.round :=
  aligned.1

/-- A durable signer record. `blockAt` has at most one value for each round, and
every recorded round is at or below `floor`. -/
structure DurableSignerState (BlockId : Type) where
  floor : Nat
  blockAt : Nat → Option (ValidatorBlockRef BlockId)
  recordedRoundLeFloor :
    ∀ round block, blockAt round = some block → round ≤ floor

namespace DurableSignerState

/-- A signer can use only a round above its durable floor. -/
def LegalTarget {BlockId : Type}
    (signer : DurableSignerState BlockId) (target : Nat) : Prop :=
  signer.floor < target

/-- A legal target has no previously recorded block. -/
theorem legal_target_is_unsigned {BlockId : Type}
    (signer : DurableSignerState BlockId) {target : Nat}
    (legal : signer.LegalTarget target) :
    signer.blockAt target = none := by
  cases recorded : signer.blockAt target with
  | none => rfl
  | some block =>
      have covered := signer.recordedRoundLeFloor target block recorded
      unfold LegalTarget at legal
      omega

/-- Record one new block at a legal target. The caller must persist this state
before it sends the block. -/
def record {BlockId : Type}
    (signer : DurableSignerState BlockId)
    (block : ValidatorBlockRef BlockId)
    (legal : signer.LegalTarget block.round) : DurableSignerState BlockId where
  floor := block.round
  blockAt := fun round =>
    if round = block.round then some block else signer.blockAt round
  recordedRoundLeFloor := by
    intro round recordedBlock recorded
    by_cases sameRound : round = block.round
    · omega
    · simp [sameRound] at recorded
      have oldCovered := signer.recordedRoundLeFloor round recordedBlock recorded
      unfold LegalTarget at legal
      omega

@[simp]
theorem record_floor {BlockId : Type}
    (signer : DurableSignerState BlockId)
    (block : ValidatorBlockRef BlockId)
    (legal : signer.LegalTarget block.round) :
    (signer.record block legal).floor = block.round := by
  rfl

@[simp]
theorem record_target {BlockId : Type}
    (signer : DurableSignerState BlockId)
    (block : ValidatorBlockRef BlockId)
    (legal : signer.LegalTarget block.round) :
    (signer.record block legal).blockAt block.round = some block := by
  simp [record]

theorem record_other_round {BlockId : Type}
    (signer : DurableSignerState BlockId)
    (block : ValidatorBlockRef BlockId)
    (legal : signer.LegalTarget block.round)
    {round : Nat} (different : round ≠ block.round) :
    (signer.record block legal).blockAt round = signer.blockAt round := by
  simp [record, different]

/-- Recording a safe-resume block never lowers the signer floor. -/
theorem record_floor_monotone {BlockId : Type}
    (signer : DurableSignerState BlockId)
    (block : ValidatorBlockRef BlockId)
    (legal : signer.LegalTarget block.round) :
    signer.floor ≤ (signer.record block legal).floor := by
  change signer.floor ≤ block.round
  exact Nat.le_of_lt legal

/-- After the record update, the same round cannot be a legal target again. -/
theorem recorded_target_is_not_legal {BlockId : Type}
    (signer : DurableSignerState BlockId)
    (block : ValidatorBlockRef BlockId)
    (legal : signer.LegalTarget block.round) :
    ¬(signer.record block legal).LegalTarget block.round := by
  simp [LegalTarget]

end DurableSignerState

/-- The local proposal mode. Safe resume is a bounded exception to exact-next
proposal. -/
inductive ValidatorProposalMode where
  | exactNext
  | safeResume
  deriving DecidableEq

/-- State used by one validator's safe-resume action. Predicates are local store
facts. They do not state network delivery or quorum behavior. -/
structure ValidatorSafeResumeState (BlockId : Type) where
  signer : DurableSignerState BlockId
  gcRound : Nat
  accepted : ValidatorBlockRef BlockId → Prop
  persisted : ValidatorBlockRef BlockId → Prop
  sent : ValidatorBlockRef BlockId → Prop
  lockedChild : Option (SafeResumeBlock BlockId)
  mode : ValidatorProposalMode

namespace ValidatorSafeResumeState

/-- Every sent block is durable and is in the signer record. -/
def WellFormed {BlockId : Type}
    (state : ValidatorSafeResumeState BlockId) : Prop :=
  (∀ block, state.sent block → state.persisted block) ∧
    (∀ block, state.sent block →
      state.signer.blockAt block.round = some block)

/-- Receiving another block adds one local accepted fact. It does not replace a
locked safe-resume child. -/
def observeAccepted {BlockId : Type}
    (state : ValidatorSafeResumeState BlockId)
    (block : ValidatorBlockRef BlockId) : ValidatorSafeResumeState BlockId :=
  { state with
    accepted := fun reference => reference = block ∨ state.accepted reference }

@[simp]
theorem observeAccepted_preserves_lock {BlockId : Type}
    (state : ValidatorSafeResumeState BlockId)
    (block : ValidatorBlockRef BlockId) :
    (state.observeAccepted block).lockedChild = state.lockedChild := by
  rfl

@[simp]
theorem observeAccepted_preserves_mode {BlockId : Type}
    (state : ValidatorSafeResumeState BlockId)
    (block : ValidatorBlockRef BlockId) :
    (state.observeAccepted block).mode = state.mode := by
  rfl

end ValidatorSafeResumeState

/-- The local guard for a locked safe-resume child.

`gcRound + 1 < child.round` puts the complete immediate-parent round above GC.
The accepted-parent rule refers only to this validator's local DAG. -/
structure LockedAcceptedSafeResumeChild
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (state : ValidatorSafeResumeState BlockId)
    (child : SafeResumeBlock BlockId) : Prop where
  inSafeResume : state.mode = .safeResume
  childIsLocked : state.lockedChild = some child
  childAccepted : state.accepted child.reference
  targetAboveSignerFloor : state.signer.LegalTarget child.reference.round
  immediateParentRoundAboveGc : state.gcRound + 1 < child.reference.round
  validParentSet : child.ValidParentSet config
  immediateParentsAccepted :
    ∀ parent, parent ∈ child.immediateParents → state.accepted parent

namespace LockedAcceptedSafeResumeChild

/-- Every immediate parent of a locked child is above the local GC boundary. -/
theorem immediate_parent_above_gc
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorSafeResumeState BlockId}
    {child : SafeResumeBlock BlockId}
    (locked : LockedAcceptedSafeResumeChild config state child)
    {parent : ValidatorBlockRef BlockId}
    (parentMember : parent ∈ child.immediateParents) :
    state.gcRound < parent.round := by
  have immediate : parent.round + 1 = child.reference.round := by
    have filtered := List.mem_filter.mp parentMember
    simpa using filtered.2
  have above := locked.immediateParentRoundAboveGc
  omega

/-- A locked target does not name a round at or below the signer floor or GC. -/
theorem target_round_is_fresh_and_above_gc
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorSafeResumeState BlockId}
    {child : SafeResumeBlock BlockId}
    (locked : LockedAcceptedSafeResumeChild config state child) :
    state.signer.floor < child.reference.round ∧
      state.gcRound < child.reference.round := by
  exact ⟨locked.targetAboveSignerFloor,
    Nat.lt_trans (Nat.lt_succ_self state.gcRound)
      locked.immediateParentRoundAboveGc⟩

end LockedAcceptedSafeResumeChild

/-- The new proposal uses the locked child's immediate-parent branches. It can
also contain one older own ancestor, as required by the Rust ancestor order. -/
def UsesLockedImmediateParents {BlockId : Type}
    (child proposal : SafeResumeBlock BlockId) : Prop :=
  ∀ parent,
    parent ∈ proposal.immediateParents ↔
      parent ∈ child.immediateParents

/-- Local construction checks for one new safe-resume proposal. -/
structure SafeResumeProposal
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (own : Nat)
    (child proposal : SafeResumeBlock BlockId) : Prop where
  childIsNotOwn : child.reference.author ≠ own
  proposalIsOwn : proposal.reference.author = own
  sameTargetRound : proposal.reference.round = child.reference.round
  ownParentFirst : proposal.OwnParentFirst own
  validParentSet : proposal.ValidParentSet config
  usesLockedImmediateParents : UsesLockedImmediateParents child proposal

/-- The complete local legality result for a safe-resume proposal. -/
structure SafeResumeTargetLegal
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (own : Nat)
    (state : ValidatorSafeResumeState BlockId)
    (proposal : SafeResumeBlock BlockId) : Prop where
  proposalIsOwn : proposal.reference.author = own
  aboveSignerFloor : state.signer.LegalTarget proposal.reference.round
  targetAboveGc : state.gcRound < proposal.reference.round
  ownParentFirst : proposal.OwnParentFirst own
  oneBranchPerAuthor : proposal.ParentAuthorsNodup
  ancestorsBeforeTarget : proposal.AncestorsBeforeTarget
  quorumImmediateParents :
    config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        proposal.immediateParentAuthors
  immediateParentsAboveGc :
    ∀ parent, parent ∈ proposal.immediateParents →
      state.gcRound < parent.round
  immediateParentsAccepted :
    ∀ parent, parent ∈ proposal.immediateParents →
      state.accepted parent

/-- The locked child and local construction checks imply all proposal legality
conditions. No cross-validator fact is an input. -/
theorem safe_resume_target_legal
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {own : Nat}
    {state : ValidatorSafeResumeState BlockId}
    {child proposal : SafeResumeBlock BlockId}
    (locked : LockedAcceptedSafeResumeChild config state child)
    (built : SafeResumeProposal config own child proposal) :
    SafeResumeTargetLegal config own state proposal := by
  have fresh : state.signer.LegalTarget proposal.reference.round := by
    rw [built.sameTargetRound]
    exact locked.targetAboveSignerFloor
  have targetAboveGc : state.gcRound < proposal.reference.round := by
    rw [built.sameTargetRound]
    exact (locked.target_round_is_fresh_and_above_gc).2
  refine {
    proposalIsOwn := built.proposalIsOwn
    aboveSignerFloor := fresh
    targetAboveGc := targetAboveGc
    ownParentFirst := built.ownParentFirst
    oneBranchPerAuthor := built.validParentSet.oneBranchPerAuthor
    ancestorsBeforeTarget := built.validParentSet.ancestorsBeforeTarget
    quorumImmediateParents := built.validParentSet.quorumImmediateParents
    immediateParentsAboveGc := ?_
    immediateParentsAccepted := ?_
  }
  intro parent parentMember
  have childMember := (built.usesLockedImmediateParents parent).mp parentMember
  exact locked.immediate_parent_above_gc childMember
  intro parent parentMember
  have childMember := (built.usesLockedImmediateParents parent).mp parentMember
  exact locked.immediateParentsAccepted parent childMember

/-- Complete one local safe-resume proposal. The proposal becomes durable, no send
occurs in this transition, the lock clears, and exact-next mode resumes. -/
def ValidatorSafeResumeState.completeSafeResume
    {BlockId : Type}
    (state : ValidatorSafeResumeState BlockId)
    (proposal : SafeResumeBlock BlockId)
    (legal : state.signer.LegalTarget proposal.reference.round) :
    ValidatorSafeResumeState BlockId :=
  { signer := state.signer.record proposal.reference legal
    gcRound := state.gcRound
    accepted := state.accepted
    persisted := fun block => block = proposal.reference ∨ state.persisted block
    sent := state.sent
    lockedChild := none
    mode := .exactNext }

namespace ValidatorSafeResumeState

/-- Safe resume records exactly one previously unused target. It cannot sign a
second block in that round through the legal-target rule. -/
theorem completeSafeResume_non_equivocation
    {BlockId : Type}
    (state : ValidatorSafeResumeState BlockId)
    (proposal : SafeResumeBlock BlockId)
    (legal : state.signer.LegalTarget proposal.reference.round) :
    state.signer.blockAt proposal.reference.round = none ∧
      (state.completeSafeResume proposal legal).signer.blockAt
          proposal.reference.round = some proposal.reference ∧
      ¬(state.completeSafeResume proposal legal).signer.LegalTarget
          proposal.reference.round := by
  exact ⟨state.signer.legal_target_is_unsigned legal,
    state.signer.record_target proposal.reference legal,
    state.signer.recorded_target_is_not_legal proposal.reference legal⟩

/-- The complete action cannot lower the durable signer floor. -/
theorem completeSafeResume_floor_monotone
    {BlockId : Type}
    (state : ValidatorSafeResumeState BlockId)
    (proposal : SafeResumeBlock BlockId)
    (legal : state.signer.LegalTarget proposal.reference.round) :
    state.signer.floor ≤
      (state.completeSafeResume proposal legal).signer.floor := by
  exact state.signer.record_floor_monotone proposal.reference legal

/-- The proposal is durable and is not sent by the completion transition. -/
theorem completeSafeResume_persists_before_send
    {BlockId : Type}
    (state : ValidatorSafeResumeState BlockId)
    (proposal : SafeResumeBlock BlockId)
    (legal : state.signer.LegalTarget proposal.reference.round)
    (wellFormed : state.WellFormed) :
    (state.completeSafeResume proposal legal).persisted proposal.reference ∧
      ¬(state.completeSafeResume proposal legal).sent proposal.reference := by
  constructor
  · exact Or.inl rfl
  · intro sent
    have oldRecorded := wellFormed.2 proposal.reference sent
    have unsigned := state.signer.legal_target_is_unsigned legal
    rw [unsigned] at oldRecorded
    contradiction

/-- Completion preserves the rule that every sent block was persisted and
recorded by the signer. -/
theorem completeSafeResume_preserves_well_formed
    {BlockId : Type}
    (state : ValidatorSafeResumeState BlockId)
    (proposal : SafeResumeBlock BlockId)
    (legal : state.signer.LegalTarget proposal.reference.round)
    (wellFormed : state.WellFormed) :
    (state.completeSafeResume proposal legal).WellFormed := by
  constructor
  · intro block sent
    exact Or.inr (wellFormed.1 block sent)
  · intro block sent
    have oldRecorded := wellFormed.2 block sent
    by_cases sameRound : block.round = proposal.reference.round
    · have unsigned := state.signer.legal_target_is_unsigned legal
      rw [sameRound, unsigned] at oldRecorded
      contradiction
    · exact state.signer.record_other_round proposal.reference legal sameRound
        |>.trans oldRecorded

@[simp]
theorem completeSafeResume_returns_to_exact_next
    {BlockId : Type}
    (state : ValidatorSafeResumeState BlockId)
    (proposal : SafeResumeBlock BlockId)
    (legal : state.signer.LegalTarget proposal.reference.round) :
    (state.completeSafeResume proposal legal).mode = .exactNext := by
  rfl

@[simp]
theorem completeSafeResume_clears_lock
    {BlockId : Type}
    (state : ValidatorSafeResumeState BlockId)
    (proposal : SafeResumeBlock BlockId)
    (legal : state.signer.LegalTarget proposal.reference.round) :
    (state.completeSafeResume proposal legal).lockedChild = none := by
  rfl

/-- Exact-next starts one round after the new durable signer floor. -/
theorem exact_next_after_safe_resume
    {BlockId : Type}
    (state : ValidatorSafeResumeState BlockId)
    (proposal : SafeResumeBlock BlockId)
    (legal : state.signer.LegalTarget proposal.reference.round) :
    (state.completeSafeResume proposal legal).signer.floor + 1 =
      proposal.reference.round + 1 := by
  rfl

end ValidatorSafeResumeState

end Mysticeti
