/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Lean.Elab.Tactic.Omega
import Mysticeti.ReachableAnchorFlexAgreement
import Mysticeti.ReferenceFlexIndexedListBridge
import Mysticeti.ValidatorCommitSync
import Mysticeti.ValidatorFlexCommitter

namespace Mysticeti

/-!
Exact installed-prefix safety.

This module does not take a common chain as an input. It separates the source
refinement into local facts:

* every successful local run is an actual `runCommitter` event;
* direct decisions expose authenticated quorum evidence;
* two same-prior pending arrays have the same round and selected-slot shape;
* every local install has one exact same-host run origin;
* every synchronized install has a verified certificate whose correct vote
  carriers come from strictly earlier same-host installs; and
* the durable installed map contains each earlier index.

The exact FlexCommitter theorem then makes one successor functional. A finite
induction from genesis makes every correct installed reference at one index
equal.
-/

/-- Corresponding direct-pass rounds have the same round and selected-slot
shape. Statuses are not compared by this relation. -/
structure CrossViewDirectRoundShape {Digest : Type}
    (left right : ReferenceFlexRoundView Digest) : Prop where
  sameRound : left.round = right.round
  selectedSlots : ExactListAgreement
    (fun leftSlot rightSlot => leftSlot.slot = rightSlot.slot)
    left.selectedSlots right.selectedSlots

/-- One actual successful full FlexCommitter run by a correct, available
validator. `priorAtInput` binds the exact full local head, including its round,
to the snapshot that the action read. -/
structure CorrectExactFlexRun
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    (runtime : LocalFlexCommitterRuntime timed source) where
  observation : LocalFlexCommitterRunObservation BlockId CommitId
  output : LocalFlexCommitOutput BlockId CommitId
  prior : ValidatorCommitHead CommitId
  validatorInRange : observation.validator < config.authorityCount
  validatorCorrect : faults.correctAvailable observation.validator = true
  returned : runtime.returned observation
  successful : observation.result = some output
  priorAtInput : observation.input.commitHead = prior

namespace CorrectExactFlexRun

variable {BlockId CommitId History Encoding PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {functions : CommitReferenceFunctions
  CommitId (LeaderBlockRef BlockId) Encoding}
variable {context : ValidatorFlexContextAt BlockId CommitId History}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {source : LocalFlexCommitterSourceMap config functions context program}
variable {runtime : LocalFlexCommitterRuntime timed source}

/-- The returned result is the exact pure full-scan result for this input. -/
theorem exactResult (run : CorrectExactFlexRun runtime) :
    tryReferenceFlexCommitWithContext functions
        (context run.observation.validator run.observation.input)
        (source.snapshot run.observation.validator run.observation.input) =
      some run.output := by
  rw [← runtime.everyReturnedResultIsExact run.observation run.returned]
  exact run.successful

/-- The observation is an actual main-trace `runCommitter` event. -/
theorem occurs (run : CorrectExactFlexRun runtime) :
    run.observation.OccursIn timed :=
  runtime.everyReturnedObservationOccurs run.observation run.returned

/-- The pure run uses the exact index and digest of its full prior head. -/
theorem priorReferenceMatches (run : CorrectExactFlexRun runtime) :
    let exactPrior :=
      (source.snapshot run.observation.validator run.observation.input).prior
    exactPrior.index = run.prior.index ∧ exactPrior.digest = run.prior.id := by
  have mapped := source.priorMatchesHead run.observation.validator
    run.observation.input
  rw [run.priorAtInput] at mapped
  exact mapped

end CorrectExactFlexRun

private theorem local_flex_commit_output_eq
    {BlockId CommitId : Type}
    {left right : LocalFlexCommitOutput BlockId CommitId}
    (sameCandidate : left.candidate = right.candidate)
    (sameBuilder : left.builderInput = right.builderInput)
    (sameRecord : left.record = right.record)
    (sameReference : left.reference = right.reference) : left = right := by
  cases left
  cases right
  simp_all

/-- Internal adapter from local source facts to the cross-view relation used by
the exact FlexCommitter theorem. No final theorem takes this adapter as an
input. -/
private structure ExactFlexDirectEvidenceSourceMap
    {BlockId CommitId History Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (context : ValidatorFlexContextAt BlockId CommitId History)
    {program : ValidatorExecutionProgram BlockId CommitId}
    (source : LocalFlexCommitterSourceMap config functions context program) where
  commitVotes : Nat → ValidatorLocalState BlockId CommitId →
    LeaderBlockRef BlockId → VoterSet
  skipVotes : Nat → ValidatorLocalState BlockId CommitId →
    LeaderBlockRef BlockId → VoterSet
  anchorOK : LeaderBlockRef BlockId → Prop
  decisionBase : Nat → ValidatorLocalState BlockId CommitId →
    List (ReferenceFlexRoundView BlockId)
  decisionBaseShape : ∀ leftValidator leftState rightValidator rightState,
    leftValidator < config.authorityCount →
    faults.correctAvailable leftValidator = true →
    rightValidator < config.authorityCount →
    faults.correctAvailable rightValidator = true →
    leftState.commitHead = rightState.commitHead →
    ExactPrefixAgreement CrossViewDirectRoundShape
      (decisionBase leftValidator leftState)
      (decisionBase rightValidator rightState)
  firstDecisionReplay : ∀ validator state,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    finishReferenceFlexRoundsAtDepth
        (context validator state).indirectRule
        (context validator state).depth (decisionBase validator state) =
      finishReferenceFlexRoundsAtDepth
        (context validator state).indirectRule
        (context validator state).depth
        (runReferenceDirectPass (context validator state).directRule
          (source.snapshot validator state).pending).toRoundList
  directStatusValid : ∀ validator state,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ∀ round, round ∈ decisionBase validator state →
    ∀ slot, slot ∈ round.selectedSlots →
      ExactDirectStatusValid config.thresholds
        (commitVotes validator state) (skipVotes validator state)
        slot.slot slot.status
  directStatusConsistentOnAdmissible : ∀ validator state,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ∀ round, round ∈ decisionBase validator state →
    ∀ slot, slot ∈ round.selectedSlots →
      (context validator state).indirectRule.ReachableDirectResultConsistent
        anchorOK slot.slot slot.status
  directAnchorsValid : ∀ validator state,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ReferenceFlexRoundsCommittedAnchorsValid anchorOK
      (decisionBase validator state)
  commitAnchorClosed : ∀ validator state,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (context validator state).indirectRule.CommitAnchorClosed anchorOK
  twoViewEvidence : ∀ leftValidator leftState rightValidator rightState,
    leftValidator < config.authorityCount →
    faults.correctAvailable leftValidator = true →
    rightValidator < config.authorityCount →
    faults.correctAvailable rightValidator = true →
    TwoDirectSlotEvidence config.thresholds
      (commitVotes leftValidator leftState) (skipVotes leftValidator leftState)
      (commitVotes rightValidator rightState) (skipVotes rightValidator rightState)

private theorem exact_list_agreement_of_key_maps_equal
    {Left Right Key : Type}
    (leftKey : Left → Key) (rightKey : Right → Key)
    {left : List Left} {right : List Right}
    (sameKeys : left.map leftKey = right.map rightKey) :
    ExactListAgreement (fun leftValue rightValue =>
      leftKey leftValue = rightKey rightValue) left right := by
  induction left generalizing right with
  | nil =>
      cases right <;> simp at sameKeys
      exact .nil
  | cons leftHead leftTail inductionHypothesis =>
      cases right with
      | nil => simp at sameKeys
      | cons rightHead rightTail =>
          simp only [List.map_cons, List.cons.injEq] at sameKeys
          exact .cons sameKeys.1
            (inductionHypothesis sameKeys.2)

/-- The direct pass cannot replace a final status. This is the pure form of the
`try_direct_commit` filter and the first-write rule in
`RoundState::update_slot_decision`. -/
theorem apply_reference_direct_decision_preserves_final
    {Digest : Type} (rule : ReferenceDirectRule Digest)
    {view : ReferenceSelectedSlotView Digest}
    (final : view.status ≠ .undecided) :
    applyReferenceDirectDecision rule view = view := by
  rcases view with ⟨slot, status⟩
  cases status with
  | undecided => exact False.elim (final rfl)
  | commit block => rfl
  | skip => rfl

/-- An indirect pass cannot replace a final status. A later Rust update checks
that a recomputed status is equal, but it keeps the first decision origin. -/
theorem finish_reference_selected_slot_preserves_final
    {Digest History : Type} (rule : ReferenceIndirectRule Digest History)
    (anchor : ReferenceAnchorScanResult Digest)
    {view : ReferenceSelectedSlotView Digest}
    (final : view.status ≠ .undecided) :
    finishReferenceSelectedSlot rule anchor view = view := by
  rcases view with ⟨slot, status⟩
  cases status with
  | undecided => exact False.elim (final rfl)
  | commit block => rfl
  | skip => rfl

private theorem run_reference_direct_round_preserves_slot_keys
    {Digest : Type} (rule : ReferenceDirectRule Digest)
    (round : ReferenceFlexRoundView Digest) :
    (runReferenceDirectRound rule round).selectedSlots.map
        ReferenceSelectedSlotView.slot =
      round.selectedSlots.map ReferenceSelectedSlotView.slot := by
  simp only [runReferenceDirectRound, List.map_map]
  apply List.map_congr_left
  intro slot _member
  rcases slot with ⟨selected, status⟩
  cases status <;> rfl

private theorem selected_slots_committed_anchors_valid_of_membership
    {Digest : Type} {anchorOK : LeaderBlockRef Digest → Prop}
    {slots : List (ReferenceSelectedSlotView Digest)}
    (valid : ∀ slot, slot ∈ slots → ∀ block,
      slot.status = .commit block → anchorOK block) :
    ReferenceSelectedSlotsCommittedAnchorsValid anchorOK slots := by
  induction slots with
  | nil => trivial
  | cons slot tail inductionHypothesis =>
      refine ⟨?_, inductionHypothesis ?_⟩
      · rcases slot with ⟨selected, status⟩
        cases status with
        | undecided => trivial
        | commit block =>
            exact valid
              { slot := selected, status := .commit block } (by simp) block rfl
        | skip => trivial
      · intro tailSlot member block committed
        exact valid tailSlot (by simp [member]) block committed

private theorem indexed_round_shape_prefix
    {Digest : Type}
    (leftRounds rightRounds : Nat → ReferenceFlexRoundView Digest)
    (leftStart rightStart leftFuel rightFuel : Nat)
    (shape : ∀ offset,
      offset < leftFuel → offset < rightFuel →
      CrossViewDirectRoundShape (leftRounds (leftStart + offset))
        (rightRounds (rightStart + offset))) :
    ExactPrefixAgreement CrossViewDirectRoundShape
      (indexedReferenceRoundsFrom leftRounds leftStart leftFuel)
      (indexedReferenceRoundsFrom rightRounds rightStart rightFuel) := by
  induction leftFuel generalizing leftStart rightStart rightFuel with
  | zero => exact .leftNil
  | succ leftRemaining inductionHypothesis =>
      cases rightFuel with
      | zero => exact .rightNil
      | succ rightRemaining =>
          simp only [indexedReferenceRoundsFrom]
          refine .cons (by simpa using shape 0 (by omega) (by omega)) ?_
          apply inductionHypothesis (leftStart := leftStart + 1)
            (rightStart := rightStart + 1)
          intro offset leftInRange rightInRange
          have next := shape (offset + 1) (by omega) (by omega)
          simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using next

/-- Exact provenance for one status after the direct pass.

A direct origin keeps its authenticated direct evidence. An indirect origin
starts from an undecided direct basis and keeps the first ordered anchor scan
that made the slot final. The historical scan input must be a prefix of the
current finished higher rounds. Thus, later appended rounds cannot replace the
first anchor. -/
inductive ExactFlexSlotDecisionProvenance
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake)
    (commitVotes skipVotes : LeaderBlockRef Digest → VoterSet)
    (exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds)
    (anchorOK : LeaderBlockRef Digest → Prop)
    (rule : ReferenceIndirectRule Digest History)
    (minimumRound : Nat)
    (currentFinishedHigher : List (ReferenceFlexRoundView Digest)) :
    ReferenceSelectedSlotView Digest →
      ReferenceSelectedSlotView Digest → Prop where
  | direct {directBasis current} :
      directBasis.slot = current.slot →
      directBasis.status = current.status →
      ExactDirectStatusValid thresholds commitVotes skipVotes
        directBasis.slot directBasis.status →
      directBasis.CommittedAnchorValid anchorOK →
      ExactFlexSlotDecisionProvenance thresholds commitVotes skipVotes
        exactAnchorHistory anchorOK rule minimumRound currentFinishedHigher
        directBasis current
  | indirect {directBasis current anchor orderedHigherPrefix originHistory
      originStatus} :
      directBasis.slot = current.slot →
      directBasis.status = .undecided →
      scanReferenceAnchorAtOrAbove minimumRound orderedHigherPrefix =
        .found anchor →
      (∃ appendedHigher,
        currentFinishedHigher = orderedHigherPrefix ++ appendedHigher) →
      originHistory = rule.historyOf anchor →
      originStatus = rule.decideFromHistory originHistory current.slot →
      current.status = originStatus →
      anchorOK anchor →
      ExactIndirectStatusValid
        (exactAnchorHistory originHistory) current.slot originStatus →
      ExactFlexSlotDecisionProvenance thresholds commitVotes skipVotes
        exactAnchorHistory anchorOK rule minimumRound currentFinishedHigher
        directBasis current

/-- A complete local pending array with sticky first-decision provenance.

`directBasis` is a ghost projection of the Rust array: it retains statuses whose
first origin is direct and changes statuses whose first origin is indirect back
to undecided. Each indirect origin contains its historical first-anchor scan.
The recursive constructor follows the Rust high-to-low indirect pass. -/
inductive ExactFlexFirstDecisionProvenance
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake)
    (commitVotes skipVotes : LeaderBlockRef Digest → VoterSet)
    (exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds)
    (anchorOK : LeaderBlockRef Digest → Prop)
    (rule : ReferenceIndirectRule Digest History)
    (depth : Nat) :
    List (ReferenceFlexRoundView Digest) →
      List (ReferenceFlexRoundView Digest) → Prop where
  | nil : ExactFlexFirstDecisionProvenance thresholds commitVotes skipVotes
      exactAnchorHistory anchorOK rule depth [] []
  | cons {directRound currentRound directTail currentTail} :
      ExactFlexFirstDecisionProvenance thresholds commitVotes skipVotes
        exactAnchorHistory anchorOK rule depth directTail currentTail →
      directRound.round = currentRound.round →
      ExactListAgreement
        (ExactFlexSlotDecisionProvenance thresholds commitVotes skipVotes
          exactAnchorHistory anchorOK rule (currentRound.round + depth)
          (finishReferenceFlexRoundsAtDepth rule depth currentTail))
        directRound.selectedSlots currentRound.selectedSlots →
      ExactFlexFirstDecisionProvenance thresholds commitVotes skipVotes
        exactAnchorHistory anchorOK rule depth
        (directRound :: directTail) (currentRound :: currentTail)

