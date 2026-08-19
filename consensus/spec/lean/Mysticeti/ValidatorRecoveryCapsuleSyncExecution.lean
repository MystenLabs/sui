/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorDeliveryAcceptance
import Mysticeti.ValidatorLocalDagCommitPropagation
import Mysticeti.ValidatorRecoveryParentNeedSync
import Mysticeti.ValidatorRecoveryTipRebroadcastExecution

namespace Mysticeti

/-! Recursive recovery-capsule synchronization in the main validator trace.

A requester learns only from an exact local block body. The body can come from
an authenticated delivered packet or from its accepted local catalog. Direct
parent references above the requester GC round create independent recursive
needs. A small local body pin keeps each observed above-GC body usable until
the local commit changes, GC reaches it, or the epoch ends.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- Requester-local pins for exact block bodies observed during recursive
recovery. Each reference has independent state. -/
structure ValidatorRecoveryObservedBodyPinState
    (BlockId CommitId : Type) where
  active : ValidatorBlockRef BlockId → Option (ValidatorCommitHead CommitId)

/-- Main-trace recursive synchronization and requester-local body pins. -/
structure ValidatorRecoveryCapsuleSyncExecution
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (syncRules : ValidatorBlockSyncExecutionRules timed) where
  recursive : ValidatorRecoveryRecursiveParentNeedExecution syncRules
  bodyPins : Time → Nat →
    ValidatorRecoveryObservedBodyPinState BlockId CommitId
  /-- A processed body creates its exact pin in the same execution batch. -/
  localBodyLatchesPin : ∀ time validator block,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (timed.execution.trace (time + 1)).epochActive = true →
    ((timed.execution.trace (time + 1)).validatorState validator).gcRound <
      block.reference.round →
    ValidatorLocalBlockBodyAt timed time validator block →
    (bodyPins (time + 1) validator).active block.reference =
      some ((timed.execution.trace (time + 1)).validatorState
        validator).commitHead
  /-- Every pin has a past exact same-host body origin. -/
  activePinHasLocalBodyOrigin : ∀ time validator reference baseline,
    (bodyPins time validator).active reference = some baseline →
    ∃ originTime block,
      originTime < time ∧
        block.reference = reference ∧
        ValidatorLocalBlockBodyAt timed originTime validator block
  /-- An observed body pin survives GC and restart while its commit baseline is
  current and GC remains below the reference. A pin for another reference
  cannot clear this pin. -/
  activePinPersistsOneStep : ∀ time validator reference baseline,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (bodyPins time validator).active reference = some baseline →
    (timed.execution.trace (time + 1)).epochActive = true →
    ((timed.execution.trace (time + 1)).validatorState validator).gcRound <
      reference.round →
    ((timed.execution.trace (time + 1)).validatorState
      validator).commitHead.index ≤ baseline.index →
    (bodyPins (time + 1) validator).active reference = some baseline
  /-- An accepted pinned body remains locally usable by a recovery proposal. -/
  acceptedPinnedBodyIsRetained : ∀ time validator reference baseline,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (bodyPins time validator).active reference = some baseline →
    ((timed.execution.trace time).validatorState validator).gcRound <
      reference.round →
    ((timed.execution.trace time).validatorState validator).accepted reference =
      true →
    ((timed.execution.trace time).validatorState validator).retained reference =
      true

/-- Recovery block acceptance can stop dependency checks at the requester's
local committed GC roots. It uses the normal delivery and buffer rules. -/
structure ValidatorRecoveryGcParentReadyAcceptanceRules
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    extends ValidatorParentReadyAcceptanceRules timed where
  /-- A buffered block can run when each direct parent is accepted or is a
  committed root at or below the local GC round. -/
  parentReadyOrGcRootEnablesAccept : ∀ validator block time,
    buffered validator block time →
    ((timed.execution.trace time).validatorState validator).gcRound <
      block.reference.round →
    (∀ parent, parent ∈ block.parents →
      ((timed.execution.trace time).validatorState validator).accepted parent =
          true ∨
        parent.round ≤
          ((timed.execution.trace time).validatorState validator).gcRound) →
    (((timed.execution.trace time).validatorState validator).accepted
          block.reference = true ∨
      timed.protectedAction time validator (.acceptBlock block))

namespace ValidatorRecoveryCapsuleSyncExecution

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {syncRules : ValidatorBlockSyncExecutionRules timed}

/-- One exact body was visible in the stated finite trace interval. -/
def BodyObservedBetween
    (start finish validator : Time) (block : ValidatorBlock BlockId) : Prop :=
  ∃ observedAt,
    start ≤ observedAt ∧ observedAt ≤ finish ∧
      ValidatorLocalBlockBodyAt timed observedAt validator block

/-- One required history body is local, or its reference is now a committed
root at or below this requester's GC round. -/
def BodyObservedOrGcRootAt
    (start finish validator : Time) (block : ValidatorBlock BlockId) : Prop :=
  BodyObservedBetween (timed := timed) start finish validator block ∨
    block.reference.round ≤
      ((timed.execution.trace finish).validatorState validator).gcRound

