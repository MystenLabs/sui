/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorCausalRecoveryCapsule
import Mysticeti.ValidatorExecution

namespace Mysticeti

/-! A durable lowest-pending barrier for local recovery.

The implementation processes one received causal bundle in increasing round
order. Before the proposer runs, it stores each usable target as pending. The
locked target is the lowest pending target above the durable signer floor. The
validator finishes that target before it changes the lock.

The maximum initial correct recovery round is only a proof value. No validator
selects it, announces it, or waits to learn the complete correct-validator set.
-/

/-- Durable local state for one recovery proposer. -/
structure LowestPendingRecoveryLocalState where
  signerFloor : Nat
  gcRound : Nat
  ownAt : Nat → Bool
  sentAt : Nat → Bool
  pending : Nat → Bool
  lockedRound : Option Nat

/-- A validator-indexed recovery execution. -/
abbrev LowestPendingRecoveryTrace :=
  Trace (Nat → LowestPendingRecoveryLocalState)

/-- Existing recovery fields map to the main validator execution. `pending` and
`lockedRound` are new durable fields that the Rust implementation must add. -/
structure LowestPendingRecoveryExecutionMapping
    {BlockId CommitId PacketId : Type}
    (worldTrace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (syncTrace : BlockSyncTrace (ValidatorBlock BlockId))
    (recoveryTrace : LowestPendingRecoveryTrace) : Prop where
  signerFloorMatches : ∀ validator time,
    (recoveryTrace time validator).signerFloor =
      ((worldTrace time).validatorState validator).highestSignedRound
  gcRoundMatches : ∀ validator time,
    (recoveryTrace time validator).gcRound =
      ((worldTrace time).validatorState validator).gcRound
  ownRoundMatches : ∀ validator round time,
    (recoveryTrace time validator).ownAt round =
      (((worldTrace time).validatorState validator).ownBlockAt round).isSome
  sentRoundMatches : ∀ validator round time,
    (recoveryTrace time validator).sentAt round =
      ((worldTrace time).validatorState validator).sentOwnBlockAt round
  acceptedBodyMatches : ∀ validator block time,
    (syncTrace time validator).accepted block =
      ((worldTrace time).validatorState validator).accepted block.reference
  retainedBodyMatches : ∀ validator block time,
    (syncTrace time validator).retained block =
      ((worldTrace time).validatorState validator).retained block.reference

/-- Own and sent state in the main validator execution. -/
def ValidatorExecutionOwnAndSentAt
    {BlockId CommitId PacketId : Type}
    (worldTrace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (validator round time : Nat) : Prop :=
  (((worldTrace time).validatorState validator).ownBlockAt round).isSome = true ∧
    ((worldTrace time).validatorState validator).sentOwnBlockAt round = true

def RecoveryOwnAt (trace : LowestPendingRecoveryTrace)
    (validator round time : Nat) : Prop :=
  (trace time validator).ownAt round = true

def RecoverySentAt (trace : LowestPendingRecoveryTrace)
    (validator round time : Nat) : Prop :=
  (trace time validator).sentAt round = true

def RecoveryPendingAt (trace : LowestPendingRecoveryTrace)
    (validator round time : Nat) : Prop :=
  (trace time validator).pending round = true

def RecoveryOwnAndSentAt (trace : LowestPendingRecoveryTrace)
    (validator round time : Nat) : Prop :=
  RecoveryOwnAt trace validator round time ∧
    RecoverySentAt trace validator round time

/-- A recovery completion fact is the same own-and-sent fact in the main
validator execution. -/
theorem recovery_own_and_sent_maps_to_validator_execution
    {BlockId CommitId PacketId : Type}
    {worldTrace : Trace (ValidatorWorldState BlockId CommitId PacketId)}
    {syncTrace : BlockSyncTrace (ValidatorBlock BlockId)}
    {recoveryTrace : LowestPendingRecoveryTrace}
    (mapping : LowestPendingRecoveryExecutionMapping worldTrace syncTrace
      recoveryTrace)
    {validator round time : Nat}
    (done : RecoveryOwnAndSentAt recoveryTrace validator round time) :
    ValidatorExecutionOwnAndSentAt worldTrace validator round time := by
  constructor
  · rw [← mapping.ownRoundMatches validator round time]
    exact done.1
  · rw [← mapping.sentRoundMatches validator round time]
    exact done.2

/-- One accepted capsule has target and parent bytes that the proposer can use.
The parent check uses both retained bytes and the current GC boundary. -/
def CausalCapsuleUsableAt
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (syncTrace : BlockSyncTrace (ValidatorBlock BlockId))
    (recoveryTrace : LowestPendingRecoveryTrace)
    (validator : Nat)
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    (time : Time) : Prop :=
  AllHistoryItemsAcceptedAt syncTrace validator capsule.history time ∧
    AllHistoryItemsRetainedAt syncTrace validator capsule.history time ∧
    ∀ parent, parent ∈ capsule.targetBlock.parents →
      parent.round = 0 ∨
        ∃ parentBlock,
          parentBlock ∈ capsule.history ∧
            parentBlock.reference = parent ∧
            AcceptedAt syncTrace validator parentBlock time ∧
            RetainedAt syncTrace validator parentBlock time ∧
            (recoveryTrace time validator).gcRound < parent.round

/-- Any positive block in an accepted causal bundle can be a recovery target.
This is what prevents a high bundle from skipping all intermediate rounds. -/
def CausalHistoryBlockUsableAt
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (syncTrace : BlockSyncTrace (ValidatorBlock BlockId))
    (recoveryTrace : LowestPendingRecoveryTrace)
    (validator : Nat)
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    (block : ValidatorBlock BlockId)
    (time : Time) : Prop :=
  block ∈ capsule.history ∧
    block.HasQuorumImmediateParents config ∧
    AllHistoryItemsAcceptedAt syncTrace validator capsule.history time ∧
    AllHistoryItemsRetainedAt syncTrace validator capsule.history time ∧
    ∀ parent, parent ∈ block.parents →
      parent.round = 0 ∨
        ∃ parentBlock,
          parentBlock ∈ capsule.history ∧
            parentBlock.reference = parent ∧
            AcceptedAt syncTrace validator parentBlock time ∧
            RetainedAt syncTrace validator parentBlock time ∧
            (recoveryTrace time validator).gcRound < parent.round

/-- The capsule target is one usable history block. -/
theorem target_is_usable_history_block
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {syncTrace : BlockSyncTrace (ValidatorBlock BlockId)}
    {recoveryTrace : LowestPendingRecoveryTrace}
    {validator : Nat}
    {capsule : CausalRecoveryCapsule (BlockId := BlockId) config}
    {time : Time}
    (usable : CausalCapsuleUsableAt syncTrace recoveryTrace validator capsule
      time) :
    CausalHistoryBlockUsableAt syncTrace recoveryTrace validator capsule
      capsule.targetBlock time := by
  refine ⟨capsule.target_and_parents_in_history.1, capsule.targetValid,
    usable.1, usable.2.1, usable.2.2⟩

/-- The initial durable state and protected source data match the static
recovery sources. These facts are local to each correct validator. -/
structure InitialRecoveryExecutionBase
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (syncTrace : BlockSyncTrace (ValidatorBlock BlockId))
    (recoveryTrace : LowestPendingRecoveryTrace)
    (sources : FixedCausalRecoverySources BlockId CommitId config)
    (start : Time) : Prop where
  memberInRange : ∀ validator, validator ∈ sources.validators →
    validator < config.authorityCount
  memberCorrectAvailable : ∀ validator,
    validator ∈ sources.validators →
      faults.correctAvailable validator = true
  everyCorrectAvailableIsMember : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
      validator ∈ sources.validators
  signerFloorMatches : ∀ validator, validator ∈ sources.validators →
    (recoveryTrace start validator).signerFloor =
      (sources.source validator).signerFloor
  gcRoundMatches : ∀ validator, validator ∈ sources.validators →
    (recoveryTrace start validator).gcRound =
      (sources.source validator).gcRound
  positiveFloorIsDurable : ∀ validator,
    validator ∈ sources.validators →
    0 < (sources.source validator).signerFloor →
      RecoveryOwnAt recoveryTrace validator
        (sources.source validator).signerFloor start
  protectedCapsuleAtStart : ∀ holder,
    holder ∈ sources.validators →
    ∀ capsule, (sources.source holder).capsule = some capsule →
      RetainedCorrectCausalHistory config faults syncTrace holder
        capsule.history start
  requestRules : ∀ holder,
    holder ∈ sources.validators →
    ∀ capsule, (sources.source holder).capsule = some capsule →
      ProtectedCausalHistoryRequestRules config faults syncTrace holder
        capsule.history

/-- GC cannot pass the recovery boundary while protected recovery work is
unfinished. This local rule keeps accepted parent bytes usable by the proposer. -/
structure RecoveryGcProtectionRules
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (recoveryTrace : LowestPendingRecoveryTrace)
    (sources : FixedCausalRecoverySources BlockId CommitId config)
    (start : Time) : Prop where
  gcDoesNotAdvance : ∀ validator,
    validator ∈ sources.validators →
    ∀ time, start ≤ time →
      (recoveryTrace time validator).gcRound ≤
        (sources.source validator).gcRound

/-- The atomic bridge from accepted causal bytes to the durable proposal queue.

If the signer has not reached one usable history block, the same local batch
action records its round as pending before the proposer runs. The batch visits
history blocks in increasing round order. The separate proposal-history rule
records that each later signer-floor increase used such a pending round. -/
structure CausalAcceptanceBarrierRules
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (syncTrace : BlockSyncTrace (ValidatorBlock BlockId))
    (recoveryTrace : LowestPendingRecoveryTrace) : Prop where
  historyRoundAboveFloorIsQueued : ∀ validator
      (capsule : CausalRecoveryCapsule (BlockId := BlockId) config) block time,
    CausalHistoryBlockUsableAt syncTrace recoveryTrace validator capsule block
      time →
    (recoveryTrace time validator).signerFloor < block.reference.round →
      RecoveryPendingAt recoveryTrace validator block.reference.round time

/-- Durable queue and lock rules for one validator. -/
structure DurableLowestPendingRules
    (trace : LowestPendingRecoveryTrace) : Prop where
  ownPersists : ∀ validator round earlier later,
    earlier ≤ later →
    RecoveryOwnAt trace validator round earlier →
      RecoveryOwnAt trace validator round later

  sentPersists : ∀ validator round earlier later,
    earlier ≤ later →
    RecoverySentAt trace validator round earlier →
      RecoverySentAt trace validator round later
  sentRequiresOwn : ∀ validator round time,
    RecoverySentAt trace validator round time →
      RecoveryOwnAt trace validator round time
  pendingPersistsUntilOwn : ∀ validator round earlier later,
    earlier ≤ later →
    RecoveryPendingAt trace validator round earlier →
    ¬RecoveryOwnAt trace validator round later →
      RecoveryPendingAt trace validator round later
  lockedRoundIsPending : ∀ validator round time,
    (trace time validator).lockedRound = some round →
      RecoveryPendingAt trace validator round time
  lockedRoundIsAboveFloor : ∀ validator round time,
    (trace time validator).lockedRound = some round →
      (trace time validator).signerFloor < round
  lockedRoundIsLowest : ∀ validator round time,
    (trace time validator).lockedRound = some round →
    ∀ other,
      RecoveryPendingAt trace validator other time →
      (trace time validator).signerFloor < other →
        round ≤ other
  lockIsStickyUntilOwn : ∀ validator round earlier later,
    earlier ≤ later →
    (trace earlier validator).lockedRound = some round →
    ¬RecoveryOwnAt trace validator round later →
      (trace later validator).lockedRound = some round
  pendingRoundCannotBeCrossed : ∀ validator round earlier later,
    earlier ≤ later →
    RecoveryPendingAt trace validator round earlier →
    round ≤ (trace later validator).signerFloor →
      RecoveryOwnAt trace validator round later

/-- Each signer-floor increase after recovery start has an earlier pending
record for that exact round. This is the direct local invariant of queue-before-
proposal processing. It is needed until the main action trace records the new
queue transition. -/
structure RecoveryProposalHistoryRules
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (trace : LowestPendingRecoveryTrace)
    (sources : FixedCausalRecoverySources BlockId CommitId config)
    (start : Time) : Prop where
  crossedRoundWasPending : ∀ validator,
    validator ∈ sources.validators →
    ∀ round time,
      start ≤ time →
      (sources.source validator).signerFloor < round →
      round ≤ (trace time validator).signerFloor →
        ∃ queuedAt,
          queuedAt ≤ time ∧
            RecoveryPendingAt trace validator round queuedAt

namespace DurableLowestPendingRules

/-- Own-and-sent completion persists. -/
theorem own_and_sent_persists
    {trace : LowestPendingRecoveryTrace}
    (rules : DurableLowestPendingRules trace)
    {validator round earlier later : Nat}
    (earlierBeforeLater : earlier ≤ later)
    (done : RecoveryOwnAndSentAt trace validator round earlier) :
    RecoveryOwnAndSentAt trace validator round later := by
  exact ⟨rules.ownPersists validator round earlier later earlierBeforeLater done.1,
    rules.sentPersists validator round earlier later earlierBeforeLater done.2⟩

/-- A pending round cannot be overtaken without an own block at that round. -/
theorem no_pending_round_overtake
    {trace : LowestPendingRecoveryTrace}
    (rules : DurableLowestPendingRules trace)
    {validator round earlier later : Nat}
    (earlierBeforeLater : earlier ≤ later)
    (pending : RecoveryPendingAt trace validator round earlier)
    (crossed : round ≤ (trace later validator).signerFloor) :
    RecoveryOwnAt trace validator round later :=
  rules.pendingRoundCannotBeCrossed validator round earlier later
    earlierBeforeLater pending crossed

end DurableLowestPendingRules

/-- Fair local proposal work eventually completes each durable pending round.
This is a single-validator task rule. It does not assume a quorum layer or a
network result. The lowest and sticky lock rules give an implementation of this
contract because there are only finitely many lower natural-number rounds. -/
structure FairLowestPendingProposalActions
    {CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (trace : LowestPendingRecoveryTrace) : Prop where
  pendingRoundEventuallyCompletes : ∀ validator round start,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    RecoveryPendingAt trace validator round start →
      ∃ finish,
        start ≤ finish ∧
          RecoveryOwnAndSentAt trace validator round finish

/-- Fair local genesis work produces round one without an authored round-zero
block. -/
structure FairCanonicalGenesisActions
    {CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (trace : LowestPendingRecoveryTrace) : Prop where
  genesisEventuallyProducesRoundOne : ∀ validator start,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (trace start validator).signerFloor = 0 →
    (trace start validator).gcRound = 0 →
      ∃ finish,
        start ≤ finish ∧
          RecoveryOwnAndSentAt trace validator 1 finish

/-- Fair restart work sends a durable highest own block if a crash happened
after persistence and before its first send. -/
structure FairDurableOwnBlockActions
    {CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (trace : LowestPendingRecoveryTrace) : Prop where
  durableOwnBlockEventuallySent : ∀ validator round start,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    RecoveryOwnAt trace validator round start →
      ∃ finish,
        start ≤ finish ∧
          RecoveryOwnAndSentAt trace validator round finish

/-- The maximum initial recovery capsule has parent rounds that are usable above
each correct validator's initial GC boundary. -/
theorem maximum_capsule_parent_round_usable_for_member
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (sources : FixedCausalRecoverySources BlockId CommitId config)
    {holder validator : Nat}
    (holderMaximum :
      sources.sourceRound holder = sources.maximumRound)
    (validatorMember : validator ∈ sources.validators)
    {capsule : CausalRecoveryCapsule (BlockId := BlockId) config}
    (capsulePresent : (sources.source holder).capsule = some capsule)
    {parent : ValidatorBlockRef BlockId}
    (parentMember : parent ∈ capsule.targetBlock.parents) :
    parent.round = 0 ∨
      (sources.source validator).gcRound < parent.round := by
  have capsuleRound : capsule.targetRound = sources.maximumRound := by
    unfold FixedCausalRecoverySources.sourceRound at holderMaximum
    simp [InitialRecoverySource.recoveryRound, capsulePresent] at holderMaximum
    exact holderMaximum
  have parentImmediate := capsule.targetValid.2.1 parent parentMember
  have validatorAtMostMaximum :=
    sources.member_round_le_maximum validatorMember
  have validatorWindow := (sources.source validator).recovery_gc_window
  rcases validatorWindow with genesisWindow | aboveGcWindow
  · rcases genesisWindow with ⟨_, validatorGc⟩
    by_cases parentGenesis : parent.round = 0
    · exact Or.inl parentGenesis
    · right
      omega
  · right
    change capsule.targetBlock.reference.round = sources.maximumRound at capsuleRound
    change (sources.source validator).recoveryRound ≤ sources.maximumRound at validatorAtMostMaximum
    omega

/-- One correct validator eventually owns and sends the ghost maximum initial
recovery round. No common target is an input. -/
theorem member_eventually_owns_maximum_initial_recovery_round
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlock BlockId)) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {syncTrace : BlockSyncTrace (ValidatorBlock BlockId)}
    {recoveryTrace : LowestPendingRecoveryTrace}
    {sources : FixedCausalRecoverySources BlockId CommitId config}
    {start : Time}
    (afterGst : network.gst ≤ start)
    (storage : RecoveryBodyStorageRules syncTrace)
    (requests : FairProtectedRequestActions config faults protocolPacket syncTrace)
    (serving : FairProtectedServeActions config faults protocolPacket syncTrace)
    (accepting : FairProtectedAcceptActions config faults protocolPacket syncTrace)
    (base : InitialRecoveryExecutionBase config faults syncTrace recoveryTrace
      sources start)
    (gcRules : RecoveryGcProtectionRules recoveryTrace sources start)
    (barrier : CausalAcceptanceBarrierRules (config := config) syncTrace
      recoveryTrace)
    (localRules : DurableLowestPendingRules recoveryTrace)
    (proposalHistory : RecoveryProposalHistoryRules recoveryTrace sources start)
    (proposalFair : FairLowestPendingProposalActions faults recoveryTrace)
    (genesisFair : FairCanonicalGenesisActions faults recoveryTrace)
    (resendFair : FairDurableOwnBlockActions faults recoveryTrace)
    {validator : Nat}
    (validatorMember : validator ∈ sources.validators) :
    ∃ finish,
      start ≤ finish ∧
        RecoveryOwnAndSentAt recoveryTrace validator sources.maximumRound
          finish := by
  obtain ⟨holder, holderMember, holderMaximum⟩ :=
    sources.maximum_round_has_owner
  cases capsulePresent : (sources.source holder).capsule with
  | none =>
      have maximumIsOne : sources.maximumRound = 1 := by
        rw [← holderMaximum]
        simp [FixedCausalRecoverySources.sourceRound,
          InitialRecoverySource.recoveryRound, capsulePresent]
      have floorAtMostOne : (sources.source validator).signerFloor ≤ 1 := by
        have floorLeSource :=
          (sources.source validator).signer_floor_le_recovery_round
        have sourceLeMaximum := sources.member_round_le_maximum validatorMember
        change (sources.source validator).recoveryRound ≤
          sources.maximumRound at sourceLeMaximum
        omega
      by_cases floorZero : (sources.source validator).signerFloor = 0
      · have traceFloorZero :
            (recoveryTrace start validator).signerFloor = 0 := by
          rw [base.signerFloorMatches validator validatorMember]
          exact floorZero
        have sourceGcZero : (sources.source validator).gcRound = 0 := by
          cases localCapsule : (sources.source validator).capsule with
          | none => exact
              ((sources.source validator).noCapsuleOnlyAtGenesis localCapsule).2
          | some capsule =>
              have localRoundAtMostOne :=
                sources.member_round_le_maximum validatorMember
              have localPositive := capsule.targetPositive
              have localWindow :=
                (sources.source validator).localGcWindow capsule localCapsule
              simp [FixedCausalRecoverySources.sourceRound,
                InitialRecoverySource.recoveryRound, localCapsule,
                maximumIsOne] at localRoundAtMostOne
              rcases localWindow with window | window
              · exact window.2
              · omega
        have traceGcZero : (recoveryTrace start validator).gcRound = 0 := by
          rw [base.gcRoundMatches validator validatorMember]
          exact sourceGcZero
        rcases genesisFair.genesisEventuallyProducesRoundOne validator start
            (base.memberInRange validator validatorMember)
            (base.memberCorrectAvailable validator validatorMember)
            traceFloorZero traceGcZero with
          ⟨finish, startBeforeFinish, produced⟩
        exact ⟨finish, startBeforeFinish, by simpa [maximumIsOne] using produced⟩
      · have floorPositive : 0 < (sources.source validator).signerFloor := by
          omega
        have floorIsOne : (sources.source validator).signerFloor = 1 := by
          omega
        have durable := base.positiveFloorIsDurable validator validatorMember
          floorPositive
        rcases resendFair.durableOwnBlockEventuallySent validator
            (sources.source validator).signerFloor start
            (base.memberInRange validator validatorMember)
            (base.memberCorrectAvailable validator validatorMember) durable with
          ⟨finish, startBeforeFinish, produced⟩
        exact ⟨finish, startBeforeFinish, by
          simpa [maximumIsOne, floorIsOne] using produced⟩
  | some capsule =>
      have sourceHistory := base.protectedCapsuleAtStart holder holderMember
        capsule capsulePresent
      have sourceRequests := base.requestRules holder holderMember capsule
        capsulePresent
      rcases retained_causal_history_eventually_accepted storage.toBlockSyncStorageRules
          requests serving accepting sourceHistory sourceRequests afterGst
          (base.memberInRange validator validatorMember)
          (base.memberCorrectAvailable validator validatorMember) with
        ⟨acceptedAt, startBeforeAccepted, allAccepted⟩
      have allRetained := accepted_history_is_retained storage allAccepted
      have targetRoundIsMaximum : capsule.targetRound = sources.maximumRound := by
        unfold FixedCausalRecoverySources.sourceRound at holderMaximum
        simp [InitialRecoverySource.recoveryRound, capsulePresent] at holderMaximum
        exact holderMaximum
      have capsuleUsable : CausalCapsuleUsableAt syncTrace recoveryTrace
          validator capsule acceptedAt := by
        refine ⟨allAccepted, allRetained, ?_⟩
        intro parent parentMember
        have initiallyUsable :=
          maximum_capsule_parent_round_usable_for_member sources holderMaximum
            validatorMember capsulePresent parentMember
        rcases initiallyUsable with parentGenesis | parentAboveInitialGc
        · exact Or.inl parentGenesis
        · apply Or.inr
          rcases capsule.targetParentsInHistory parent parentMember with
            parentGenesis | ⟨parentBlock, parentInHistory, parentReference⟩
          · have parentRoundZero :=
              capsule.genesisParentsAreRoundZero parent parentGenesis
            omega
          refine ⟨parentBlock, parentInHistory, parentReference,
            allAccepted parentBlock parentInHistory,
            allRetained parentBlock parentInHistory, ?_⟩
          have gcBound := gcRules.gcDoesNotAdvance validator validatorMember
            acceptedAt startBeforeAccepted
          omega
      have targetHistoryUsable := target_is_usable_history_block capsuleUsable
      by_cases targetAboveFloor :
          (recoveryTrace acceptedAt validator).signerFloor < capsule.targetRound
      · have pending := barrier.historyRoundAboveFloorIsQueued validator capsule
          capsule.targetBlock acceptedAt targetHistoryUsable targetAboveFloor
        rcases proposalFair.pendingRoundEventuallyCompletes validator
            capsule.targetRound acceptedAt
            (base.memberInRange validator validatorMember)
            (base.memberCorrectAvailable validator validatorMember) pending with
          ⟨finish, acceptedBeforeFinish, produced⟩
        exact ⟨finish, Nat.le_trans startBeforeAccepted acceptedBeforeFinish,
          by simpa [targetRoundIsMaximum] using produced⟩
      · have targetAtOrBelowFloor :
            capsule.targetRound ≤
              (recoveryTrace acceptedAt validator).signerFloor := by
          omega
        have initialFloorLeTarget :
            (sources.source validator).signerFloor ≤ capsule.targetRound := by
          have floorLeSource :=
            (sources.source validator).signer_floor_le_recovery_round
          have sourceLeMaximum := sources.member_round_le_maximum validatorMember
          change (sources.source validator).recoveryRound ≤
            sources.maximumRound at sourceLeMaximum
          omega
        by_cases targetIsInitialFloor :
            capsule.targetRound = (sources.source validator).signerFloor
        · have initialFloorPositive :
              0 < (sources.source validator).signerFloor := by
            have targetPositive : 0 < capsule.targetRound :=
              capsule.targetPositive
            omega
          have initialFloorIsMaximum :
              (sources.source validator).signerFloor = sources.maximumRound := by
            omega
          have durable := base.positiveFloorIsDurable validator validatorMember
            initialFloorPositive
          rcases resendFair.durableOwnBlockEventuallySent validator
              (sources.source validator).signerFloor start
              (base.memberInRange validator validatorMember)
              (base.memberCorrectAvailable validator validatorMember) durable with
            ⟨finish, startBeforeFinish, produced⟩
          exact ⟨finish, startBeforeFinish, by
            simpa [initialFloorIsMaximum] using produced⟩
        · have targetAboveInitialFloor :
              (sources.source validator).signerFloor < capsule.targetRound := by
            omega
          obtain ⟨queuedAt, queuedBeforeAccepted, pending⟩ :=
            proposalHistory.crossedRoundWasPending validator validatorMember
              capsule.targetRound acceptedAt startBeforeAccepted
              targetAboveInitialFloor targetAtOrBelowFloor
          have ownAtAccepted := localRules.no_pending_round_overtake
            queuedBeforeAccepted pending targetAtOrBelowFloor
          rcases resendFair.durableOwnBlockEventuallySent validator
              capsule.targetRound acceptedAt
              (base.memberInRange validator validatorMember)
              (base.memberCorrectAvailable validator validatorMember)
              ownAtAccepted with
            ⟨finish, acceptedBeforeFinish, produced⟩
          exact ⟨finish, Nat.le_trans startBeforeAccepted acceptedBeforeFinish,
            by simpa [targetRoundIsMaximum] using produced⟩

/-- Pointwise persistence combines the finite correct-validator completion
times. -/
theorem eventually_all_list_members
    (members : List Nat)
    (predicate : Nat → Time → Prop)
    (start : Time)
    (persists : ∀ validator earlier later,
      earlier ≤ later → predicate validator earlier →
        predicate validator later)
    (eventually : ∀ validator, validator ∈ members →
      ∃ finish, start ≤ finish ∧ predicate validator finish) :
    ∃ finish,
      start ≤ finish ∧
        ∀ validator, validator ∈ members → predicate validator finish := by
  induction members with
  | nil =>
      refine ⟨start, Nat.le_refl _, ?_⟩
      intro validator validatorMember
      simp at validatorMember
  | cons head tail inductionHypothesis =>
      have tailEventually : ∀ validator, validator ∈ tail →
          ∃ finish, start ≤ finish ∧ predicate validator finish := by
        intro validator validatorMember
        exact eventually validator (List.mem_cons_of_mem head validatorMember)
      rcases inductionHypothesis tailEventually with
        ⟨tailFinish, startBeforeTail, tailDone⟩
      rcases eventually head List.mem_cons_self with
        ⟨headFinish, startBeforeHead, headDone⟩
      let finish := Nat.max headFinish tailFinish
      refine ⟨finish,
        Nat.le_trans startBeforeHead (Nat.le_max_left _ _), ?_⟩
      intro validator validatorMember
      rcases List.mem_cons.mp validatorMember with rfl | inTail
      · exact persists _ headFinish finish (Nat.le_max_left _ _) headDone
      · exact persists validator tailFinish finish (Nat.le_max_right _ _)
          (tailDone validator inTail)

/-- All correct, available validators eventually own and send the ghost maximum
initial recovery round. The theorem does not take a common target, a parent
quorum layer, or completed synchronization as an input. -/
theorem maximum_initial_recovery_round_eventually_owned_by_all
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlock BlockId)) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {syncTrace : BlockSyncTrace (ValidatorBlock BlockId)}
    {recoveryTrace : LowestPendingRecoveryTrace}
    {sources : FixedCausalRecoverySources BlockId CommitId config}
    {start : Time}
    (afterGst : network.gst ≤ start)
    (storage : RecoveryBodyStorageRules syncTrace)
    (requests : FairProtectedRequestActions config faults protocolPacket syncTrace)
    (serving : FairProtectedServeActions config faults protocolPacket syncTrace)
    (accepting : FairProtectedAcceptActions config faults protocolPacket syncTrace)
    (base : InitialRecoveryExecutionBase config faults syncTrace recoveryTrace
      sources start)
    (gcRules : RecoveryGcProtectionRules recoveryTrace sources start)
    (barrier : CausalAcceptanceBarrierRules (config := config) syncTrace
      recoveryTrace)
    (localRules : DurableLowestPendingRules recoveryTrace)
    (proposalHistory : RecoveryProposalHistoryRules recoveryTrace sources start)
    (proposalFair : FairLowestPendingProposalActions faults recoveryTrace)
    (genesisFair : FairCanonicalGenesisActions faults recoveryTrace)
    (resendFair : FairDurableOwnBlockActions faults recoveryTrace) :
    ∃ finish,
      start ≤ finish ∧
      ∀ validator,
        validator < config.authorityCount →
        faults.correctAvailable validator = true →
          RecoveryOwnAndSentAt recoveryTrace validator sources.maximumRound
            finish := by
  have eachMember : ∀ validator, validator ∈ sources.validators →
      ∃ finish,
        start ≤ finish ∧
          RecoveryOwnAndSentAt recoveryTrace validator sources.maximumRound
            finish := by
    intro validator validatorMember
    exact member_eventually_owns_maximum_initial_recovery_round afterGst storage
      requests serving accepting base gcRules barrier localRules proposalHistory
      proposalFair genesisFair resendFair validatorMember
  rcases eventually_all_list_members sources.validators
      (fun validator time =>
        RecoveryOwnAndSentAt recoveryTrace validator sources.maximumRound time)
      start
      (fun validator earlier later earlierBeforeLater done =>
        localRules.own_and_sent_persists earlierBeforeLater done)
      eachMember with ⟨finish, startBeforeFinish, allMembersDone⟩
  exact ⟨finish, startBeforeFinish, by
    intro validator validatorInRange validatorCorrect
    exact allMembersDone validator
      (base.everyCorrectAvailableIsMember validator validatorInRange
        validatorCorrect)⟩

/-- The same convergence result in the main validator execution trace. The
mapping keeps only the proposed pending queue and lock outside the current main
state record. -/
theorem maximum_initial_recovery_round_eventually_owned_in_validator_execution
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlock BlockId)) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {worldTrace : Trace (ValidatorWorldState BlockId CommitId PacketId)}
    {syncTrace : BlockSyncTrace (ValidatorBlock BlockId)}
    {recoveryTrace : LowestPendingRecoveryTrace}
    {sources : FixedCausalRecoverySources BlockId CommitId config}
    {start : Time}
    (afterGst : network.gst ≤ start)
    (storage : RecoveryBodyStorageRules syncTrace)
    (requests : FairProtectedRequestActions config faults protocolPacket syncTrace)
    (serving : FairProtectedServeActions config faults protocolPacket syncTrace)
    (accepting : FairProtectedAcceptActions config faults protocolPacket syncTrace)
    (base : InitialRecoveryExecutionBase config faults syncTrace recoveryTrace
      sources start)
    (gcRules : RecoveryGcProtectionRules recoveryTrace sources start)
    (barrier : CausalAcceptanceBarrierRules (config := config) syncTrace
      recoveryTrace)
    (localRules : DurableLowestPendingRules recoveryTrace)
    (proposalHistory : RecoveryProposalHistoryRules recoveryTrace sources start)
    (proposalFair : FairLowestPendingProposalActions faults recoveryTrace)
    (genesisFair : FairCanonicalGenesisActions faults recoveryTrace)
    (resendFair : FairDurableOwnBlockActions faults recoveryTrace)
    (mapping : LowestPendingRecoveryExecutionMapping worldTrace syncTrace
      recoveryTrace) :
    ∃ finish,
      start ≤ finish ∧
      ∀ validator,
        validator < config.authorityCount →
        faults.correctAvailable validator = true →
          ValidatorExecutionOwnAndSentAt worldTrace validator
            sources.maximumRound finish := by
  rcases maximum_initial_recovery_round_eventually_owned_by_all afterGst storage
      requests serving accepting base gcRules barrier localRules proposalHistory
      proposalFair genesisFair resendFair with
    ⟨finish, startBeforeFinish, allDone⟩
  exact ⟨finish, startBeforeFinish, by
    intro validator validatorInRange validatorCorrect
    exact recovery_own_and_sent_maps_to_validator_execution mapping
      (allDone validator validatorInRange validatorCorrect)⟩

end Mysticeti