namespace ExactFlexFirstDecisionProvenance

variable {Digest History : Type}
variable {authorityCount : Nat} {stake : Nat → Nat}
variable {thresholds : Thresholds authorityCount stake}
variable {commitVotes skipVotes : LeaderBlockRef Digest → VoterSet}
variable {exactAnchorHistory : History →
  LeaderAnchorHistory Digest authorityCount stake thresholds}
variable {anchorOK : LeaderBlockRef Digest → Prop}
variable {rule : ReferenceIndirectRule Digest History}
variable {depth : Nat}

/-- A successful first-anchor scan stays successful when later rounds are
appended. -/
theorem foundAnchorAppendStable
    {minimumRound : Nat}
    {orderedPrefix appended : List (ReferenceFlexRoundView Digest)}
    {anchor : LeaderBlockRef Digest}
    (found : scanReferenceAnchorAtOrAbove minimumRound orderedPrefix =
      .found anchor) :
    scanReferenceAnchorAtOrAbove minimumRound (orderedPrefix ++ appended) =
      .found anchor := by
  induction orderedPrefix with
  | nil => simp [scanReferenceAnchorAtOrAbove] at found
  | cons round tail ih =>
      by_cases below : round.round < minimumRound
      · simp [scanReferenceAnchorAtOrAbove, below] at found ⊢
        exact ih found
      · cases scanned : scanReferenceSelectedSlots round.selectedSlots with
        | blocked =>
            simp [scanReferenceAnchorAtOrAbove, below, scanned] at found
        | noAnchor =>
            simp [scanReferenceAnchorAtOrAbove, below, scanned] at found ⊢
            exact ih found
        | found block =>
            simpa [scanReferenceAnchorAtOrAbove, below, scanned] using found

private theorem relationDirectValid
    {minimumRound : Nat}
    {higher : List (ReferenceFlexRoundView Digest)}
    {directBasis current : ReferenceSelectedSlotView Digest}
    (provenance : ExactFlexSlotDecisionProvenance thresholds commitVotes
      skipVotes exactAnchorHistory anchorOK rule minimumRound higher
      directBasis current) :
    ExactDirectStatusValid thresholds commitVotes skipVotes
      directBasis.slot directBasis.status := by
  cases provenance with
  | direct _ _ directValid _ => exact directValid
  | indirect _ basisUndecided _ _ _ _ _ _ _ =>
      rw [basisUndecided]
      trivial

private theorem relationAnchorValid
    {minimumRound : Nat}
    {higher : List (ReferenceFlexRoundView Digest)}
    {directBasis current : ReferenceSelectedSlotView Digest}
    (provenance : ExactFlexSlotDecisionProvenance thresholds commitVotes
      skipVotes exactAnchorHistory anchorOK rule minimumRound higher
      directBasis current) :
    directBasis.CommittedAnchorValid anchorOK := by
  cases provenance with
  | direct _ _ _ anchorValid => exact anchorValid
  | indirect _ basisUndecided _ _ _ _ _ _ _ =>
      cases directBasis with
      | mk slot status =>
          simp only at basisUndecided ⊢
          subst status
          trivial

private theorem relationFinishedEqual
    {minimumRound : Nat}
    {higher : List (ReferenceFlexRoundView Digest)}
    {directBasis current : ReferenceSelectedSlotView Digest}
    (provenance : ExactFlexSlotDecisionProvenance thresholds commitVotes
      skipVotes exactAnchorHistory anchorOK rule minimumRound higher
      directBasis current) :
    finishReferenceSelectedSlot rule
        (scanReferenceAnchorAtOrAbove minimumRound higher) directBasis =
      finishReferenceSelectedSlot rule
        (scanReferenceAnchorAtOrAbove minimumRound higher) current := by
  cases provenance with
  | direct sameSlot sameStatus _ _ =>
      have same : directBasis = current := by
        cases directBasis
        cases current
        simp only at sameSlot sameStatus ⊢
        simp_all
      rw [same]
  | indirect sameSlot basisUndecided historicalFound prefixProof historyMatches originDecision currentPersisted anchorValid indirectValid =>
      rename_i anchor orderedHigherPrefix originHistory originStatus
      rcases prefixProof with ⟨appended, higherEq⟩
      have currentFound :
          scanReferenceAnchorAtOrAbove minimumRound higher = .found anchor := by
        rw [higherEq]
        exact foundAnchorAppendStable historicalFound
      have currentDecision : current.status = rule.decide anchor current.slot := by
        rw [currentPersisted, originDecision, historyMatches]
        rfl
      cases directBasis with
      | mk basisSlot basisStatus =>
          cases current with
          | mk currentSlot currentStatus =>
              simp only at sameSlot basisUndecided currentDecision ⊢
              subst currentSlot
              subst basisStatus
              rw [currentDecision]
              simp [finishReferenceSelectedSlot, currentFound,
                rule.decide_final]

private theorem relationsFinishedEqual
    {minimumRound : Nat}
    {higher : List (ReferenceFlexRoundView Digest)}
    {directBasis current : List (ReferenceSelectedSlotView Digest)}
    (provenance : ExactListAgreement
      (ExactFlexSlotDecisionProvenance thresholds commitVotes skipVotes
        exactAnchorHistory anchorOK rule minimumRound higher)
      directBasis current) :
    finishReferenceSelectedSlots rule
        (scanReferenceAnchorAtOrAbove minimumRound higher) directBasis =
      finishReferenceSelectedSlots rule
        (scanReferenceAnchorAtOrAbove minimumRound higher) current := by
  induction provenance with
  | nil => rfl
  | cons headProvenance _ ih =>
      simp only [finishReferenceSelectedSlots, List.map_cons]
      rw [relationFinishedEqual headProvenance]
      apply congrArg (fun tail => _ :: tail)
      simpa [finishReferenceSelectedSlots] using ih

/-- Replaying the current indirect pass from the ghost direct basis gives the
same finished array. Thus, later passes preserve every first final decision. -/
theorem replay
    {directBasis current : List (ReferenceFlexRoundView Digest)}
    (provenance : ExactFlexFirstDecisionProvenance thresholds commitVotes
      skipVotes exactAnchorHistory anchorOK rule depth directBasis current) :
    finishReferenceFlexRoundsAtDepth rule depth directBasis =
      finishReferenceFlexRoundsAtDepth rule depth current := by
  induction provenance with
  | nil => rfl
  | @cons directRound currentRound directTail currentTail tail sameRound slots ih =>
      simp only [finishReferenceFlexRoundsAtDepth,
        finishReferenceFlexRoundAtDepth]
      rw [ih, sameRound]
      rw [relationsFinishedEqual slots]

private theorem relationSlotsShape
    {minimumRound : Nat}
    {higher : List (ReferenceFlexRoundView Digest)}
    {directBasis current : List (ReferenceSelectedSlotView Digest)}
    (provenance : ExactListAgreement
      (ExactFlexSlotDecisionProvenance thresholds commitVotes skipVotes
        exactAnchorHistory anchorOK rule minimumRound higher)
      directBasis current) :
    ExactListAgreement (fun left right => left.slot = right.slot)
      directBasis current := by
  induction provenance with
  | nil => exact .nil
  | cons headProvenance _ ih =>
      cases headProvenance with
      | direct sameSlot _ _ _ => exact .cons sameSlot ih
      | indirect sameSlot _ _ _ _ _ _ _ _ => exact .cons sameSlot ih

/-- The direct basis and current array have the same round and selected-slot
order. Only their status provenance differs. -/
theorem shape
    {directBasis current : List (ReferenceFlexRoundView Digest)}
    (provenance : ExactFlexFirstDecisionProvenance thresholds commitVotes
      skipVotes exactAnchorHistory anchorOK rule depth directBasis current) :
    ExactListAgreement CrossViewDirectRoundShape directBasis current := by
  induction provenance with
  | nil => exact .nil
  | cons tail sameRound slots ih =>
      exact .cons ⟨sameRound, relationSlotsShape slots⟩ ih