/-- One validator's GC round cannot decrease. -/
theorem validator_gc_round_mono
    {earlier later validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (ordered : earlier ≤ later) :
    ((timed.execution.trace earlier).validatorState validator).gcRound ≤
      ((timed.execution.trace later).validatorState validator).gcRound := by
  have durable := timed.execution.durableStateMonotone validator earlier later
    validatorInRange ordered
  rcases durable with ⟨_, _, _, _, _, _, _, _, _, _, _, gcMonotone⟩
  exact gcMonotone

/-- A list with distinct mapped values also has distinct source values. -/
theorem list_nodup_of_map_nodup
    {α β : Type} {function : α → β} {items : List α}
    (mappedNodup : (items.map function).Nodup) :
    items.Nodup := by
  induction items with
  | nil => simp
  | cons item tail inductionHypothesis =>
      simp only [List.map_cons, List.nodup_cons] at mappedNodup ⊢
      constructor
      · intro itemInTail
        exact mappedNodup.1 (List.mem_map.mpr ⟨item, itemInTail, rfl⟩)
      · exact inductionHypothesis mappedNodup.2

/-- In a list without duplicates, an item that occurs after a source belongs
to every suffix that starts immediately after that source. -/
theorem list_after_member_of_suffix
    {α : Type} {items front suffix before between after : List α}
    {source target : α}
    (nodup : items.Nodup)
    (split : items = front ++ source :: suffix)
    (ordered : items = before ++ source :: between ++ target :: after) :
    target ∈ suffix := by
  have reversedNodup : items.reverse.Nodup := by
    change List.Pairwise (fun a b : α => a ≠ b) items.reverse
    rw [List.pairwise_reverse]
    exact nodup.imp (fun different => Ne.symm different)
  have reversedSplit :
      items.reverse = suffix.reverse ++ source :: front.reverse := by
    rw [split]
    simp [List.reverse_append, List.append_assoc]
  have reversedOrder :
      items.reverse =
        after.reverse ++ target :: (between.reverse ++ source :: before.reverse) := by
    rw [ordered]
    simp [List.reverse_append, List.append_assoc]
  have targetInReverse := list_before_member_of_front reversedNodup
    reversedSplit reversedOrder
  simpa using targetInReverse

/-- A non-target history item has a direct child in the suffix after it. -/
theorem non_target_history_block_has_child_in_suffix
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    {front tail : List (ValidatorBlock BlockId)}
    {block : ValidatorBlock BlockId}
    (split : capsule.history = front ++ block :: tail)
    (notTarget : block ≠ capsule.targetBlock) :
    ∃ child,
      child ∈ tail ∧ block.reference ∈ child.parents := by
  have blockMember : block ∈ capsule.history := by
    rw [split]
    simp
  rcases capsule.non_target_block_has_later_child blockMember notTarget with
    ⟨child, before, between, after, _childMember, parentInChild, ordered⟩
  have historyNodup : capsule.history.Nodup :=
    list_nodup_of_map_nodup capsule.historyReferencesNodup
  exact ⟨child,
    list_after_member_of_suffix historyNodup split ordered,
    parentInChild⟩

/-- A later target-closure walk can use only bodies from the pinned source
capsule. The global catalog can gain entries, but it cannot replace one exact
body that was already in the source catalog. -/
theorem pinned_capsule_contains_later_above_cutoff_target_closure
    {start finish holder cutoff : Time}
    {capsule : CausalRecoveryCapsule (BlockId := BlockId) config}
    {reference : ValidatorBlockRef BlockId}
    (source : CausalRecoveryCapsuleExecutionSource syncRules capsule holder start)
    (ordered : start ≤ finish)
    (path : ValidatorCausalClosureReferenceAboveRound
      (timed.execution.trace finish) cutoff capsule.targetBlock.reference
        reference) :
    ∃ block, block ∈ capsule.history ∧ block.reference = reference := by
  induction path with
  | anchor _above =>
      exact ⟨capsule.targetBlock,
        capsule.target_and_parents_in_history.1, rfl⟩
  | @parent child parent catalogBlock childInClosure catalog
      referenceExact parentIncluded parentAbove inductionHypothesis =>
      rcases inductionHypothesis with
        ⟨capsuleChild, childMember, capsuleChildReference⟩
      have capsuleCatalogAtFinish :
          (timed.execution.trace finish).blockCatalog child.id =
            some capsuleChild := by
        have capsuleCatalogAtStart := source.catalog capsuleChild childMember
        have catalogMonotone := timed.execution.blockCatalogMonotone start finish
          ordered capsuleChild.reference.id capsuleChild capsuleCatalogAtStart
        simpa [capsuleChildReference] using catalogMonotone
      have sameChildBody : catalogBlock = capsuleChild := by
        rw [capsuleCatalogAtFinish] at catalog
        exact (Option.some.inj catalog).symm
      have parentInCapsuleChild : parent ∈ capsuleChild.parents := by
        simpa [sameChildBody] using parentIncluded
      rcases capsule.historyClosed capsuleChild childMember parent
          parentInCapsuleChild with
        parentGenesis | ⟨parentBlock, parentMember, parentReference⟩
      · have parentRoundZero :=
          capsule.genesisParentsAreRoundZero parent parentGenesis
        omega
      · exact ⟨parentBlock, parentMember, parentReference⟩

/-- Every result of an above-cutoff target walk is above that cutoff. -/
theorem above_cutoff_target_closure_reference_is_above
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {cutoff : Nat} {target reference : ValidatorBlockRef BlockId}
    (path : ValidatorCausalClosureReferenceAboveRound world cutoff target
      reference) :
    cutoff < reference.round := by
  cases path with
  | anchor above => exact above
  | parent _ _ _ _ above => exact above

/-- An observed-body fact remains true when only its upper time bound grows. -/
theorem body_observed_between_mono
    {start earlier later validator : Time} {block : ValidatorBlock BlockId}
    (ordered : earlier ≤ later)
    (observed : BodyObservedBetween (timed := timed) start earlier validator
      block) :
    BodyObservedBetween (timed := timed) start later validator block := by
  rcases observed with ⟨observedAt, afterStart, beforeEarlier, body⟩
  exact ⟨observedAt, afterStart, Nat.le_trans beforeEarlier ordered, body⟩

/-- Local observation or committed-root status remains true at a later time.
-/
theorem body_observed_or_gc_root_mono
    {start earlier later validator : Time} {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (ordered : earlier ≤ later)
    (ready : BodyObservedOrGcRootAt (timed := timed) start earlier validator
      block) :
    BodyObservedOrGcRootAt (timed := timed) start later validator block := by
  rcases ready with observed | atRoot
  · exact Or.inl (body_observed_between_mono ordered observed)
  · exact Or.inr (Nat.le_trans atRoot
      (validator_gc_round_mono (timed := timed) validatorInRange ordered))

/-- A delivered valid block is accepted after each direct parent is accepted
or becomes a local committed GC root. -/
theorem delivered_block_with_ready_or_gc_root_parents_is_accepted
    (rules : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    {deliveryTime parentsReadyAt : Time} {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    {block : ValidatorBlock BlockId}
    (packetPresent :
      (timed.execution.trace deliveryTime).packets packetId = some packet)
    (packetPayload : packet.payload = .block block)
    (delivered : ValidatorPacketDeliveryOccurs
      (timed.execution.events deliveryTime) packetId)
    (receiverInRange : packet.receiver < config.authorityCount)
    (receiverCorrectAvailable :
      faults.correctAvailable packet.receiver = true)
    (authorInRange : block.reference.author < config.authorityCount)
    (validParents : block.HasQuorumImmediateParents config)
    (deliveryBeforeParentsReady : deliveryTime + 1 ≤ parentsReadyAt)
    (blockAboveGc :
      ((timed.execution.trace parentsReadyAt).validatorState
        packet.receiver).gcRound < block.reference.round)
    (parentsReady : ∀ parent, parent ∈ block.parents →
      ((timed.execution.trace parentsReadyAt).validatorState
        packet.receiver).accepted parent = true ∨
      parent.round ≤
        ((timed.execution.trace parentsReadyAt).validatorState
          packet.receiver).gcRound) :
    ∃ acceptedAt,
      parentsReadyAt ≤ acceptedAt ∧
        ((timed.execution.trace acceptedAt).validatorState
          packet.receiver).accepted block.reference = true := by
  have gcAtDeliveryBeforeReady := validator_gc_round_mono (timed := timed)
    receiverInRange deliveryBeforeParentsReady
  have blockAboveGcAtDelivery :
      ((timed.execution.trace (deliveryTime + 1)).validatorState
        packet.receiver).gcRound < block.reference.round := by omega
  rcases rules.deliveryBuffersOrAccepts deliveryTime packetId packet block
      packetPresent packetPayload delivered authorInRange (Or.inr validParents)
      blockAboveGcAtDelivery with
    acceptedAtDelivery | bufferedAtDelivery
  · have acceptedAtReady := timed.execution.accepted_block_persists
      receiverInRange deliveryBeforeParentsReady acceptedAtDelivery
    exact ⟨parentsReadyAt, Nat.le_refl _, acceptedAtReady⟩
  · by_cases acceptedAtReady :
        ((timed.execution.trace parentsReadyAt).validatorState
          packet.receiver).accepted block.reference = true
    · exact ⟨parentsReadyAt, Nat.le_refl _, acceptedAtReady⟩
    · have bufferedAtReady := rules.bufferedPersistsWhilePending
        packet.receiver block (deliveryTime + 1) parentsReadyAt
        deliveryBeforeParentsReady bufferedAtDelivery (by
          simpa using acceptedAtReady) blockAboveGc
      rcases rules.parentReadyOrGcRootEnablesAccept packet.receiver block
          parentsReadyAt bufferedAtReady blockAboveGc parentsReady with
        acceptedNow | acceptEnabled
      · exact ⟨parentsReadyAt, Nat.le_refl _, acceptedNow⟩
      · let completion := timed.completeProtectedAction packet.receiver
          (.acceptBlock block) parentsReadyAt receiverInRange
          receiverCorrectAvailable acceptEnabled
        have acceptedAfter := accept_block_action_makes_block_accepted
          timed.execution completion.occurs
        refine ⟨completion.event.completedAt + 1, ?_, acceptedAfter⟩
        have startsBeforeCompletion :
            parentsReadyAt ≤ completion.event.completedAt := by
          simpa [completion.sameEnableTime] using
            completion.enableBeforeCompletion
        exact Nat.le_trans startsBeforeCompletion (Nat.le_add_right _ _)

/-- A processed exact body creates a pin with an explicit local origin. -/
theorem local_body_creates_durable_pin
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    {time validator : Time} {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (activeAfter : (timed.execution.trace (time + 1)).epochActive = true)
    (aboveGc : ((timed.execution.trace (time + 1)).validatorState
      validator).gcRound < block.reference.round)
    (body : ValidatorLocalBlockBodyAt timed time validator block) :
    (sync.bodyPins (time + 1) validator).active block.reference =
        some ((timed.execution.trace (time + 1)).validatorState
          validator).commitHead ∧
      ∃ originTime originBlock,
        originTime < time + 1 ∧
          originBlock.reference = block.reference ∧
          ValidatorLocalBlockBodyAt timed originTime validator originBlock := by
  have pinned := sync.localBodyLatchesPin time validator block validatorInRange
    validatorCorrectAvailable activeAfter aboveGc body
  exact ⟨pinned, sync.activePinHasLocalBodyOrigin (time + 1) validator
    block.reference _ pinned⟩

/-- One body pin survives any finite active interval with no local commit
advance beyond its baseline. -/
theorem body_pin_persists_while_head_is_current
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    {start finish validator : Time}
    {reference : ValidatorBlockRef BlockId}
    {baseline : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ordered : start ≤ finish)
    (pinned : (sync.bodyPins start validator).active reference = some baseline)
    (activeEpoch : ∀ time, start ≤ time → time ≤ finish →
      (timed.execution.trace time).epochActive = true)
    (aboveGc : ∀ time, start ≤ time → time ≤ finish →
      ((timed.execution.trace time).validatorState validator).gcRound <
        reference.round)
    (headCurrent : ∀ time, start ≤ time → time ≤ finish →
      ((timed.execution.trace time).validatorState
        validator).commitHead.index ≤ baseline.index) :
    (sync.bodyPins finish validator).active reference = some baseline := by
  have advance : ∀ offset,
      start + offset ≤ finish →
      (sync.bodyPins (start + offset) validator).active reference =
        some baseline := by
    intro offset
    induction offset with
    | zero =>
        intro _
        simpa using pinned
    | succ offset inductionHypothesis =>
        intro nextBeforeFinish
        have nextBeforeFinish' : start + offset + 1 ≤ finish := by
          simpa [Nat.add_assoc] using nextBeforeFinish
        have currentBeforeFinish : start + offset ≤ finish :=
          Nat.le_trans (Nat.le_add_right _ 1) nextBeforeFinish'
        have pinnedCurrent := inductionHypothesis currentBeforeFinish
        have startBeforeNext : start ≤ start + offset + 1 :=
          Nat.le_trans (Nat.le_add_right start offset) (Nat.le_add_right _ 1)
        simpa [Nat.add_assoc] using sync.activePinPersistsOneStep
          (start + offset) validator reference baseline validatorInRange
          validatorCorrectAvailable pinnedCurrent
          (activeEpoch (start + offset + 1) startBeforeNext nextBeforeFinish')
          (aboveGc (start + offset + 1) startBeforeNext nextBeforeFinish')
          (headCurrent (start + offset + 1) startBeforeNext nextBeforeFinish')
  obtain ⟨offset, finishShape⟩ := Nat.exists_eq_add_of_le ordered
  subst finish
  exact advance offset (Nat.le_refl _)

/-- Independent extra roots cannot remove one current exact-reference pin. -/
theorem body_pin_for_reference_survives_one_step
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    {time validator : Time} {reference : ValidatorBlockRef BlockId}
    {baseline : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (pinned : (sync.bodyPins time validator).active reference = some baseline)
    (activeAfter : (timed.execution.trace (time + 1)).epochActive = true)
    (aboveGc : ((timed.execution.trace (time + 1)).validatorState
      validator).gcRound < reference.round)
    (headCurrent : ((timed.execution.trace (time + 1)).validatorState
      validator).commitHead.index ≤ baseline.index) :
    (sync.bodyPins (time + 1) validator).active reference = some baseline :=
  sync.activePinPersistsOneStep time validator reference baseline
    validatorInRange validatorCorrectAvailable pinned activeAfter aboveGc
    headCurrent

/-- A local child-body observation leads to one exact parent-body observation.
The only global data in this theorem is the proof-only source capsule. The
requester rule sees only the child body and its direct parent reference. -/
theorem observed_child_eventually_exposes_direct_parent
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    {rootStart childTime holder requester : Time}
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId} {entry}
    {child block : ValidatorBlock BlockId}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ rootStart)
    (activeEpoch : ∀ time, rootStart ≤ time →
      (timed.execution.trace time).epochActive = true)
    (stored : (pins.trace rootStart holder).capsuleAt capsuleKey = some entry)
    (pinned : (pins.trace rootStart holder).pinned capsuleKey = true)
    (requesterHeadCurrent : ∀ time, rootStart ≤ time →
      ((timed.execution.trace time).validatorState requester).commitHead.index ≤
        ((timed.execution.trace rootStart).validatorState
          requester).commitHead.index)
    (rootBeforeChild : rootStart ≤ childTime)
    (childBody : ValidatorLocalBlockBodyAt timed childTime requester child)
    (member : block ∈ entry.capsule.history)
    (parentInChild : block.reference ∈ child.parents) :
    ∃ finish,
      childTime ≤ finish ∧
        (ValidatorLocalBlockBodyAt timed finish requester block ∨
          block.reference.round ≤
            ((timed.execution.trace finish).validatorState requester).gcRound) := by
  have afterRootAtLatch : rootStart ≤ childTime + 1 :=
    Nat.le_trans rootBeforeChild (Nat.le_add_right _ 1)
  by_cases parentAboveGc :
      ((timed.execution.trace (childTime + 1)).validatorState
        requester).gcRound < block.reference.round
  · cases parentAccepted :
        ((timed.execution.trace (childTime + 1)).validatorState
          requester).accepted block.reference with
    | true =>
        have sourceAtRoot := pins.pinned_capsule_is_execution_source
          holderInRange holderCorrectAvailable stored pinned
        have catalogAtRoot := sourceAtRoot.catalog block member
        have catalogAtLatch := timed.execution.blockCatalogMonotone rootStart
          (childTime + 1) afterRootAtLatch block.reference.id block catalogAtRoot
        exact ⟨childTime + 1, Nat.le_add_right _ 1,
          Or.inl (.acceptedCatalogued parentAccepted catalogAtLatch)⟩
    | false =>
        rcases sync.recursive.local_body_creates_exact_recursive_parent_need
            (activeEpoch (childTime + 1) afterRootAtLatch) childBody
            parentInChild parentAboveGc parentAccepted with
          ⟨need, activeNeed, needBaseline, _parentExact, _origin⟩
        have sourceAtLatch :=
          pins.pin_persists_while_epoch_active afterRootAtLatch stored pinned (by
            intro time timeAfterRoot _
            exact activeEpoch time timeAfterRoot)
        rcases ValidatorRecoveryRecursiveParentNeedExecution.pinned_history_item_eventually_delivered_or_requester_progress
            pins sync.recursive holderInRange holderCorrectAvailable requesterInRange
            requesterCorrectAvailable (Nat.le_trans afterGst afterRootAtLatch)
            (by
              intro time timeAfterLatch
              exact activeEpoch time
                (Nat.le_trans afterRootAtLatch timeAfterLatch))
            sourceAtLatch.1 sourceAtLatch.2 member activeNeed with
          ⟨finish, latchBeforeFinish, body | advanced⟩
        · exact ⟨finish, Nat.le_trans (Nat.le_add_right _ 1)
            latchBeforeFinish, Or.inl body⟩
        · have finishHeadCurrent := requesterHeadCurrent finish
            (Nat.le_trans afterRootAtLatch latchBeforeFinish)
          have rootHeadBeforeLatch :=
            (timed.execution.durableStateMonotone requester rootStart
              (childTime + 1) requesterInRange afterRootAtLatch).1
          rw [needBaseline] at advanced
          omega
  · exact ⟨childTime + 1, Nat.le_add_right _ 1, Or.inr (by omega)⟩

/-- One delivered capsule target exposes its complete causal closure above the
requester's moving GC round. Direct-parent discovery proceeds from the target
to older blocks. It stops when a reference becomes a committed GC root. -/
theorem delivered_target_eventually_discovers_above_gc_history
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    {rootStart targetTime holder requester : Time}
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId} {entry}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ rootStart)
    (activeEpoch : ∀ time, rootStart ≤ time →
      (timed.execution.trace time).epochActive = true)
    (stored : (pins.trace rootStart holder).capsuleAt capsuleKey = some entry)
    (pinned : (pins.trace rootStart holder).pinned capsuleKey = true)
    (requesterHeadCurrent : ∀ time, rootStart ≤ time →
      ((timed.execution.trace time).validatorState requester).commitHead.index ≤
        ((timed.execution.trace rootStart).validatorState
          requester).commitHead.index)
    (rootBeforeTarget : rootStart ≤ targetTime)
    (targetBody : ValidatorLocalBlockBodyAt timed targetTime requester
      entry.capsule.targetBlock) :
    ∃ finish,
      targetTime ≤ finish ∧
        ∀ block, block ∈ entry.capsule.history →
          BodyObservedOrGcRootAt (timed := timed) rootStart finish requester
            block := by
  classical
  have targetReady :
      BodyObservedOrGcRootAt (timed := timed) rootStart targetTime requester
        entry.capsule.targetBlock :=
    Or.inl ⟨targetTime, rootBeforeTarget, Nat.le_refl _, targetBody⟩
  have advance : ∀ remaining processed current,
      entry.capsule.history.reverse = processed ++ remaining →
      targetTime ≤ current →
      (∀ block, block ∈ processed →
        BodyObservedOrGcRootAt (timed := timed) rootStart current requester
          block) →
      ∃ finish,
        current ≤ finish ∧
          ∀ block, block ∈ processed ++ remaining →
            BodyObservedOrGcRootAt (timed := timed) rootStart finish requester
              block := by
    intro remaining
    induction remaining with
    | nil =>
        intro processed current _split _targetBeforeCurrent processedReady
        exact ⟨current, Nat.le_refl _, by simpa using processedReady⟩
    | cons block tail inductionHypothesis =>
        intro processed current reverseSplit targetBeforeCurrent processedReady
        have blockReady : ∃ next,
            current ≤ next ∧
              BodyObservedOrGcRootAt (timed := timed) rootStart next requester
                block := by
          by_cases isTarget : block = entry.capsule.targetBlock
          · subst block
            exact ⟨current, Nat.le_refl _,
              body_observed_or_gc_root_mono (timed := timed) requesterInRange
                targetBeforeCurrent targetReady⟩
          · have originalSplit :
                entry.capsule.history =
                  tail.reverse ++ block :: processed.reverse := by
              have reversed := congrArg List.reverse reverseSplit
              simpa [List.reverse_append, List.append_assoc] using reversed
            rcases non_target_history_block_has_child_in_suffix entry.capsule
                originalSplit isTarget with
              ⟨child, childInProcessedReverse, parentInChild⟩
            have childInProcessed : child ∈ processed := by
              simpa using childInProcessedReverse
            have childMember : child ∈ entry.capsule.history := by
              rw [originalSplit]
              simp [childInProcessedReverse]
            have childValid := entry.capsule.positiveHistoryBlocksValid child
              childMember (entry.capsule.historyBlocksPositive child childMember)
            have parentRound := childValid.2.1 block.reference parentInChild
            rcases processedReady child childInProcessed with
              childObserved | childAtRoot
            · rcases childObserved with
                ⟨childObservedAt, rootBeforeChild, childBeforeCurrent,
                  childBody⟩
              rcases observed_child_eventually_exposes_direct_parent pins sync
                  holderInRange holderCorrectAvailable requesterInRange
                  requesterCorrectAvailable afterGst activeEpoch stored pinned
                  requesterHeadCurrent rootBeforeChild childBody
                  (by
                    rw [originalSplit]
                    simp)
                  parentInChild with
                ⟨parentFinish, childBeforeFinish, parentBody | parentAtRoot⟩
              · refine ⟨max current parentFinish, Nat.le_max_left _ _, Or.inl ?_⟩
                exact ⟨parentFinish,
                  Nat.le_trans rootBeforeChild childBeforeFinish,
                  Nat.le_max_right _ _, parentBody⟩
              · refine ⟨max current parentFinish, Nat.le_max_left _ _, Or.inr ?_⟩
                exact Nat.le_trans parentAtRoot
                  (validator_gc_round_mono (timed := timed) requesterInRange
                    (Nat.le_max_right current parentFinish))
            · exact ⟨current, Nat.le_refl _, Or.inr (by omega)⟩
        rcases blockReady with ⟨next, currentBeforeNext, readyAtNext⟩
        have nextSplit :
            entry.capsule.history.reverse =
              (processed ++ [block]) ++ tail := by
          simpa [List.append_assoc] using reverseSplit
        have processedReadyAtNext : ∀ item, item ∈ processed ++ [block] →
            BodyObservedOrGcRootAt (timed := timed) rootStart next requester
              item := by
          intro item itemMember
          rcases List.mem_append.mp itemMember with
            itemInProcessed | itemIsBlock
          · exact body_observed_or_gc_root_mono (timed := timed)
              requesterInRange
              currentBeforeNext (processedReady item itemInProcessed)
          · have sameBlock : item = block := by simpa using itemIsBlock
            simpa [sameBlock] using readyAtNext
        rcases inductionHypothesis (processed ++ [block]) next nextSplit
            (Nat.le_trans targetBeforeCurrent currentBeforeNext)
            processedReadyAtNext with
          ⟨finish, nextBeforeFinish, allReady⟩
        exact ⟨finish, Nat.le_trans currentBeforeNext nextBeforeFinish, by
          simpa [List.append_assoc] using allReady⟩
  rcases advance entry.capsule.history.reverse [] targetTime (by simp)
      (Nat.le_refl _) (by simp) with
    ⟨finish, targetBeforeFinish, allReady⟩
  refine ⟨finish, targetBeforeFinish, ?_⟩
  intro block blockMember
  apply allReady
  simpa using blockMember

/-- One observed above-GC history block is accepted after its direct parents
are accepted or become committed GC roots. -/
theorem observed_history_block_eventually_accepted
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    {rootStart discoveryFinish current holder requester : Time}
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId} {entry}
    {block : ValidatorBlock BlockId}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (stored : (pins.trace rootStart holder).capsuleAt capsuleKey = some entry)
    (pinned : (pins.trace rootStart holder).pinned capsuleKey = true)
    (member : block ∈ entry.capsule.history)
    (observed : BodyObservedBetween (timed := timed) rootStart discoveryFinish
      requester block)
    (discoveryBeforeCurrent : discoveryFinish + 1 ≤ current)
    (blockAboveGc :
      ((timed.execution.trace current).validatorState requester).gcRound <
        block.reference.round)
    (parentsReady : ∀ parent, parent ∈ block.parents →
      ((timed.execution.trace current).validatorState requester).accepted
          parent = true ∨
        parent.round ≤
          ((timed.execution.trace current).validatorState requester).gcRound) :
    ∃ finish,
      current ≤ finish ∧
        ((timed.execution.trace finish).validatorState requester).accepted
          block.reference = true := by
  rcases observed with
    ⟨observedAt, _rootBeforeObserved, observedBeforeDiscovery, body⟩
  have observedBeforeCurrent : observedAt ≤ current :=
    Nat.le_trans observedBeforeDiscovery
      (Nat.le_trans (Nat.le_add_right _ 1) discoveryBeforeCurrent)
  have source := pins.pinned_capsule_is_execution_source holderInRange
    holderCorrectAvailable stored pinned
  have authorInRange := source.authorInRange block member
  have validParents := entry.capsule.positiveHistoryBlocksValid block member
    (entry.capsule.historyBlocksPositive block member)
  cases body with
  | acceptedCatalogued accepted _catalogued =>
      exact ⟨current, Nat.le_refl _,
        timed.execution.accepted_block_persists requesterInRange
          observedBeforeCurrent accepted⟩
  | delivered packetId packet packetPresent _packetIsProtocol packetReceiver
        packetPayload deliveryOccurs =>
      have receiverInRange : packet.receiver < config.authorityCount := by
        simpa [packetReceiver] using requesterInRange
      have receiverCorrectAvailable :
          faults.correctAvailable packet.receiver = true := by
        simpa [packetReceiver] using requesterCorrectAvailable
      have deliveryBeforeCurrent : observedAt + 1 ≤ current :=
        Nat.le_trans (Nat.add_le_add_right observedBeforeDiscovery 1)
          discoveryBeforeCurrent
      rcases delivered_block_with_ready_or_gc_root_parents_is_accepted
          acceptance packetPresent packetPayload deliveryOccurs receiverInRange
          receiverCorrectAvailable authorInRange validParents
          deliveryBeforeCurrent (by simpa [packetReceiver] using blockAboveGc)
          (by
            intro parent parentMember
            simpa [packetReceiver] using parentsReady parent parentMember) with
        ⟨acceptedAt, currentBeforeAccepted, accepted⟩
      exact ⟨acceptedAt, currentBeforeAccepted, by
        simpa [packetReceiver] using accepted⟩

/-- Parent-first local processing accepts every discovered history body that
remains above GC. A block that reaches GC needs no body retention or new
acceptance work. -/
theorem discovered_history_eventually_accepted_or_gc_root
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    {rootStart discoveryFinish holder requester : Time}
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId} {entry}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (stored : (pins.trace rootStart holder).capsuleAt capsuleKey = some entry)
    (pinned : (pins.trace rootStart holder).pinned capsuleKey = true)
    (discovered : ∀ block, block ∈ entry.capsule.history →
      BodyObservedOrGcRootAt (timed := timed) rootStart discoveryFinish
        requester block) :
    ∃ finish,
      discoveryFinish + 1 ≤ finish ∧
        ∀ block, block ∈ entry.capsule.history →
          ((timed.execution.trace finish).validatorState requester).accepted
              block.reference = true ∨
            block.reference.round ≤
              ((timed.execution.trace finish).validatorState requester).gcRound := by
  classical
  have historyNodup : entry.capsule.history.Nodup :=
    list_nodup_of_map_nodup entry.capsule.historyReferencesNodup
  have advance : ∀ remaining processed current,
      entry.capsule.history = processed ++ remaining →
      discoveryFinish + 1 ≤ current →
      (∀ block, block ∈ processed →
        ((timed.execution.trace current).validatorState requester).accepted
            block.reference = true ∨
          block.reference.round ≤
            ((timed.execution.trace current).validatorState requester).gcRound) →
      ∃ finish,
        current ≤ finish ∧
          ∀ block, block ∈ processed ++ remaining →
            ((timed.execution.trace finish).validatorState requester).accepted
                block.reference = true ∨
              block.reference.round ≤
                ((timed.execution.trace finish).validatorState
                  requester).gcRound := by
    intro remaining
    induction remaining with
    | nil =>
        intro processed current _split _discoveryBeforeCurrent processedReady
        exact ⟨current, Nat.le_refl _, by simpa using processedReady⟩
    | cons block tail inductionHypothesis =>
        intro processed current historySplit discoveryBeforeCurrent processedReady
        have blockMember : block ∈ entry.capsule.history := by
          rw [historySplit]
          simp
        by_cases blockAtRoot : block.reference.round ≤
            ((timed.execution.trace current).validatorState requester).gcRound
        · have nextSplit :
              entry.capsule.history = (processed ++ [block]) ++ tail := by
            simpa [List.append_assoc] using historySplit
          have processedReadyWithBlock : ∀ item,
              item ∈ processed ++ [block] →
              ((timed.execution.trace current).validatorState requester).accepted
                    item.reference = true ∨
                item.reference.round ≤
                  ((timed.execution.trace current).validatorState
                    requester).gcRound := by
            intro item itemMember
            rcases List.mem_append.mp itemMember with
              itemInProcessed | itemIsBlock
            · exact processedReady item itemInProcessed
            · have sameBlock : item = block := by simpa using itemIsBlock
              simpa [sameBlock] using Or.inr blockAtRoot
          rcases inductionHypothesis (processed ++ [block]) current nextSplit
              discoveryBeforeCurrent processedReadyWithBlock with
            ⟨finish, currentBeforeFinish, allReady⟩
          exact ⟨finish, currentBeforeFinish, by
            simpa [List.append_assoc] using allReady⟩
        · have blockAboveGc :
              ((timed.execution.trace current).validatorState
                requester).gcRound < block.reference.round := by omega
          have blockObserved : BodyObservedBetween (timed := timed) rootStart
              discoveryFinish requester block := by
            rcases discovered block blockMember with observed | atRoot
            · exact observed
            · have gcBeforeCurrent := validator_gc_round_mono (timed := timed)
                requesterInRange
                (Nat.le_trans (Nat.le_add_right discoveryFinish 1)
                  discoveryBeforeCurrent)
              omega
          have parentsReady : ∀ parent, parent ∈ block.parents →
              ((timed.execution.trace current).validatorState
                    requester).accepted parent = true ∨
                parent.round ≤
                  ((timed.execution.trace current).validatorState
                    requester).gcRound := by
            intro parent parentInBlock
            by_cases parentAtRoot : parent.round ≤
                ((timed.execution.trace current).validatorState
                  requester).gcRound
            · exact Or.inr parentAtRoot
            · have parentAboveGc :
                  ((timed.execution.trace current).validatorState
                    requester).gcRound < parent.round := by omega
              rcases entry.capsule.historyClosed block blockMember parent
                  parentInBlock with
                parentGenesis | ⟨parentBlock, parentBlockMember,
                  parentReference⟩
              · have parentRoundZero :=
                  entry.capsule.genesisParentsAreRoundZero parent parentGenesis
                omega
              · rcases entry.capsule.historyTopological block parentBlock
                    blockMember parentBlockMember (by
                      simpa [parentReference] using parentInBlock) with
                  ⟨before, between, after, ordered⟩
                have ordered' :
                    entry.capsule.history =
                      before ++ (parentBlock :: between ++ (block :: after)) := by
                  simpa [List.append_assoc] using ordered
                have parentInProcessed := list_before_member_of_front
                  historyNodup historySplit ordered'
                rcases processedReady parentBlock parentInProcessed with
                  parentAccepted | parentBlockAtRoot
                · exact Or.inl (by simpa [parentReference] using parentAccepted)
                · rw [parentReference] at parentBlockAtRoot
                  omega
          rcases observed_history_block_eventually_accepted pins acceptance
              holderInRange holderCorrectAvailable requesterInRange
              requesterCorrectAvailable stored pinned blockMember blockObserved
              discoveryBeforeCurrent blockAboveGc parentsReady with
            ⟨blockFinish, currentBeforeBlockFinish, blockAccepted⟩
          have nextSplit :
              entry.capsule.history = (processed ++ [block]) ++ tail := by
            simpa [List.append_assoc] using historySplit
          have processedReadyWithBlock : ∀ item,
              item ∈ processed ++ [block] →
              ((timed.execution.trace blockFinish).validatorState
                    requester).accepted item.reference = true ∨
                item.reference.round ≤
                  ((timed.execution.trace blockFinish).validatorState
                    requester).gcRound := by
            intro item itemMember
            rcases List.mem_append.mp itemMember with
              itemInProcessed | itemIsBlock
            · rcases processedReady item itemInProcessed with
                itemAccepted | itemAtRoot
              · exact Or.inl (timed.execution.accepted_block_persists
                  requesterInRange currentBeforeBlockFinish itemAccepted)
              · exact Or.inr (Nat.le_trans itemAtRoot
                  (validator_gc_round_mono (timed := timed) requesterInRange
                    currentBeforeBlockFinish))
            · have sameBlock : item = block := by simpa using itemIsBlock
              simpa [sameBlock] using Or.inl blockAccepted
          rcases inductionHypothesis (processed ++ [block]) blockFinish
              nextSplit
              (Nat.le_trans discoveryBeforeCurrent currentBeforeBlockFinish)
              processedReadyWithBlock with
            ⟨finish, blockBeforeFinish, allReady⟩
          exact ⟨finish, Nat.le_trans currentBeforeBlockFinish blockBeforeFinish,
            by simpa [List.append_assoc] using allReady⟩
  rcases advance entry.capsule.history [] (discoveryFinish + 1) (by simp)
      (Nat.le_refl _) (by simp) with
    ⟨finish, discoveryBeforeFinish, allReady⟩
  exact ⟨finish, discoveryBeforeFinish, by simpa using allReady⟩

/-- Each accepted history body that remains above GC stays retained for the
recovery handoff. Bodies at or below GC need no pin. -/
theorem accepted_discovered_history_is_retained_above_gc
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    {rootStart discoveryFinish finish requester : Time}
    {entry : ValidatorPinnedRecoveryCapsule (BlockId := BlockId) config}
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (activeEpoch : ∀ time, rootStart ≤ time →
      (timed.execution.trace time).epochActive = true)
    (requesterHeadCurrent : ∀ time, rootStart ≤ time →
      ((timed.execution.trace time).validatorState requester).commitHead.index ≤
        ((timed.execution.trace rootStart).validatorState
          requester).commitHead.index)
    (_rootBeforeDiscovery : rootStart ≤ discoveryFinish)
    (discoveryBeforeFinish : discoveryFinish + 1 ≤ finish)
    (discovered : ∀ block, block ∈ entry.capsule.history →
      BodyObservedOrGcRootAt (timed := timed) rootStart discoveryFinish
        requester block)
    (ready : ∀ block, block ∈ entry.capsule.history →
      ((timed.execution.trace finish).validatorState requester).accepted
            block.reference = true ∨
        block.reference.round ≤
          ((timed.execution.trace finish).validatorState requester).gcRound) :
    ∀ block, block ∈ entry.capsule.history →
      block.reference.round ≤
          ((timed.execution.trace finish).validatorState requester).gcRound ∨
        (((timed.execution.trace finish).validatorState requester).accepted
              block.reference = true ∧
          ((timed.execution.trace finish).validatorState requester).retained
              block.reference = true) := by
  intro block blockMember
  by_cases atRoot : block.reference.round ≤
      ((timed.execution.trace finish).validatorState requester).gcRound
  · exact Or.inl atRoot
  · have aboveGcAtFinish :
        ((timed.execution.trace finish).validatorState requester).gcRound <
          block.reference.round := by omega
    have acceptedAtFinish :
        ((timed.execution.trace finish).validatorState requester).accepted
            block.reference = true := by
      rcases ready block blockMember with accepted | root
      · exact accepted
      · omega
    have observed : BodyObservedBetween (timed := timed) rootStart
        discoveryFinish requester block := by
      rcases discovered block blockMember with localBody | root
      · exact localBody
      · have gcBeforeFinish := validator_gc_round_mono (timed := timed)
          requesterInRange
          (Nat.le_trans (Nat.le_add_right discoveryFinish 1)
            discoveryBeforeFinish)
        omega
    rcases observed with
      ⟨observedAt, rootBeforeObserved, observedBeforeDiscovery, body⟩
    have latchBeforeFinish : observedAt + 1 ≤ finish :=
      Nat.le_trans (Nat.add_le_add_right observedBeforeDiscovery 1)
        discoveryBeforeFinish
    have rootBeforeLatch : rootStart ≤ observedAt + 1 :=
      Nat.le_trans rootBeforeObserved (Nat.le_add_right _ 1)
    have aboveGcAtLatch :
        ((timed.execution.trace (observedAt + 1)).validatorState
          requester).gcRound < block.reference.round := by
      have gcBeforeFinish := validator_gc_round_mono (timed := timed)
        requesterInRange
        latchBeforeFinish
      omega
    have pinAtLatch := sync.localBodyLatchesPin observedAt requester block
      requesterInRange requesterCorrectAvailable
      (activeEpoch (observedAt + 1) rootBeforeLatch) aboveGcAtLatch body
    have pinAtFinish := sync.body_pin_persists_while_head_is_current
      requesterInRange requesterCorrectAvailable latchBeforeFinish pinAtLatch
      (by
        intro time latchBeforeTime _timeBeforeFinish
        exact activeEpoch time (Nat.le_trans rootBeforeLatch latchBeforeTime))
      (by
        intro time _latchBeforeTime timeBeforeFinish
        have gcBeforeFinish := validator_gc_round_mono (timed := timed)
          requesterInRange
          timeBeforeFinish
        omega)
      (by
        intro time latchBeforeTime _timeBeforeFinish
        have rootBeforeTime := Nat.le_trans rootBeforeLatch latchBeforeTime
        have timeHead := requesterHeadCurrent time rootBeforeTime
        have rootBeforeLatchHead :=
          (timed.execution.durableStateMonotone requester rootStart
            (observedAt + 1) requesterInRange rootBeforeLatch).1
        omega)
    have retainedAtFinish := sync.acceptedPinnedBodyIsRetained finish requester
      block.reference _ requesterInRange requesterCorrectAvailable pinAtFinish
      aboveGcAtFinish acceptedAtFinish
    exact Or.inr ⟨acceptedAtFinish, retainedAtFinish⟩

/-- One delivered target installs its above-GC causal closure at one correct
requester. The only alternative is a requester-local commit advance. -/
theorem delivered_target_installs_retained_above_gc_history_or_commit_advance
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    {start targetTime holder requester : Time}
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId} {entry}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (activeEpoch : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (stored : (pins.trace start holder).capsuleAt capsuleKey = some entry)
    (pinned : (pins.trace start holder).pinned capsuleKey = true)
    (startBeforeTarget : start ≤ targetTime)
    (targetBody : ValidatorLocalBlockBodyAt timed targetTime requester
      entry.capsule.targetBlock) :
    ∃ finish,
      start ≤ finish ∧
        (((timed.execution.trace start).validatorState
              requester).commitHead.index <
            ((timed.execution.trace finish).validatorState
              requester).commitHead.index ∨
          (entry.capsule.genesisParents = pins.canonicalGenesisParents ∧
            ∀ block, block ∈ entry.capsule.history →
              block.reference.round ≤
                    ((timed.execution.trace finish).validatorState
                      requester).gcRound ∨
                (((timed.execution.trace finish).validatorState
                      requester).accepted block.reference = true ∧
                  ((timed.execution.trace finish).validatorState
                      requester).retained block.reference = true))) := by
  classical
  by_cases advanced : ∃ finish, start ≤ finish ∧
      ((timed.execution.trace start).validatorState
            requester).commitHead.index <
        ((timed.execution.trace finish).validatorState
          requester).commitHead.index
  · rcases advanced with ⟨finish, startBeforeFinish, headAdvanced⟩
    exact ⟨finish, startBeforeFinish, Or.inl headAdvanced⟩
  · have requesterHeadCurrent : ∀ time, start ≤ time →
        ((timed.execution.trace time).validatorState
            requester).commitHead.index ≤
          ((timed.execution.trace start).validatorState
            requester).commitHead.index := by
      intro time startBeforeTime
      have notAdvanced : ¬
          ((timed.execution.trace start).validatorState
                requester).commitHead.index <
            ((timed.execution.trace time).validatorState
              requester).commitHead.index := by
        intro headAdvanced
        exact advanced ⟨time, startBeforeTime, headAdvanced⟩
      omega
    rcases delivered_target_eventually_discovers_above_gc_history pins sync
        holderInRange holderCorrectAvailable requesterInRange
        requesterCorrectAvailable afterGst activeEpoch stored pinned
        requesterHeadCurrent startBeforeTarget targetBody with
      ⟨discoveryFinish, targetBeforeDiscovery, discovered⟩
    rcases discovered_history_eventually_accepted_or_gc_root pins acceptance
        holderInRange holderCorrectAvailable requesterInRange
        requesterCorrectAvailable stored pinned discovered with
      ⟨finish, discoveryBeforeFinish, ready⟩
    have startBeforeDiscovery : start ≤ discoveryFinish :=
      Nat.le_trans startBeforeTarget targetBeforeDiscovery
    have retained := accepted_discovered_history_is_retained_above_gc sync
      requesterInRange requesterCorrectAvailable activeEpoch
      requesterHeadCurrent startBeforeDiscovery discoveryBeforeFinish
      discovered ready
    have canonical := pins.correctCapsuleUsesCanonicalGenesis start holder
      capsuleKey entry holderInRange holderCorrectAvailable stored
    exact ⟨finish,
      Nat.le_trans startBeforeDiscovery
        (Nat.le_trans (Nat.le_add_right discoveryFinish 1)
          discoveryBeforeFinish),
      Or.inr ⟨canonical, retained⟩⟩

/-- One delivered target installs one required causal reference without
exposing the proof-only capsule history. If GC crosses the reference, the
requester has installed the next commit index. -/
theorem delivered_target_installs_required_reference_or_next_commit
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    {start targetTime holder requester : Time}
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId} {entry}
    {prior : ValidatorCommitHead CommitId}
    {reference : ValidatorBlockRef BlockId}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (activeEpoch : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (stored : (pins.trace start holder).capsuleAt capsuleKey = some entry)
    (pinned : (pins.trace start holder).pinned capsuleKey = true)
    (headAtStart :
      ((timed.execution.trace start).validatorState requester).commitHead = prior)
    (referenceAfterPrior : prior.round < reference.round)
    (path : ValidatorCausalClosureReferenceAboveRound
      (timed.execution.trace start)
      ((timed.execution.trace start).validatorState requester).gcRound
      entry.capsule.targetBlock.reference reference)
    (startBeforeTarget : start ≤ targetTime)
    (targetBody : ValidatorLocalBlockBodyAt timed targetTime requester
      entry.capsule.targetBlock) :
    ∃ finish,
      start ≤ finish ∧
        ((∃ commitId,
            ((timed.execution.trace finish).validatorState
              requester).installedCommitAt (prior.index + 1) = some commitId) ∨
          (((timed.execution.trace finish).validatorState requester).accepted
                reference = true ∧
            ((timed.execution.trace finish).validatorState requester).retained
                reference = true)) := by
  have source := pins.pinned_capsule_is_execution_source holderInRange
    holderCorrectAvailable stored pinned
  rcases pinned_capsule_contains_above_cutoff_target_closure source path with
    ⟨block, blockMember, blockReference⟩
  rcases delivered_target_installs_retained_above_gc_history_or_commit_advance
      pins sync acceptance holderInRange holderCorrectAvailable requesterInRange
      requesterCorrectAvailable afterGst activeEpoch stored pinned
      startBeforeTarget targetBody with
    ⟨finish, startBeforeFinish, headAdvanced | installedHistory⟩
  · have nextInstalled := prefixMap.installedAtOrBelowHead finish requester
      (prior.index + 1) requesterInRange requesterCorrectAvailable (by
        change prior.index + 1 ≤
          ((timed.execution.trace finish).validatorState
            requester).commitHead.index
        rw [headAtStart] at headAdvanced
        omega)
    exact ⟨finish, startBeforeFinish, Or.inl nextInstalled⟩
  · rcases installedHistory.2 block blockMember with blockAtRoot | blockReady
    · rcases required_reference_gc_split timed.execution prefixMap
          requesterInRange requesterCorrectAvailable startBeforeFinish headAtStart
          referenceAfterPrior with referenceAboveGc | nextInstalled
      · rw [blockReference] at blockAtRoot
        omega
      · exact ⟨finish, startBeforeFinish, Or.inl nextInstalled⟩
    · exact ⟨finish, startBeforeFinish, Or.inr (by
        simpa [blockReference] using blockReady)⟩

/-- One delivered target installs its complete above-GC causal closure at one
common finish time. If local commit progress crosses the proof base, the
requester has installed the next commit index instead. -/
theorem delivered_target_installs_above_gc_closure_or_next_commit
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    {start targetTime holder requester : Time}
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId} {entry}
    {prior : ValidatorCommitHead CommitId}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (activeEpoch : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (stored : (pins.trace start holder).capsuleAt capsuleKey = some entry)
    (pinned : (pins.trace start holder).pinned capsuleKey = true)
    (headAtStart :
      ((timed.execution.trace start).validatorState requester).commitHead = prior)
    (startBeforeTarget : start ≤ targetTime)
    (targetBody : ValidatorLocalBlockBodyAt timed targetTime requester
      entry.capsule.targetBlock) :
    ∃ finish,
      start ≤ finish ∧
        ((∃ commitId,
            ((timed.execution.trace finish).validatorState
              requester).installedCommitAt (prior.index + 1) = some commitId) ∨
          ValidatorAcceptedCausalClosureAboveRound
            (timed.execution.trace finish) requester
            ((timed.execution.trace finish).validatorState requester).gcRound
            entry.capsule.targetBlock.reference) := by
  have source := pins.pinned_capsule_is_execution_source holderInRange
    holderCorrectAvailable stored pinned
  rcases delivered_target_installs_retained_above_gc_history_or_commit_advance
      pins sync acceptance holderInRange holderCorrectAvailable requesterInRange
      requesterCorrectAvailable afterGst activeEpoch stored pinned
      startBeforeTarget targetBody with
    ⟨finish, startBeforeFinish, headAdvanced | installedHistory⟩
  · have nextIndexLe : prior.index + 1 ≤
        ((timed.execution.trace finish).validatorState
          requester).commitHead.index := by
      rw [headAtStart] at headAdvanced
      omega
    have nextInstalled := prefixMap.installedAtOrBelowHead finish requester
      (prior.index + 1) requesterInRange requesterCorrectAvailable (by
        simpa [ValidatorWorldState.localCommitIndex] using nextIndexLe)
    exact ⟨finish, startBeforeFinish, Or.inl nextInstalled⟩
  · refine ⟨finish, startBeforeFinish, Or.inr ?_⟩
    intro reference path
    rcases pinned_capsule_contains_later_above_cutoff_target_closure source
        startBeforeFinish path with
      ⟨block, blockMember, blockReference⟩
    rcases installedHistory.2 block blockMember with blockAtRoot | blockReady
    · have referenceAbove :=
          above_cutoff_target_closure_reference_is_above path
      rw [blockReference] at blockAtRoot
      omega
    · simpa [blockReference] using blockReady.1

/-- Commit progress recovery advertises the current pinned tip. One correct
peer then installs the tip's above-GC causal closure or advances its own
commit head. -/
theorem recovery_mode_installs_current_tip_history_or_requester_commit_advance
    {recoveryWait : Time}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    {start holder requester : Time}
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId} {entry}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (differentValidator : requester ≠ holder)
    (afterGst : network.gst ≤ start)
    (activeEpoch : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
      start holder)
    (positiveTip : 0 < ((timed.execution.trace start).validatorState
      holder).highestSignedRound)
    (currentTip :
      ((timed.execution.trace start).validatorState holder).ownBlockAt
          ((timed.execution.trace start).validatorState
              holder).highestSignedRound =
        some entry.capsule.targetBlock.reference)
    (stored : (pins.trace start holder).capsuleAt capsuleKey = some entry)
    (pinned : (pins.trace start holder).pinned capsuleKey = true) :
    ∃ finish,
      start ≤ finish ∧
        (((timed.execution.trace start).validatorState
              requester).commitHead.index <
            ((timed.execution.trace finish).validatorState
              requester).commitHead.index ∨
          (entry.capsule.genesisParents = pins.canonicalGenesisParents ∧
            ∀ block, block ∈ entry.capsule.history →
              block.reference.round ≤
                    ((timed.execution.trace finish).validatorState
                      requester).gcRound ∨
                (((timed.execution.trace finish).validatorState
                      requester).accepted block.reference = true ∧
                  ((timed.execution.trace finish).validatorState
                      requester).retained block.reference = true))) := by
  rcases broadcast.recovery_mode_delivers_pinned_tip_body effects
      holderInRange holderCorrectAvailable requesterInRange
      requesterCorrectAvailable differentValidator afterGst recoveryMode
      positiveTip currentTip stored pinned with
    ⟨packetId, packet, startBeforeDelivery, packetPresent, packetIsProtocol,
      _packetSender, packetReceiver, packetPayload, deliveryOccurs⟩
  have targetBody : ValidatorLocalBlockBodyAt timed packet.deliveredAt requester
      entry.capsule.targetBlock :=
    .delivered packetId packet packetPresent packetIsProtocol packetReceiver
      packetPayload deliveryOccurs
  exact delivered_target_installs_retained_above_gc_history_or_commit_advance
    pins sync acceptance holderInRange holderCorrectAvailable requesterInRange
    requesterCorrectAvailable afterGst activeEpoch stored pinned
    startBeforeDelivery targetBody

end ValidatorRecoveryCapsuleSyncExecution

end Mysticeti