private theorem exactListLeftProperty
    {Left Right : Type} {relation : Left → Right → Prop}
    {property : Left → Prop} {left : List Left} {right : List Right}
    (agreement : ExactListAgreement relation left right)
    (relationProperty : ∀ leftValue rightValue,
      relation leftValue rightValue → property leftValue) :
    ∀ leftValue, leftValue ∈ left → property leftValue := by
  induction agreement with
  | nil => simp
  | cons headAgreement _ ih =>
      intro leftValue member
      rcases List.mem_cons.mp member with same | tailMember
      · subst leftValue
        exact relationProperty _ _ headAgreement
      · exact ih leftValue tailMember

/-- Every status in the ghost basis has exact direct evidence. -/
theorem directStatusValid
    {directBasis current : List (ReferenceFlexRoundView Digest)}
    (provenance : ExactFlexFirstDecisionProvenance thresholds commitVotes
      skipVotes exactAnchorHistory anchorOK rule depth directBasis current) :
    ∀ round, round ∈ directBasis → ∀ slot, slot ∈ round.selectedSlots →
      ExactDirectStatusValid thresholds commitVotes skipVotes
        slot.slot slot.status := by
  induction provenance with
  | nil => simp
  | cons tail _ slots ih =>
      intro round roundMember
      rcases List.mem_cons.mp roundMember with same | tailMember
      · subst round
        intro slot slotMember
        exact exactListLeftProperty slots
          (fun _ _ relation => relationDirectValid relation) slot slotMember
      · exact ih round tailMember

/-- Every direct commit in the ghost basis is an admissible anchor. -/
theorem directAnchorsValid
    {directBasis current : List (ReferenceFlexRoundView Digest)}
    (provenance : ExactFlexFirstDecisionProvenance thresholds commitVotes
      skipVotes exactAnchorHistory anchorOK rule depth directBasis current) :
    ReferenceFlexRoundsCommittedAnchorsValid anchorOK directBasis := by
  induction provenance with
  | nil => trivial
  | cons tail _ slots ih =>
      constructor
      · exact selected_slots_committed_anchors_valid_of_membership
          (fun slot member block committed => by
            have valid := exactListLeftProperty slots
              (fun _ _ relation => relationAnchorValid relation) slot member
            simpa [ReferenceSelectedSlotView.CommittedAnchorValid, committed]
              using valid)
      · exact ih

end ExactFlexFirstDecisionProvenance

private theorem exactSlotKeyAgreementToMapEquality
    {Digest : Type}
    {left right : List (ReferenceSelectedSlotView Digest)}
    (agreement : ExactListAgreement (fun leftSlot rightSlot =>
      leftSlot.slot = rightSlot.slot) left right) :
    left.map ReferenceSelectedSlotView.slot =
      right.map ReferenceSelectedSlotView.slot := by
  induction agreement with
  | nil => rfl
  | cons headAgreement _ ih => simp [headAgreement, ih]

private theorem crossViewDirectRoundShapeSymm
    {Digest : Type} {left right : ReferenceFlexRoundView Digest}
    (shape : CrossViewDirectRoundShape left right) :
    CrossViewDirectRoundShape right left := by
  refine ⟨shape.sameRound.symm, ?_⟩
  apply exact_list_agreement_of_key_maps_equal
    ReferenceSelectedSlotView.slot ReferenceSelectedSlotView.slot
  exact (exactSlotKeyAgreementToMapEquality shape.selectedSlots).symm

private theorem crossViewDirectRoundShapeTrans
    {Digest : Type} {left middle right : ReferenceFlexRoundView Digest}
    (leftShape : CrossViewDirectRoundShape left middle)
    (rightShape : CrossViewDirectRoundShape middle right) :
    CrossViewDirectRoundShape left right := by
  refine ⟨leftShape.sameRound.trans rightShape.sameRound, ?_⟩
  apply exact_list_agreement_of_key_maps_equal
    ReferenceSelectedSlotView.slot ReferenceSelectedSlotView.slot
  exact (exactSlotKeyAgreementToMapEquality leftShape.selectedSlots).trans
    (exactSlotKeyAgreementToMapEquality rightShape.selectedSlots)

/-- Exact same-host shape on each side transports the common-prefix shape to
the two ghost direct bases. -/
private theorem decisionBaseShapeFromCurrentShape
    {Digest : Type}
    {leftBase leftCurrent rightBase rightCurrent :
      List (ReferenceFlexRoundView Digest)}
    (leftShape : ExactListAgreement CrossViewDirectRoundShape
      leftBase leftCurrent)
    (currentShape : ExactPrefixAgreement CrossViewDirectRoundShape
      leftCurrent rightCurrent)
    (rightShape : ExactListAgreement CrossViewDirectRoundShape
      rightBase rightCurrent) :
    ExactPrefixAgreement CrossViewDirectRoundShape leftBase rightBase := by
  induction currentShape generalizing leftBase rightBase with
  | leftNil =>
      cases leftShape
      exact .leftNil
  | rightNil =>
      cases rightShape
      exact .rightNil
  | @cons leftCurrentHead rightCurrentHead leftCurrentTail rightCurrentTail
      currentHeadShape currentTailShape ih =>
      cases leftShape with
      | cons leftHeadShape leftTailShape =>
          cases rightShape with
          | cons rightHeadShape rightTailShape =>
              exact .cons
                (crossViewDirectRoundShapeTrans leftHeadShape
                  (crossViewDirectRoundShapeTrans currentHeadShape
                    (crossViewDirectRoundShapeSymm rightHeadShape)))
                (ih leftTailShape rightTailShape)

/-- Fundamental authenticated-vote and one-host decider mappings.

This is the permitted source boundary for direct safety. It maps voter-set
membership to authenticated per-author votes. The two host rules state only
that a non-Byzantine identity does not make incompatible votes. They do not
state that voter-set overlaps, decisions, candidates, or commit heads agree. -/
structure AuthenticatedFlexVoteSourceMap
    {BlockId CommitId History Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (context : ValidatorFlexContextAt BlockId CommitId History)
    {program : ValidatorExecutionProgram BlockId CommitId}
    (source : LocalFlexCommitterSourceMap config functions context program) where
  commitVotes : Nat → ValidatorLocalState BlockId CommitId →
    LeaderBlockRef BlockId → VoterSet
  skipVotes : Nat → ValidatorLocalState BlockId CommitId →
    LeaderBlockRef BlockId → VoterSet
  authenticatedCommitVote : Nat → ValidatorLocalState BlockId CommitId →
    Nat → LeaderBlockRef BlockId → Prop
  authenticatedSkipVote : Nat → ValidatorLocalState BlockId CommitId →
    Nat → LeaderBlockRef BlockId → Prop
  commitVoteMembership : ∀ observer state author block,
    commitVotes observer state block author = true ↔
      authenticatedCommitVote observer state author block
  skipVoteMembership : ∀ observer state author block,
    skipVotes observer state block author = true ↔
      authenticatedSkipVote observer state author block
  /-- Map the implementation's immutable causal history to the exact anchor
  history used by the vote proof. `ASM-SAFE-INDIRECT-ORIGIN`. -/
  exactAnchorHistory : History → LeaderAnchorHistory BlockId
    config.authorityCount config.stake config.thresholds
  /-- Static classification for exact anchors that pass the verifier and have
  the required vote evidence. The closure fields below prove that every anchor
  used by a correct recursive scan is in this class. This field does not say
  that an anchor appears in a future execution, and it does not compare views. -/
  admissibleAnchor : LeaderBlockRef BlockId → Prop
  /-- The first pending round is a deterministic function of the exact commit
  head and epoch configuration. This supports schedules that do not use every
  round. Contiguous rounds and selected-slot order are separate one-host
  source facts. -/
  firstPendingRoundForHead : ValidatorCommitHead CommitId → Nat
  firstPendingRoundMatchesHead : ∀ validator state,
    (source.tryCommitStateValid validator state).firstPendingRound =
      firstPendingRoundForHead state.commitHead
  /-- Ghost projection of one Rust pending array before indirect decisions.
  Direct-origin statuses stay final. Indirect-origin statuses become undecided.
  The projection does not change the live Rust state. -/
  firstDecisionBase : Nat → ValidatorLocalState BlockId CommitId →
    List (ReferenceFlexRoundView BlockId)
  /-- Exact first-write provenance for all current statuses. A cached indirect
  result keeps its historical history, status, anchor, and ordered scan prefix.
  A retained pending array must keep this origin immutable. A schedule reset or
  restart must construct a new pending array and new provenance. -/
  firstDecisionProvenance : ∀ validator state,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ExactFlexFirstDecisionProvenance config.thresholds
      (commitVotes validator state) (skipVotes validator state)
      exactAnchorHistory admissibleAnchor
      (context validator state).indirectRule
      (context validator state).depth
      (firstDecisionBase validator state)
      (runReferenceDirectPass (context validator state).directRule
        (source.snapshot validator state).pending).toRoundList
  /-- An indirect commit from an admissible anchor is also admissible. A later
  descending decision can use that exact committed reference as an anchor. -/
  indirectCommitAnchorIsAdmissible : ∀ validator state,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ∀ anchor, admissibleAnchor anchor →
    ∀ slot block,
      (context validator state).indirectRule.decide anchor slot = .commit block →
      admissibleAnchor block
  /-- Exact vote and causal-closure evidence for each admissible anchor. -/
  admissibleAnchorEvidence : ∀ validator state,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ∀ anchor, admissibleAnchor anchor →
      DirectAnchorEvidence config.thresholds
        (commitVotes validator state) (skipVotes validator state)
        (exactAnchorHistory
          ((context validator state).indirectRule.historyOf anchor))
  /-- The Rust indirect result is the valid exact result of the mapped causal
  history for each admissible anchor. -/
  admissibleIndirectStatusValid : ∀ validator state,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ∀ anchor, admissibleAnchor anchor → ∀ slot,
      ExactIndirectStatusValid
        (exactAnchorHistory
          ((context validator state).indirectRule.historyOf anchor))
        slot ((context validator state).indirectRule.decide anchor slot)
  /-- One non-Byzantine identity cannot authenticate two different commit
  votes for one selected leader slot. -/
  correctCommitVoteIsUnique :
    ∀ (leftObserver : Nat) (leftState : ValidatorLocalState BlockId CommitId)
      (rightObserver : Nat) (rightState : ValidatorLocalState BlockId CommitId)
      (author : Nat) (leftBlock rightBlock : LeaderBlockRef BlockId),
      author < config.authorityCount →
      faults.byzantine author = false →
      leftBlock.SameSelectedSlot rightBlock →
      authenticatedCommitVote leftObserver leftState author leftBlock →
      authenticatedCommitVote rightObserver rightState author rightBlock →
      leftBlock = rightBlock
  /-- One non-Byzantine identity cannot authenticate both commit and skip for
  the same selected leader block. -/
  correctCommitSkipIsExcluded :
    ∀ (commitObserver : Nat)
      (commitState : ValidatorLocalState BlockId CommitId)
      (skipObserver : Nat) (skipState : ValidatorLocalState BlockId CommitId)
      (author : Nat) (block : LeaderBlockRef BlockId),
      author < config.authorityCount →
      faults.byzantine author = false →
      authenticatedCommitVote commitObserver commitState author block →
      authenticatedSkipVote skipObserver skipState author block → False

namespace AuthenticatedFlexVoteSourceMap

variable {BlockId CommitId History Encoding : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {functions : CommitReferenceFunctions
  CommitId (LeaderBlockRef BlockId) Encoding}
variable {context : ValidatorFlexContextAt BlockId CommitId History}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {source : LocalFlexCommitterSourceMap config functions context program}

/-- Per-state pending-array facts and the deterministic selected leader order
derive the common round and slot shape for two same-head snapshots. -/
theorem postDirectShapeFromLocalSchedule
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (leftValidator : Nat) (leftState : ValidatorLocalState BlockId CommitId)
    (rightValidator : Nat) (rightState : ValidatorLocalState BlockId CommitId)
    (_leftValidatorInRange : leftValidator < config.authorityCount)
    (_leftValidatorCorrect : faults.correctAvailable leftValidator = true)
    (_rightValidatorInRange : rightValidator < config.authorityCount)
    (_rightValidatorCorrect : faults.correctAvailable rightValidator = true)
    (sameHead : leftState.commitHead = rightState.commitHead) :
    ExactPrefixAgreement CrossViewDirectRoundShape
      (runReferenceDirectPass (context leftValidator leftState).directRule
        (source.snapshot leftValidator leftState).pending).toRoundList
      (runReferenceDirectPass (context rightValidator rightState).directRule
        (source.snapshot rightValidator rightState).pending).toRoundList := by
  let leftInput := source.snapshot leftValidator leftState
  let rightInput := source.snapshot rightValidator rightState
  let leftDirect := runReferenceDirectPass
    (context leftValidator leftState).directRule leftInput.pending
  let rightDirect := runReferenceDirectPass
    (context rightValidator rightState).directRule rightInput.pending
  apply indexed_round_shape_prefix leftDirect.rounds rightDirect.rounds 0 0
    leftDirect.roundCount rightDirect.roundCount
  intro offset leftInRange rightInRange
  have leftConsecutive :=
    (source.tryCommitStateValid leftValidator leftState).roundsConsecutive
      offset leftInRange
  have rightConsecutive :=
    (source.tryCommitStateValid rightValidator rightState).roundsConsecutive
      offset rightInRange
  have leftFirst := authenticated.firstPendingRoundMatchesHead
    leftValidator leftState
  have rightFirst := authenticated.firstPendingRoundMatchesHead
    rightValidator rightState
  have sameRound : (leftDirect.rounds offset).round =
      (rightDirect.rounds offset).round := by
    change (leftInput.pending.rounds offset).round =
      (rightInput.pending.rounds offset).round
    rw [leftConsecutive, rightConsecutive, leftFirst, rightFirst, sameHead]
  have samePendingRound : (leftInput.pending.rounds offset).round =
      (rightInput.pending.rounds offset).round := by
    simpa [leftDirect, rightDirect, runReferenceDirectPass,
      runReferenceDirectRound] using sameRound
  have leftSelected := source.selectedSlotsMatch leftValidator leftState offset
    leftInRange
  have leftConfigured := source.selectedSlotsMatchConfig leftValidator leftState
    offset leftInRange
  have rightSelected := source.selectedSlotsMatch rightValidator rightState
    offset rightInRange
  have rightConfigured := source.selectedSlotsMatchConfig rightValidator
    rightState offset rightInRange
  have sameSlotKeys : (leftDirect.rounds offset).selectedSlots.map
        ReferenceSelectedSlotView.slot =
      (rightDirect.rounds offset).selectedSlots.map
        ReferenceSelectedSlotView.slot := by
    change (runReferenceDirectRound
          (context leftValidator leftState).directRule
          (leftInput.pending.rounds offset)).selectedSlots.map
          ReferenceSelectedSlotView.slot =
        (runReferenceDirectRound
          (context rightValidator rightState).directRule
          (rightInput.pending.rounds offset)).selectedSlots.map
          ReferenceSelectedSlotView.slot
    rw [run_reference_direct_round_preserves_slot_keys,
      run_reference_direct_round_preserves_slot_keys]
    rw [leftSelected, leftConfigured, rightSelected, rightConfigured]
    rw [sameHead, samePendingRound]
  exact {
    sameRound := by simpa using sameRound
    selectedSlots := exact_list_agreement_of_key_maps_equal
      ReferenceSelectedSlotView.slot ReferenceSelectedSlotView.slot
        (by simpa using sameSlotKeys) }

/-- Authentication, non-equivocation, and the static fault bound derive the
pairwise quorum-evidence adapter used by the exact FlexCommitter theorem. -/
private def toDerivedDirectSafety
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source) :
    ExactFlexDirectEvidenceSourceMap faults functions context source where
  commitVotes := authenticated.commitVotes
  skipVotes := authenticated.skipVotes
  anchorOK := authenticated.admissibleAnchor
  decisionBase := authenticated.firstDecisionBase
  decisionBaseShape := by
    intro leftValidator leftState rightValidator rightState leftInRange leftCorrect
      rightInRange rightCorrect sameHead
    have leftProvenance := authenticated.firstDecisionProvenance
      leftValidator leftState leftInRange leftCorrect
    have rightProvenance := authenticated.firstDecisionProvenance
      rightValidator rightState rightInRange rightCorrect
    exact decisionBaseShapeFromCurrentShape leftProvenance.shape
      (authenticated.postDirectShapeFromLocalSchedule leftValidator leftState
        rightValidator rightState leftInRange leftCorrect rightInRange
        rightCorrect sameHead)
      rightProvenance.shape
  firstDecisionReplay := by
    intro validator state validatorInRange validatorCorrect
    exact (authenticated.firstDecisionProvenance validator state
      validatorInRange validatorCorrect).replay
  directStatusValid := by
    intro validator state validatorInRange validatorCorrect
    exact (authenticated.firstDecisionProvenance validator state
      validatorInRange validatorCorrect).directStatusValid
  directStatusConsistentOnAdmissible := by
    intro validator state validatorInRange validatorCorrect round roundMember
      slot slotMember anchor anchorAdmissible directFinal
    exact
      (authenticated.admissibleAnchorEvidence validator state
          validatorInRange validatorCorrect anchor anchorAdmissible)
        |>.valid_direct_indirect_statuses_equal
          ((authenticated.firstDecisionProvenance validator state
              validatorInRange validatorCorrect).directStatusValid
            round roundMember slot slotMember)
          directFinal
          (authenticated.admissibleIndirectStatusValid validator state
            validatorInRange validatorCorrect anchor anchorAdmissible slot.slot)
  directAnchorsValid := by
    intro validator state validatorInRange validatorCorrect
    exact (authenticated.firstDecisionProvenance validator state
      validatorInRange validatorCorrect).directAnchorsValid
  commitAnchorClosed := by
    intro validator state validatorInRange validatorCorrect anchor slot block
      anchorAdmissible committed
    exact authenticated.indirectCommitAnchorIsAdmissible validator state
      validatorInRange validatorCorrect anchor anchorAdmissible slot block
      committed
  twoViewEvidence := by
    intro leftValidator leftState rightValidator rightState _ _ _ _
    exact {
      faulty := faults.byzantine
      faultBounded := faults.byzantineStakeBounded
      commitCommitOverlap := by
        intro leftBlock rightBlock sameSlot different
        intro author authorInRange overlap
        have both :
            authenticated.commitVotes leftValidator leftState leftBlock author =
                true ∧
              authenticated.commitVotes rightValidator rightState rightBlock
                author = true := by
          simpa [VoterSet.inter] using overlap
        cases faulty : faults.byzantine author with
        | true => rfl
        | false =>
            have sameBlock := authenticated.correctCommitVoteIsUnique
              leftValidator leftState rightValidator rightState author
              leftBlock rightBlock authorInRange faulty sameSlot
              ((authenticated.commitVoteMembership leftValidator leftState
                author leftBlock).mp both.1)
              ((authenticated.commitVoteMembership rightValidator rightState
                author rightBlock).mp both.2)
            exact False.elim (different sameBlock)
      leftCommitRightSkipOverlap := by
        intro block
        intro author authorInRange overlap
        have both :
            authenticated.commitVotes leftValidator leftState block author =
                true ∧
              authenticated.skipVotes rightValidator rightState block author =
                true := by
          simpa [VoterSet.inter] using overlap
        cases faulty : faults.byzantine author with
        | true => rfl
        | false =>
            exact False.elim (authenticated.correctCommitSkipIsExcluded
              leftValidator leftState rightValidator rightState author block
              authorInRange faulty
              ((authenticated.commitVoteMembership leftValidator leftState
                author block).mp both.1)
              ((authenticated.skipVoteMembership rightValidator rightState
                author block).mp both.2))
      rightCommitLeftSkipOverlap := by
        intro block
        intro author authorInRange overlap
        have both :
            authenticated.commitVotes rightValidator rightState block author =
                true ∧
              authenticated.skipVotes leftValidator leftState block author =
                true := by
          simpa [VoterSet.inter] using overlap
        cases faulty : faults.byzantine author with
        | true => rfl
        | false =>
            exact False.elim (authenticated.correctCommitSkipIsExcluded
              rightValidator rightState leftValidator leftState author block
              authorInRange faulty
              ((authenticated.commitVoteMembership rightValidator rightState
                author block).mp both.1)
              ((authenticated.skipVoteMembership leftValidator leftState
                author block).mp both.2)) }

end AuthenticatedFlexVoteSourceMap

private theorem authenticated_direct_slot_lists_agree
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    {leftCommitVotes leftSkipVotes rightCommitVotes rightSkipVotes :
      LeaderBlockRef Digest → VoterSet}
    (evidence : TwoDirectSlotEvidence thresholds leftCommitVotes leftSkipVotes
      rightCommitVotes rightSkipVotes)
    {left right : List (ReferenceSelectedSlotView Digest)}
    (shape : ExactListAgreement
      (fun leftSlot rightSlot => leftSlot.slot = rightSlot.slot) left right)
    (leftValid : ∀ slot, slot ∈ left →
      ExactDirectStatusValid thresholds leftCommitVotes leftSkipVotes
        slot.slot slot.status)
    (rightValid : ∀ slot, slot ∈ right →
      ExactDirectStatusValid thresholds rightCommitVotes rightSkipVotes
        slot.slot slot.status)
    (leftConsistent : ∀ slot, slot ∈ left →
      rule.ReachableDirectResultConsistent anchorOK slot.slot slot.status)
    (rightConsistent : ∀ slot, slot ∈ right →
      rule.ReachableDirectResultConsistent anchorOK slot.slot slot.status) :
    ExactListAgreement (CrossViewReachableDirectSlotAgreement rule anchorOK)
      left right := by
  induction shape with
  | nil => exact .nil
  | @cons leftSlot rightSlot leftTail rightTail sameSlot tailShape ih =>
      have leftHeadValid := leftValid leftSlot (by simp)
      have rightHeadValid := rightValid rightSlot (by simp)
      have rightHeadValidAtLeft : ExactDirectStatusValid thresholds
          rightCommitVotes rightSkipVotes leftSlot.slot rightSlot.status := by
        simpa [sameSlot] using rightHeadValid
      have compatible := evidence.valid_direct_statuses_compatible
        leftHeadValid rightHeadValidAtLeft
      refine .cons {
        exactAgreement := ⟨sameSlot, compatible⟩
        leftConsistent := leftConsistent leftSlot (by simp)
        rightConsistent := rightConsistent rightSlot (by simp) } ?_
      exact ih
        (fun slot member => leftValid slot (by simp [member]))
        (fun slot member => rightValid slot (by simp [member]))
        (fun slot member => leftConsistent slot (by simp [member]))
        (fun slot member => rightConsistent slot (by simp [member]))

private theorem authenticated_direct_round_prefix_agrees
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    (leftRule rightRule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    (sameRule : leftRule = rightRule)
    {leftCommitVotes leftSkipVotes rightCommitVotes rightSkipVotes :
      LeaderBlockRef Digest → VoterSet}
    (evidence : TwoDirectSlotEvidence thresholds leftCommitVotes leftSkipVotes
      rightCommitVotes rightSkipVotes)
    {left right : List (ReferenceFlexRoundView Digest)}
    (shape : ExactPrefixAgreement CrossViewDirectRoundShape left right)
    (leftValid : ∀ round, round ∈ left → ∀ slot,
      slot ∈ round.selectedSlots →
      ExactDirectStatusValid thresholds leftCommitVotes leftSkipVotes
        slot.slot slot.status)
    (rightValid : ∀ round, round ∈ right → ∀ slot,
      slot ∈ round.selectedSlots →
      ExactDirectStatusValid thresholds rightCommitVotes rightSkipVotes
        slot.slot slot.status)
    (leftConsistent : ∀ round, round ∈ left → ∀ slot,
      slot ∈ round.selectedSlots →
      leftRule.ReachableDirectResultConsistent anchorOK slot.slot slot.status)
    (rightConsistent : ∀ round, round ∈ right → ∀ slot,
      slot ∈ round.selectedSlots →
      rightRule.ReachableDirectResultConsistent anchorOK slot.slot slot.status) :
    ExactPrefixAgreement
      (CrossViewReachableDirectRoundAgreement leftRule anchorOK) left right := by
  subst rightRule
  induction shape with
  | leftNil => exact .leftNil
  | rightNil => exact .rightNil
  | @cons leftRound rightRound leftTail rightTail roundShape tailShape ih =>
      have slots := authenticated_direct_slot_lists_agree leftRule anchorOK evidence
        roundShape.selectedSlots
        (fun slot member => leftValid leftRound (by simp) slot member)
        (fun slot member => rightValid rightRound (by simp) slot member)
        (fun slot member => leftConsistent leftRound (by simp) slot member)
        (fun slot member => rightConsistent rightRound (by simp) slot member)
      refine .cons ⟨roundShape.sameRound, slots⟩ ?_
      exact ih
        (fun round member => leftValid round (by simp [member]))
        (fun round member => rightValid round (by simp [member]))
        (fun round member => leftConsistent round (by simp [member]))
        (fun round member => rightConsistent round (by simp [member]))

namespace ExactFlexDirectEvidenceSourceMap

variable {BlockId CommitId History Encoding PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {functions : CommitReferenceFunctions
  CommitId (LeaderBlockRef BlockId) Encoding}
variable {context : ValidatorFlexContextAt BlockId CommitId History}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {source : LocalFlexCommitterSourceMap config functions context program}
variable {runtime : LocalFlexCommitterRuntime timed source}

/-- Authenticated direct evidence and the executable exact-reference theorem
make a successful local successor functional for one exact full prior head. -/
private theorem same_prior_runs_agree
    (safety : ExactFlexDirectEvidenceSourceMap faults functions context source)
    (left right : CorrectExactFlexRun runtime)
    (samePrior : left.prior = right.prior) : left.output = right.output := by
  let leftContext := context left.observation.validator left.observation.input
  let rightContext := context right.observation.validator right.observation.input
  let leftInput := source.snapshot left.observation.validator
    left.observation.input
  let rightInput := source.snapshot right.observation.validator
    right.observation.input
  have sameHeads : left.observation.input.commitHead =
      right.observation.input.commitHead := by
    rw [left.priorAtInput, right.priorAtInput, samePrior]
  have shape := safety.decisionBaseShape left.observation.validator
    left.observation.input right.observation.validator right.observation.input
    left.validatorInRange left.validatorCorrect right.validatorInRange
    right.validatorCorrect sameHeads
  have sameRule : leftContext.indirectRule = rightContext.indirectRule := by
    exact source.contextIndirectRuleStable left.observation.validator
      left.observation.input right.observation.validator right.observation.input
  have directAgreement := authenticated_direct_round_prefix_agrees
    leftContext.indirectRule rightContext.indirectRule safety.anchorOK sameRule
    (safety.twoViewEvidence left.observation.validator left.observation.input
      right.observation.validator right.observation.input left.validatorInRange
      left.validatorCorrect right.validatorInRange right.validatorCorrect)
    shape
    (safety.directStatusValid left.observation.validator left.observation.input
      left.validatorInRange left.validatorCorrect)
    (safety.directStatusValid right.observation.validator right.observation.input
      right.validatorInRange right.validatorCorrect)
    (safety.directStatusConsistentOnAdmissible left.observation.validator
      left.observation.input left.validatorInRange left.validatorCorrect)
    (safety.directStatusConsistentOnAdmissible right.observation.validator
      right.observation.input right.validatorInRange right.validatorCorrect)
  have sameDepth : leftContext.depth = rightContext.depth := by
    exact source.contextDepthStable left.observation.validator
      left.observation.input right.observation.validator right.observation.input
  have sameExactPrior : leftInput.prior = rightInput.prior := by
    have leftMapped := left.priorReferenceMatches
    have rightMapped := right.priorReferenceMatches
    have sameIndex : leftInput.prior.index = rightInput.prior.index :=
      leftMapped.1.trans (samePrior ▸ rightMapped.1.symm)
    have sameDigest : leftInput.prior.digest = rightInput.prior.digest :=
      leftMapped.2.trans (samePrior ▸ rightMapped.2.symm)
    cases leftPriorEq : leftInput.prior with
    | mk leftIndex leftDigest =>
        cases rightPriorEq : rightInput.prior with
        | mk rightIndex rightDigest =>
            simp only [leftPriorEq, rightPriorEq] at sameIndex sameDigest ⊢
            subst rightIndex
            subst rightDigest
            rfl
  have leftStateValid := source.tryCommitStateValid
    left.observation.validator left.observation.input
  have rightStateValid := source.tryCommitStateValid
    right.observation.validator right.observation.input
  have leftNormalized :=
    successful_try_reference_flex_commit_candidate_in_finished_list
      functions leftContext leftInput leftStateValid left.exactResult
  have rightNormalized :=
    successful_try_reference_flex_commit_candidate_in_finished_list
      functions rightContext rightInput rightStateValid right.exactResult
  have leftNormalizedBase :
      findReferenceFlexCommitCandidate
          (finishReferenceFlexRoundsAtDepth leftContext.indirectRule
            leftContext.depth
            (safety.decisionBase left.observation.validator
              left.observation.input)) =
        some left.output.candidate := by
    rw [safety.firstDecisionReplay left.observation.validator
      left.observation.input left.validatorInRange left.validatorCorrect]
    exact leftNormalized
  have rightNormalizedBase :
      findReferenceFlexCommitCandidate
          (finishReferenceFlexRoundsAtDepth rightContext.indirectRule
            rightContext.depth
            (safety.decisionBase right.observation.validator
              right.observation.input)) =
        some right.output.candidate := by
    rw [safety.firstDecisionReplay right.observation.validator
      right.observation.input right.validatorInRange right.validatorCorrect]
    exact rightNormalized
  have rightNormalizedBaseCommon :
      findReferenceFlexCommitCandidate
          (finishReferenceFlexRoundsAtDepth leftContext.indirectRule
            leftContext.depth
            (safety.decisionBase right.observation.validator
              right.observation.input)) =
        some right.output.candidate := by
    rw [sameDepth, sameRule]
    exact rightNormalizedBase
  have sameCandidate :=
    recursive_reachable_flex_commit_candidates_at_depth_prefix_agree
      leftContext.indirectRule safety.anchorOK
      (safety.commitAnchorClosed left.observation.validator
        left.observation.input left.validatorInRange left.validatorCorrect)
      leftContext.depth
      (safety.directAnchorsValid left.observation.validator
        left.observation.input left.validatorInRange left.validatorCorrect)
      (safety.directAnchorsValid right.observation.validator
        right.observation.input right.validatorInRange right.validatorCorrect)
      directAgreement leftNormalizedBase rightNormalizedBaseCommon
  have leftReturned := successful_try_reference_flex_commit_candidate_returned
    functions leftContext leftInput left.exactResult
  have rightReturned := successful_try_reference_flex_commit_candidate_returned
    functions rightContext rightInput right.exactResult
  have leftMaterialValid := source.tryCommitMaterialValid
    left.observation.validator left.observation.input
  have rightMaterialValid := source.tryCommitMaterialValid
    right.observation.validator right.observation.input
  have leftComplete := leftMaterialValid.completeForReturned
    left.output.candidate leftReturned
  have rightComplete := rightMaterialValid.completeForReturned
    right.output.candidate rightReturned
  have sameMappedInput := source.materializer.complete_local_inputs_agree
    sameExactPrior sameCandidate leftComplete rightComplete
  have leftExact := successful_try_reference_flex_commit_exact_output
    functions leftContext leftInput left.exactResult
  have rightExact := successful_try_reference_flex_commit_exact_output
    functions rightContext rightInput right.exactResult
  have leftBuilder : left.output.builderInput =
      source.materializer.localInput left.observation.input leftInput.prior
        left.output.candidate := by
    rw [leftExact.1, leftMaterialValid.materialMatches]
    rfl
  have rightBuilder : right.output.builderInput =
      source.materializer.localInput right.observation.input rightInput.prior
        right.output.candidate := by
    rw [rightExact.1, rightMaterialValid.materialMatches]
    rfl
  have sameBuilder : left.output.builderInput = right.output.builderInput := by
    rw [leftBuilder, rightBuilder]
    exact sameMappedInput
  have sameRecord : left.output.record = right.output.record := by
    rw [leftExact.2.1, rightExact.2.1, sameBuilder]
  have sameReference : left.output.reference = right.output.reference := by
    rw [leftExact.2.2, rightExact.2.2, sameRecord]
  exact local_flex_commit_output_eq sameCandidate sameBuilder sameRecord
    sameReference

end ExactFlexDirectEvidenceSourceMap

/-- Two actual successful correct local FlexCommitter runs from one exact full
head return the same complete output. The result includes the candidate,
builder input, commit body, and exact commit reference. It uses no commit-sync
action, commit-vote certificate, or installed common-chain premise. -/
theorem correct_local_flex_runs_same_prior_exact_output
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (left right : CorrectExactFlexRun runtime)
    (samePrior : left.prior = right.prior) : left.output = right.output := by
  exact authenticated.toDerivedDirectSafety.same_prior_runs_agree left right
    samePrior

/-- If a later correct local run still starts from the earlier run's exact
prior head, it cannot select a different successor or skip that successor.
The time premise only identifies which observed run is later. -/
theorem later_correct_local_flex_run_cannot_skip_exact_successor
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (earlier later : CorrectExactFlexRun runtime)
    (_earlierBeforeLater : earlier.observation.time ≤ later.observation.time)
    (samePrior : earlier.prior = later.prior) :
    later.output.toCommitHead = earlier.output.toCommitHead := by
  have sameOutput := correct_local_flex_runs_same_prior_exact_output
    authenticated earlier later samePrior
  rw [sameOutput]

/-- One exact commit head is the successful full FlexCommitter successor of
another exact full head. -/
def ExactFlexSuccessor
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    (runtime : LocalFlexCommitterRuntime timed source)
    (prior next : ValidatorCommitHead CommitId) : Prop :=
  ∃ run : CorrectExactFlexRun runtime,
    run.prior = prior ∧ run.output.toCommitHead = next

namespace ExactFlexSuccessor

variable {BlockId CommitId History Encoding PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {functions : CommitReferenceFunctions
  CommitId (LeaderBlockRef BlockId) Encoding}
variable {context : ValidatorFlexContextAt BlockId CommitId History}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {source : LocalFlexCommitterSourceMap config functions context program}
variable {runtime : LocalFlexCommitterRuntime timed source}

/-- Every successful full exact run returns the next commit index. -/
private theorem successful_full_flex_run_uses_next_index
    (run : CorrectExactFlexRun runtime) :
    run.output.reference.index = run.prior.index + 1 := by
  have found := run.exactResult
  have mapped := run.priorReferenceMatches
  unfold tryReferenceFlexCommitWithContext tryReferenceFlexCommit at found
  let directState := runReferenceDirectPass
    (context run.observation.validator run.observation.input).directRule
    (source.snapshot run.observation.validator run.observation.input).pending
  cases directFound : findIndexedReferenceFlexCandidate directState with
  | some candidate =>
      simp only [directState, directFound] at found
      rw [← Option.some.inj found]
      change
        (source.snapshot run.observation.validator
          run.observation.input).prior.index + 1 = run.prior.index + 1
      rw [mapped.1]
  | none =>
      cases indirectFound : findIndexedReferenceFlexCandidate
          (runFullIndexedReferenceIndirect
            (context run.observation.validator run.observation.input).depth
            (context run.observation.validator run.observation.input).indirectRule
            directState) with
      | none => simp [directState, directFound, indirectFound] at found
      | some candidate =>
          simp only [directState, directFound, indirectFound] at found
          rw [← Option.some.inj found]
          change
            (source.snapshot run.observation.validator
              run.observation.input).prior.index + 1 = run.prior.index + 1
          rw [mapped.1]

/-- One exact local successor always advances the index by one. -/
theorem nextIndex {prior next : ValidatorCommitHead CommitId}
    (successor : ExactFlexSuccessor runtime prior next) :
    next.index = prior.index + 1 := by
  rcases successor with ⟨run, runPrior, runOutput⟩
  rw [← runOutput]
  change run.output.reference.index = prior.index + 1
  rw [successful_full_flex_run_uses_next_index run, runPrior]

/-- One successful correct local run advances its exact prior by one index. It
cannot jump over an index. -/
theorem correctRunAdvancesOneIndex (run : CorrectExactFlexRun runtime) :
    run.output.toCommitHead.index = run.prior.index + 1 := by
  exact nextIndex ⟨run, rfl, rfl⟩

/-- Authenticated exact FlexCommitter safety makes one successor unique. -/
theorem unique
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    {prior left right : ValidatorCommitHead CommitId}
    (leftSuccessor : ExactFlexSuccessor runtime prior left)
    (rightSuccessor : ExactFlexSuccessor runtime prior right) : left = right := by
  rcases leftSuccessor with ⟨leftRun, leftPrior, leftOutput⟩
  rcases rightSuccessor with ⟨rightRun, rightPrior, rightOutput⟩
  have outputs := correct_local_flex_runs_same_prior_exact_output
    authenticated leftRun rightRun
    (leftPrior.trans rightPrior.symm)
  rw [← leftOutput, ← rightOutput, outputs]

end ExactFlexSuccessor

/-- A finite exact commit path from genesis. The path length is the commit
index when genesis has index zero. -/
inductive ExactCommitPath {Reference : Type}
    (successor : Reference → Reference → Prop)
    (genesis : Reference) : Nat → Reference → Prop where
  | genesis : ExactCommitPath successor genesis 0 genesis
  | next {length : Nat} {prior reference : Reference} :
      ExactCommitPath successor genesis length prior →
      successor prior reference →
      ExactCommitPath successor genesis (length + 1) reference

/-- A functional successor gives one exact reference at each finite path
length. -/
theorem exact_commit_path_unique
    {Reference : Type}
    {successor : Reference → Reference → Prop}
    {genesis : Reference}
    (successorUnique : ∀ prior left right,
      successor prior left → successor prior right → left = right)
    {length : Nat} {left right : Reference}
    (leftPath : ExactCommitPath successor genesis length left)
    (rightPath : ExactCommitPath successor genesis length right) : left = right := by
  induction leftPath generalizing right with
  | genesis =>
      cases rightPath
      rfl
  | @next length leftPrior leftReference leftPriorPath leftStep ih =>
      cases rightPath with
      | next rightPriorPath rightStep =>
          have samePrior := ih rightPriorPath
          subst_vars
          exact successorUnique _ _ _ leftStep rightStep

/-- One-validator durable full-head lookup.

The main validator state stores only `(index, digest)`. Rust also retains the
commit body, whose named leader supplies the round. `exactInstalledHead` is the
source mapping for that body. It states no cross-validator equality.

The store fields are `ASM-SAFE-COMMIT-STORE`. `validBodyDigestBinding` is
`ASM-SAFE-DIGEST-IDENTITY`. -/
structure ExactCommitDurablePrefixSourceMap
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (genesis : ValidatorCommitHead CommitId) where
  exactInstalledHead : Time → Nat → ValidatorCommitHead CommitId → Prop
  /-- A valid commit body has one full head for one signed `(index, digest)`.
  This is the source and cryptographic binding needed because a commit vote
  does not sign the named leader round as a separate field. Rust binds it
  anyway: `CommitV1.leader` sits inside the body that the commit digest
  hashes. -/
  validCommitBody : ValidatorCommitHead CommitId → Prop
  validBodyDigestBinding : ∀ left right,
    validCommitBody left → validCommitBody right →
    left.index = right.index → left.id = right.id → left = right
  exactInstalledHeadIsValid : ∀ time validator reference,
    exactInstalledHead time validator reference → validCommitBody reference
  exactHeadHasStoredId : ∀ time validator reference,
    exactInstalledHead time validator reference →
    ((trace time).validatorState validator).installedCommitAt reference.index =
      some reference.id
  storedIdHasExactHead : ∀ time validator index commitId,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((trace time).validatorState validator).installedCommitAt index =
        some commitId →
    ∃ reference,
      exactInstalledHead time validator reference ∧
        reference.index = index ∧ reference.id = commitId
  zeroHeadIsGenesis : ∀ time validator reference,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    exactInstalledHead time validator reference →
    reference.index = 0 → reference = genesis
  installedAtOrBelowHead : ∀ time validator index,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    index ≤ (trace time).localCommitIndex validator →
    ∃ commitId,
      ((trace time).validatorState validator).installedCommitAt index =
        some commitId
  /-- The durable prefix of one host is itself a digest chain. Each installed
  head above genesis names the identifier of the head one index below it. Rust
  writes `CommitV1.previous_digest` into every stored commit body, so this is a
  one-host storage rule and not a cross-host claim. -/
  installedHeadLinksToPredecessor : ∀ time validator reference,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    exactInstalledHead time validator reference →
    0 < reference.index →
    ∃ predecessor,
      exactInstalledHead time validator predecessor ∧
        predecessor.index = reference.index - 1 ∧
        reference.previousId = some predecessor.id

namespace ExactCommitDurablePrefixSourceMap

/-- Walking down a checked digest chain from an installed tip stays inside the
host's own durable prefix.

This was an assumed field of `ExactCommitInstallProvenance`. It is now derived
from one-host facts only. The chain links each entry to the one before it, and
one index with one identifier names one body: both are
`ASM-SAFE-DIGEST-IDENTITY`. The host's own prefix links the same way and keeps
the predecessor: that is `ASM-SAFE-COMMIT-STORE`. No step compares two hosts. -/
theorem digest_chain_entry_matches_installed_prefix
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace (ValidatorWorldState BlockId CommitId PacketId)}
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults trace genesis)
    {time author : Nat}
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true) :
    ∀ commits : List (CommonCommitRef CommitId),
      ConsecutiveCommitIndices commits →
      DigestLinkedCommits durable.validCommitBody commits →
      ∀ tip, commits.getLast? = some tip →
      ∀ tipHead, durable.exactInstalledHead time author tipHead →
      tipHead.index = tip.index →
      tipHead.id = tip.id →
      ∀ entry ∈ commits,
        ∃ installedEntry,
          durable.exactInstalledHead time author installedEntry ∧
            installedEntry.index = entry.index ∧
            installedEntry.id = entry.id := by
  intro commits
  induction commits with
  | nil =>
    intro _ _ tip lastTip
    simp at lastTip
  | cons first rest ih =>
    cases rest with
    | nil =>
      intro _ _ tip lastTip tipHead installed indexEq idEq entry member
      simp at lastTip member
      subst lastTip
      subst member
      exact ⟨tipHead, installed, indexEq, idEq⟩
    | cons second rest' =>
      intro consecutive linked tip lastTip tipHead installed indexEq idEq entry
        member
      simp only [ConsecutiveCommitIndices] at consecutive
      simp only [DigestLinkedCommits] at linked
      have tailLast : (second :: rest').getLast? = some tip := by
        simpa using lastTip
      have tailInstalled := ih consecutive.2 linked.2.2 tip tailLast tipHead
        installed indexEq idEq
      rcases List.mem_cons.mp member with entryIsFirst | entryInTail
      · subst entryIsFirst
        rcases tailInstalled second List.mem_cons_self with
          ⟨secondHead, secondInstalled, secondIndex, secondId⟩
        have secondHeadIsSecond : secondHead = second :=
          durable.validBodyDigestBinding secondHead second
            (durable.exactInstalledHeadIsValid time author secondHead
              secondInstalled)
            linked.2.2.head_valid secondIndex secondId
        have secondIndexStep : second.index = entry.index + 1 := consecutive.1
        have secondHeadPositive : 0 < secondHead.index := by
          rw [secondIndex, secondIndexStep]
          exact Nat.succ_pos _
        rcases durable.installedHeadLinksToPredecessor time author secondHead
            authorInRange authorCorrect secondInstalled secondHeadPositive with
          ⟨predecessor, predecessorInstalled, predecessorIndex, linkToPredecessor⟩
        have linkIsEntry : secondHead.previousId = some entry.id := by
          rw [secondHeadIsSecond]
          exact linked.2.1
        have predecessorId : predecessor.id = entry.id :=
          Option.some.inj (linkToPredecessor.symm.trans linkIsEntry)
        have predecessorIsEntryIndex : predecessor.index = entry.index := by
          rw [predecessorIndex, secondIndex, secondIndexStep]
          omega
        exact ⟨predecessor, predecessorInstalled, predecessorIsEntryIndex,
          predecessorId⟩
      · exact tailInstalled entry entryInTail

end ExactCommitDurablePrefixSourceMap

/-- A quorum voter set contains one correct, available validator.

The result follows from the static threshold rules and the Byzantine plus
unavailable stake bounds. It is not a certificate-provenance assumption. -/
private theorem quorum_set_has_correct_available_member
    {CommitId : Type} {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config) (voters : VoterSet)
    (quorum : config.thresholds.quorum ≤
      weight config.authorityCount config.stake voters) :
    ∃ author, author < config.authorityCount ∧
      faults.correctAvailable author = true ∧ voters author = true := by
  have quorumAboveNonProgress :
      config.thresholds.fault + faults.unavailableStakeBound <
        config.thresholds.quorum := by
    have thresholdRule :=
      config.thresholds.quorum_preserves_certificate
    have certificatePositive := config.thresholds.certificate_positive
    have budgetsFit := faults.faultBudgetsFit
    have quorumDefinition := faults.quorumDefinition
    omega
  have nonProgressBound := faults.non_progress_stake_bounded
  have intersectionBound :
      weight config.authorityCount config.stake
          (VoterSet.inter voters faults.nonProgress) ≤
        weight config.authorityCount config.stake faults.nonProgress :=
    weight_mono config.stake
      (VoterSet.inter_subset_right config.authorityCount voters
        faults.nonProgress)
  have partition := weight_diff_add_inter config.authorityCount config.stake
    voters faults.nonProgress
  have positiveCorrectStake : 0 <
      weight config.authorityCount config.stake
        (VoterSet.diff voters faults.nonProgress) := by
    omega
  rcases positive_weight_has_member positiveCorrectStake with
    ⟨author, authorInRange, selected, _positiveStake⟩
  have selectedAndCorrect : voters author = true ∧
      faults.nonProgress author = false := by
    simpa [VoterSet.diff] using selected
  have correctAvailable : faults.correctAvailable author = true := by
    simp [FixedFaultInterval.correctAvailable, VoterSet.diff, VoterSet.full,
      selectedAndCorrect.2]
  exact ⟨author, authorInRange, correctAvailable, selectedAndCorrect.1⟩

/-- Derived provenance for one positive installed full head.

A local install supplies this origin directly. The sync proof obtains it by
following strictly earlier carrier installs. -/
structure CorrectInstalledExactOrigin
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    (runtime : LocalFlexCommitterRuntime timed source)
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults timed.execution.trace
      genesis)
    (reference : ValidatorCommitHead CommitId) where
  run : CorrectExactFlexRun runtime
  priorInstalled : durable.exactInstalledHead run.observation.time
    run.observation.validator run.prior
  runOutput : run.output.toCommitHead = reference
  recordTime : Time
  installTime : Time
  runBeforeRecord : run.observation.time < recordTime
  recordVisibleByInstall : recordTime + 1 ≤ installTime
  recordOccurs : ValidatorLocalActionOccurs
    (timed.execution.events recordTime) run.observation.validator
      (.recordCommit reference)
  installed : durable.exactInstalledHead installTime
    run.observation.validator reference

/-- Same-host provenance for a local durable install. The run, record action,
and visible installed entry occur in that order. -/
structure LocalInstalledExactOrigin
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    (runtime : LocalFlexCommitterRuntime timed source)
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults timed.execution.trace
      genesis)
    (installTime installer : Nat)
    (reference : ValidatorCommitHead CommitId) where
  origin : CorrectInstalledExactOrigin runtime durable reference
  sameInstaller : origin.run.observation.validator = installer
  sameInstallTime : origin.installTime = installTime

/-- The actual same-host persistence event for one authenticated commit-vote
carrier. Bundle verification supplies the signed carrier. This witness only
maps its exact block reference and author to the main execution trace. -/
structure AuthenticatedCommitVoteCarrierOrigin
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (blockCarriesCommitVote : ValidatorBlock BlockId →
      CommonCommitRef CommitId → Prop)
    (carrier : CommitVoteCarrier BlockId CommitId) where
  time : Time
  block : ValidatorBlock BlockId
  blockReference : block.reference = carrier.block
  carrierAuthor : carrier.block.author = carrier.author
  carriesAuthenticatedVote : blockCarriesCommitVote block carrier.reference
  persistenceOccurs : ValidatorLocalActionOccurs
    (timed.execution.events time) carrier.author (.persistProposal block)

/-- Primitive verified-sync provenance for one durable install.

This structure does not select a successful FlexCommitter run or an earlier
install. It maps each verified carrier to its earlier persistence event. A
separate single-host rule says that a correct host persists such a carrier only
after it installs the signed `(index, digest)`. -/
structure VerifiedSyncInstalledExactOrigin
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults timed.execution.trace
      genesis)
    (validChain : Nat → List (CommonCommitRef CommitId) → Prop)
    (validBlocks : CommitSyncBundle BlockId CommitId → Prop)
    (blockCarriesCommitVote : ValidatorBlock BlockId →
      CommonCommitRef CommitId → Prop)
    (installTime installer : Nat)
    (reference : ValidatorCommitHead CommitId) where
  afterIndex : Nat
  tip : ValidatorCommitHead CommitId
  bundle : CommitSyncBundle BlockId CommitId
  bundleVerified : ExactCertifiedCommitBundle config.thresholds validChain
    validBlocks afterIndex tip bundle
  /-- The action can install any entry of the verified range. Only `tip` has
  the quorum certificate in this bundle. -/
  referenceInBundle : reference ∈ bundle.commits
  syncTime : Time
  syncVisibleByInstall : syncTime + 1 ≤ installTime
  syncOccurs : ValidatorLocalActionOccurs
    (timed.execution.events syncTime) installer (.applySyncedCommit reference)
  /-- Every carrier used by the verified bundle has an earlier authenticated
  persistence event in the main execution. -/
  certifiedCarrierHasPersistenceOrigin : ∀ author carrier,
    author < config.authorityCount →
    faults.correctAvailable author = true →
    carrier ∈ bundle.certifyingBlocks →
    carrier.author = author →
    ∃ origin : AuthenticatedCommitVoteCarrierOrigin timed
        blockCarriesCommitVote carrier,
      origin.time < syncTime

/-- Local provenance rules for actual positive installs.

The local field is the reverse link from `recordCommit` to its protected exact
run result. The sync field exposes the verified bundle and actual carrier
persistence origins. One field is a single-host ordering rule. One field says
what the synchronization chain check means. The theorem below derives the local
run for a synchronized reference. Neither field states that two installed
references are equal.

The three origin and ordering fields are `ASM-SAFE-INSTALL-PROVENANCE`.
`validChainIsDigestChain` is `ASM-SAFE-DIGEST-IDENTITY`. -/
structure ExactCommitInstallProvenance
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    (runtime : LocalFlexCommitterRuntime timed source)
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults timed.execution.trace
      genesis)
    (validChain : Nat → List (CommonCommitRef CommitId) → Prop)
    (validBlocks : CommitSyncBundle BlockId CommitId → Prop) where
  blockCarriesCommitVote : ValidatorBlock BlockId →
    CommonCommitRef CommitId → Prop
  /-- The synchronization chain check is the digest-link check. Rust checks
  indexes, digest links, and block references over a returned range, and each
  commit body holds `previous_digest`. This field maps one code path to one
  definition and states nothing about storage.

  The chain-to-prefix rule that used to sit here is now the theorem
  `ExactCommitDurablePrefixSourceMap.digest_chain_entry_matches_installed_prefix`,
  and `verifiedChainEntryMatchesInstalledPrefix` below re-exposes it with the
  same signature. -/
  validChainIsDigestChain : ∀ afterIndex commits,
    validChain afterIndex commits →
    DigestLinkedCommits durable.validCommitBody commits
  localInstallOrigin : ∀ time validator reference,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    0 < reference.index →
    durable.exactInstalledHead time validator reference →
    ((timed.execution.trace time).validatorState validator).commitInstallSourceAt
        reference.index = some .localExecution →
    Nonempty (LocalInstalledExactOrigin runtime durable time validator reference)
  verifiedSyncInstallOrigin : ∀ time validator reference,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    0 < reference.index →
    durable.exactInstalledHead time validator reference →
    ((timed.execution.trace time).validatorState validator).commitInstallSourceAt
        reference.index = some .verifiedCommitSync →
    Nonempty (VerifiedSyncInstalledExactOrigin durable validChain validBlocks
      blockCarriesCommitVote time validator reference)
  /-- One correct host can persist an authenticated commit-vote carrier only
  after that same host installed the signed `(index, digest)`. This is a
  single-host ordering rule. -/
  correctCarrierFollowsInstalledCommit : ∀ author carrier
      (origin : AuthenticatedCommitVoteCarrierOrigin timed
        blockCarriesCommitVote carrier),
    carrier.author = author →
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ∃ installTime signedHead,
      installTime < origin.time ∧
        durable.exactInstalledHead installTime author signedHead ∧
        signedHead.index = carrier.reference.index ∧
        signedHead.id = carrier.reference.id

namespace ExactCommitInstallProvenance

variable {BlockId CommitId History Encoding PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {functions : CommitReferenceFunctions
  CommitId (LeaderBlockRef BlockId) Encoding}
variable {context : ValidatorFlexContextAt BlockId CommitId History}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {source : LocalFlexCommitterSourceMap config functions context program}
variable {runtime : LocalFlexCommitterRuntime timed source}
variable {genesis : ValidatorCommitHead CommitId}
variable {durable : ExactCommitDurablePrefixSourceMap faults
  timed.execution.trace genesis}
variable {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
variable {validBlocks : CommitSyncBundle BlockId CommitId → Prop}

/-- The chain-to-prefix rule, now derived instead of assumed.

The signature is the one the removed field had, so call sites do not change.
The proof is
`ExactCommitDurablePrefixSourceMap.digest_chain_entry_matches_installed_prefix`
applied to the digest meaning of the synchronization chain check. -/
theorem verifiedChainEntryMatchesInstalledPrefix
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks) :
    ∀ time author afterIndex tipHead tip commits entry,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      durable.exactInstalledHead time author tipHead →
      ConsecutiveCommitIndices commits →
      validChain afterIndex commits →
      commits.getLast? = some tip →
      tipHead.index = tip.index → tipHead.id = tip.id →
      entry ∈ commits →
      ∃ installedEntry,
        durable.exactInstalledHead time author installedEntry ∧
          installedEntry.index = entry.index ∧ installedEntry.id = entry.id := by
  intro time author afterIndex tipHead tip commits entry authorInRange
    authorCorrect tipInstalled consecutive chain lastTip tipIndex tipId member
  exact durable.digest_chain_entry_matches_installed_prefix authorInRange
    authorCorrect commits consecutive
    (provenance.validChainIsDigestChain afterIndex commits chain)
    tip lastTip tipHead tipInstalled tipIndex tipId entry member

/-- Follow local install provenance or strictly earlier certified carrier
installs until a same-host local full-run origin is found. The origin install
is not later than the installed entry from which the search starts. -/
theorem exactOriginAtOrBefore
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {time validator : Nat} {reference : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrect : faults.correctAvailable validator = true)
    (positive : 0 < reference.index)
    (installed : durable.exactInstalledHead time validator reference) :
    Nonempty { origin : CorrectInstalledExactOrigin runtime durable reference //
      origin.installTime ≤ time } := by
  induction time using Nat.strongRecOn generalizing validator reference with
  | ind time inductionHypothesis =>
      have stored := durable.exactHeadHasStoredId time validator reference
        installed
      have permitted :=
        (timed.execution.statesWellFormed time validator validatorInRange)
          |>.installedCommitHasPermittedSource reference.index reference.id
            positive stored
      rcases permitted with localSource | syncSource
      · rcases provenance.localInstallOrigin time validator reference
            validatorInRange validatorCorrect positive installed localSource with
          ⟨origin⟩
        exact ⟨⟨origin.origin, by
          rw [origin.sameInstallTime]
          exact Nat.le_refl time⟩⟩
      · rcases provenance.verifiedSyncInstallOrigin time validator reference
            validatorInRange validatorCorrect positive installed syncSource with
          ⟨syncOrigin⟩
        rcases syncOrigin.bundleVerified with
          ⟨_firstIndex, lastTip, consecutive, chain, _blocks,
            certificateQuorum, certifiedCarrier⟩
        rcases quorum_set_has_correct_available_member faults
            syncOrigin.bundle.certifyingAuthors certificateQuorum with
          ⟨author, authorInRange, authorCorrect, authorCertified⟩
        rcases certifiedCarrier author authorInRange authorCertified with
          ⟨carrier, carrierMember, carrierAuthor, _blockAuthor,
            carrierReference⟩
        have carrierTipIndex : carrier.reference.index = syncOrigin.tip.index := by
          rw [carrierReference]
        have carrierTipId : carrier.reference.id = syncOrigin.tip.id := by
          rw [carrierReference]
        rcases syncOrigin.certifiedCarrierHasPersistenceOrigin author carrier
            authorInRange authorCorrect carrierMember carrierAuthor with
          ⟨carrierOrigin, carrierBeforeSync⟩
        rcases provenance.correctCarrierFollowsInstalledCommit author carrier
            carrierOrigin carrierAuthor authorInRange authorCorrect with
          ⟨carrierInstallTime, signedTipHead, installBeforeCarrier,
            signedTipInstalled, signedCarrierIndex, signedCarrierId⟩
        have carrierInstallBeforeSync : carrierInstallTime <
            syncOrigin.syncTime :=
          Nat.lt_trans installBeforeCarrier carrierBeforeSync
        have carrierInstallBeforeInstall : carrierInstallTime < time := by
          have syncBeforeInstall : syncOrigin.syncTime < time :=
            Nat.lt_of_succ_le syncOrigin.syncVisibleByInstall
          exact Nat.lt_trans carrierInstallBeforeSync syncBeforeInstall
        have signedTipIndex : signedTipHead.index = syncOrigin.tip.index :=
          signedCarrierIndex.trans carrierTipIndex
        have signedTipId : signedTipHead.id = syncOrigin.tip.id :=
          signedCarrierId.trans carrierTipId
        rcases provenance.verifiedChainEntryMatchesInstalledPrefix
            carrierInstallTime author syncOrigin.afterIndex signedTipHead
            syncOrigin.tip syncOrigin.bundle.commits reference authorInRange
            authorCorrect signedTipInstalled consecutive chain lastTip
            signedTipIndex signedTipId syncOrigin.referenceInBundle with
          ⟨installedEntry, installedEntryExact, installedEntryIndex,
            installedEntryId⟩
        have installedEntryIsReference : installedEntry = reference :=
          durable.validBodyDigestBinding installedEntry reference
            (durable.exactInstalledHeadIsValid carrierInstallTime author
              installedEntry installedEntryExact)
            (durable.exactInstalledHeadIsValid time validator reference
              installed)
            installedEntryIndex installedEntryId
        subst installedEntry
        rcases inductionHypothesis carrierInstallTime carrierInstallBeforeInstall
            (validator := author) (reference := reference) authorInRange
            authorCorrect positive installedEntryExact with
          ⟨⟨origin, originVisibleByCarrierInstall⟩⟩
        exact ⟨⟨origin, Nat.le_trans originVisibleByCarrierInstall
          (Nat.le_of_lt carrierInstallBeforeInstall)⟩⟩

/-- Every positive installed exact head has a correct local full-run origin. -/
theorem exactOrigin
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {time validator : Nat} {reference : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrect : faults.correctAvailable validator = true)
    (positive : 0 < reference.index)
    (installed : durable.exactInstalledHead time validator reference) :
    Nonempty (CorrectInstalledExactOrigin runtime durable reference) := by
  rcases provenance.exactOriginAtOrBefore validatorInRange validatorCorrect
      positive installed with ⟨⟨origin, _originVisibleByTime⟩⟩
  exact ⟨origin⟩

/-- The exact predecessor in an origin is one index before the installed
reference. -/
theorem originPriorIndex
    {reference : ValidatorCommitHead CommitId}
    (origin : CorrectInstalledExactOrigin runtime durable reference) :
    origin.run.prior.index + 1 = reference.index := by
  have step : ExactFlexSuccessor runtime origin.run.prior reference := by
    exact ⟨origin.run, rfl, origin.runOutput⟩
  exact (ExactFlexSuccessor.nextIndex step).symm

/-- Every exact installed full head has a finite correct FlexCommitter path
from genesis. This is the installed-prefix induction; it is not an input. -/
theorem exactInstalledHeadHasPath
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {time validator : Nat} {reference : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrect : faults.correctAvailable validator = true)
    (installed : durable.exactInstalledHead time validator reference) :
    ExactCommitPath (ExactFlexSuccessor runtime) genesis reference.index
      reference := by
  have pathAtIndex : ∀ index time validator
      (reference : ValidatorCommitHead CommitId),
      reference.index = index →
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      durable.exactInstalledHead time validator reference →
      ExactCommitPath (ExactFlexSuccessor runtime) genesis index reference := by
    intro index
    induction index with
    | zero =>
        intro time validator reference referenceIndex validatorInRange
          validatorCorrect installed
        have isGenesis := durable.zeroHeadIsGenesis time validator reference
          validatorInRange validatorCorrect installed referenceIndex
        subst reference
        exact ExactCommitPath.genesis
    | succ index inductionHypothesis =>
        intro time validator reference referenceIndex validatorInRange
          validatorCorrect installed
        have positive : 0 < reference.index := by omega
        rcases provenance.exactOrigin validatorInRange validatorCorrect positive
            installed with ⟨origin⟩
        have priorIndex : origin.run.prior.index = index := by
          have relation := originPriorIndex (runtime := runtime) origin
          omega
        have priorPath := inductionHypothesis origin.run.observation.time
          origin.run.observation.validator origin.run.prior priorIndex
          origin.run.validatorInRange origin.run.validatorCorrect
          origin.priorInstalled
        have successor : ExactFlexSuccessor runtime origin.run.prior reference :=
          ⟨origin.run, rfl, origin.runOutput⟩
        exact ExactCommitPath.next priorPath successor
  exact pathAtIndex reference.index time validator reference rfl
    validatorInRange validatorCorrect installed

/-- Two provenance-backed exact installed heads at the same index are equal. -/
theorem exactInstalledHeadsAtSameIndexAgree
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {leftTime leftValidator rightTime rightValidator : Nat}
    {left right : ValidatorCommitHead CommitId}
    (leftValidatorInRange : leftValidator < config.authorityCount)
    (leftValidatorCorrect : faults.correctAvailable leftValidator = true)
    (rightValidatorInRange : rightValidator < config.authorityCount)
    (rightValidatorCorrect : faults.correctAvailable rightValidator = true)
    (leftInstalled :
      durable.exactInstalledHead leftTime leftValidator left)
    (rightInstalled :
      durable.exactInstalledHead rightTime rightValidator right)
    (sameIndex : left.index = right.index) : left = right := by
  have leftPath := provenance.exactInstalledHeadHasPath
    leftValidatorInRange leftValidatorCorrect leftInstalled
  have rightPath := provenance.exactInstalledHeadHasPath
    rightValidatorInRange rightValidatorCorrect rightInstalled
  have rightPathAtLeft : ExactCommitPath (ExactFlexSuccessor runtime) genesis
      left.index right := by
    rw [sameIndex]
    exact rightPath
  have successorUnique : ∀ prior first second,
      ExactFlexSuccessor runtime prior first →
      ExactFlexSuccessor runtime prior second → first = second := by
    intro prior first second firstStep secondStep
    exact ExactFlexSuccessor.unique authenticated firstStep secondStep
  exact exact_commit_path_unique successorUnique leftPath rightPathAtLeft

/-- Recover the complete exact head, including its round, from one actual
durable `(index, digest)` witness. -/
theorem exactHeadForStoredId
    (_provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {time validator index : Nat} {commitId : CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrect : faults.correctAvailable validator = true)
    (stored : ValidatorLocalState.installedCommitAt
      ((timed.execution.trace time).validatorState validator) index =
        some commitId) :
    ∃ reference,
      durable.exactInstalledHead time validator reference ∧
        reference.index = index ∧ reference.id = commitId :=
  durable.storedIdHasExactHead time validator index commitId validatorInRange
    validatorCorrect stored

/-- Two actual correct durable entries at one index contain the same digest.
The proof recovers both full heads and runs the path induction. -/
theorem storedIdsAtSameIndexAgree
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {leftTime leftValidator rightTime rightValidator index : Nat}
    {leftId rightId : CommitId}
    (leftValidatorInRange : leftValidator < config.authorityCount)
    (leftValidatorCorrect : faults.correctAvailable leftValidator = true)
    (rightValidatorInRange : rightValidator < config.authorityCount)
    (rightValidatorCorrect : faults.correctAvailable rightValidator = true)
    (leftStored : ValidatorLocalState.installedCommitAt
      ((timed.execution.trace leftTime).validatorState leftValidator) index =
        some leftId)
    (rightStored : ValidatorLocalState.installedCommitAt
      ((timed.execution.trace rightTime).validatorState rightValidator) index =
        some rightId) : leftId = rightId := by
  rcases provenance.exactHeadForStoredId leftValidatorInRange
      leftValidatorCorrect leftStored with
    ⟨left, leftExact, leftIndex, leftIdEq⟩
  rcases provenance.exactHeadForStoredId rightValidatorInRange
      rightValidatorCorrect rightStored with
    ⟨right, rightExact, rightIndex, rightIdEq⟩
  have sameHead := provenance.exactInstalledHeadsAtSameIndexAgree authenticated
    leftValidatorInRange leftValidatorCorrect rightValidatorInRange
    rightValidatorCorrect leftExact rightExact (leftIndex.trans rightIndex.symm)
  exact leftIdEq.symm.trans
    ((congrArg ValidatorCommitHead.id sameHead).trans rightIdEq)

/-- If a correct validator's current head is at or above one provenance-backed
common head, its durable prefix contains that exact digest. -/
theorem exactHeadAtOrBelowLocalHeadIsStored
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {commonTime commonValidator targetTime targetValidator : Nat}
    {reference : ValidatorCommitHead CommitId}
    (commonValidatorInRange : commonValidator < config.authorityCount)
    (commonValidatorCorrect : faults.correctAvailable commonValidator = true)
    (commonInstalled :
      durable.exactInstalledHead commonTime commonValidator reference)
    (targetValidatorInRange : targetValidator < config.authorityCount)
    (targetValidatorCorrect : faults.correctAvailable targetValidator = true)
    (atOrAbove : reference.index ≤
      (timed.execution.trace targetTime).localCommitIndex targetValidator) :
    ValidatorLocalState.installedCommitAt
      ((timed.execution.trace targetTime).validatorState targetValidator)
      reference.index = some reference.id := by
  rcases durable.installedAtOrBelowHead targetTime targetValidator
      reference.index targetValidatorInRange targetValidatorCorrect atOrAbove with
    ⟨targetId, targetStored⟩
  have sameId := provenance.storedIdsAtSameIndexAgree authenticated
    commonValidatorInRange commonValidatorCorrect targetValidatorInRange
    targetValidatorCorrect (durable.exactHeadHasStoredId commonTime
      commonValidator reference commonInstalled) targetStored
  rwa [← sameId] at targetStored

end ExactCommitInstallProvenance

end Mysticeti
