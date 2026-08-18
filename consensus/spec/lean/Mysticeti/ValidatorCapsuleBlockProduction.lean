/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorBlockProduction
import Mysticeti.ValidatorLowestPendingRecovery

namespace Mysticeti

/-! Causal recovery as the base for exact-next block production.

The lowest-pending recovery proof selects the maximum initial recovery round
only as a ghost value. A validator does not know or announce that value. This
module maps the local completion facts to the main validator trace, derives
acceptance of the resulting blocks, and starts exact-next induction at that
complete round.

The only extra proposal rule records local provenance. Each recovery block uses
a valid parent list from an accepted causal bundle. The proof then derives
parent delivery from protected source data. It does not take parent availability,
a common block layer, or future block production as an input.
-/

/-- Every correct, available validator has accepted every protected positive
capsule history. A source without a positive capsule has no history to sync. -/
def EveryRecoveryHistoryAcceptedAt
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (syncTrace : BlockSyncTrace (ValidatorBlock BlockId))
    (sources : FixedCausalRecoverySources BlockId CommitId config)
    (time : Time) : Prop :=
  ∀ observer,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    ∀ holder,
      holder ∈ sources.validators →
      ∀ capsule,
        (sources.source holder).capsule = some capsule →
        AllHistoryItemsAcceptedAt syncTrace observer capsule.history time

/-- A recovery block records the causal source of each non-genesis parent.

This is a one-validator proposal effect. It says which locally accepted bodies
the proposer used. It does not say that another validator has those bodies. -/
structure RecoveryBlockCausalProvenance
    {CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (worldTrace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId))
    (sources :
      FixedCausalRecoverySources ScheduledBlockId CommitId config) : Prop where
  ownBlockHasCausalParents : ∀ time author round,
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ((worldTrace time).validatorState author).ownBlockAt round =
        some (scheduledBlockRef author round) →
    ∃ block,
      (worldTrace time).blockCatalog (author, round) = some block ∧
      block.reference = scheduledBlockRef author round ∧
      block.HasQuorumImmediateParents config ∧
      ∀ parent,
        parent ∈ block.parents →
        (parent.round = 0 ∧
          parent = scheduledBlockRef parent.author 0 ∧
          parent.author < config.authorityCount ∧
          faults.correctAvailable parent.author = true) ∨
        ∃ holder capsule parentBody,
          holder ∈ sources.validators ∧
          (sources.source holder).capsule = some capsule ∧
          parentBody ∈ capsule.history ∧
          parentBody.reference = parent
  /-- Canonical genesis parents are local permanent data. -/
  correctGenesisIsRetained : ∀ time observer author,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ((worldTrace time).validatorState observer).retained
      (scheduledBlockRef author 0) = true

/-- The accepted-body trace and the main validator trace use the same local
accepted and retained block facts. -/
def RecoveryBodyTraceMatchesWorld
    {CommitId PacketId : Type}
    (worldTrace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId))
    (syncTrace : BlockSyncTrace (ValidatorBlock ScheduledBlockId)) : Prop :=
  ∀ time validator block,
    (AcceptedAt syncTrace validator block time ↔
      ((worldTrace time).validatorState validator).accepted
        block.reference = true) ∧
    (RetainedAt syncTrace validator block time ↔
      ((worldTrace time).validatorState validator).retained
        block.reference = true)

/-- One parent body is present, accepted, and not obsolete at one validator. -/
def RecoveryParentReadyAt
    {BlockId CommitId : Type}
    (state : ValidatorLocalState BlockId CommitId)
    (parent : ValidatorBlockRef BlockId) : Prop :=
  state.accepted parent = true ∧
    state.retained parent = true ∧
    (parent.round = 0 ∨ state.gcRound < parent.round)

/-- The ghost maximum recovery round is positive and its immediate parents are
above each correct validator's protected GC boundary, unless they are genesis. -/
theorem maximum_recovery_parent_is_gc_usable
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {worldTrace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {syncTrace : BlockSyncTrace (ValidatorBlock ScheduledBlockId)}
    {recoveryTrace : LowestPendingRecoveryTrace}
    {sources :
      FixedCausalRecoverySources ScheduledBlockId CommitId config}
    {start time observer : Time}
    (base : InitialRecoveryExecutionBase config faults syncTrace recoveryTrace
      sources start)
    (gcRules : RecoveryGcProtectionRules recoveryTrace sources start)
    (mapping : LowestPendingRecoveryExecutionMapping worldTrace syncTrace
      recoveryTrace)
    (startBeforeTime : start ≤ time)
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    {block : ValidatorBlock ScheduledBlockId}
    (atMaximum : block.reference.round = sources.maximumRound)
    (valid : block.HasQuorumImmediateParents config)
    {parent : ValidatorBlockRef ScheduledBlockId}
    (parentMember : parent ∈ block.parents) :
    parent.round = 0 ∨
      ((worldTrace time).validatorState observer).gcRound < parent.round := by
  have observerMember := base.everyCorrectAvailableIsMember observer
    observerInRange observerCorrect
  have sourceAtMostMaximum := sources.member_round_le_maximum observerMember
  have localWindow := (sources.source observer).recovery_gc_window
  have parentImmediate := valid.2.1 parent parentMember
  have gcBound := gcRules.gcDoesNotAdvance observer observerMember time
    startBeforeTime
  have gcMapping := mapping.gcRoundMatches observer time
  rcases localWindow with genesisWindow | aboveGcWindow
  · rcases genesisWindow with ⟨sourceIsOne, initialGcZero⟩
    by_cases parentGenesis : parent.round = 0
    · exact Or.inl parentGenesis
    · right
      change (sources.source observer).recoveryRound ≤
        sources.maximumRound at sourceAtMostMaximum
      rw [← gcMapping]
      omega
  · right
    change (sources.source observer).recoveryRound ≤
      sources.maximumRound at sourceAtMostMaximum
    rw [← gcMapping]
    omega

/-- Causal provenance plus completed history sync gives one usable parent at
one correct observer. -/
theorem recovery_block_parent_is_ready
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {worldTrace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {syncTrace : BlockSyncTrace (ValidatorBlock ScheduledBlockId)}
    {recoveryTrace : LowestPendingRecoveryTrace}
    {sources :
      FixedCausalRecoverySources ScheduledBlockId CommitId config}
    {start time observer author : Time}
    (bodyStorage : RecoveryBodyStorageRules syncTrace)
    (base : InitialRecoveryExecutionBase config faults syncTrace recoveryTrace
      sources start)
    (gcRules : RecoveryGcProtectionRules recoveryTrace sources start)
    (mapping : LowestPendingRecoveryExecutionMapping worldTrace syncTrace
      recoveryTrace)
    (exactRules : ExactNextBlockProductionRules config faults worldTrace)
    (genesis : CanonicalGenesisRules config faults worldTrace)
    (provenance : RecoveryBlockCausalProvenance config faults worldTrace sources)
    (startBeforeTime : start ≤ time)
    (historiesAccepted :
      EveryRecoveryHistoryAcceptedAt faults syncTrace sources time)
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true)
    {block : ValidatorBlock ScheduledBlockId}
    (blockReference :
      block.reference = scheduledBlockRef author sources.maximumRound)
    (blockValid : block.HasQuorumImmediateParents config)
    (parentProvenance : ∀ parent,
      parent ∈ block.parents →
      (parent.round = 0 ∧
        parent = scheduledBlockRef parent.author 0 ∧
        parent.author < config.authorityCount ∧
        faults.correctAvailable parent.author = true) ∨
      ∃ holder capsule parentBody,
        holder ∈ sources.validators ∧
        (sources.source holder).capsule = some capsule ∧
        parentBody ∈ capsule.history ∧
        parentBody.reference = parent)
    {parent : ValidatorBlockRef ScheduledBlockId}
    (parentMember : parent ∈ block.parents) :
    RecoveryParentReadyAt ((worldTrace time).validatorState observer) parent := by
  have gcUsable := maximum_recovery_parent_is_gc_usable base gcRules mapping
    startBeforeTime observerInRange observerCorrect
    (block := block) (by rw [blockReference]; rfl) blockValid parentMember
  rcases parentProvenance parent parentMember with
    ⟨parentGenesis, parentCanonical, parentAuthorInRange, parentAuthorCorrect⟩ |
      ⟨holder, capsule, parentBody, holderMember, capsulePresent,
        parentInHistory, parentReference⟩
  · have representative := genesis.accepted time observer parent.author
      observerInRange observerCorrect parentAuthorInRange parentAuthorCorrect
    have parentAccepted := exactRules.representativeIsAccepted time observer
      parent.author 0 observerInRange observerCorrect parentAuthorInRange
      parentAuthorCorrect representative
    rw [parentCanonical]
    refine ⟨parentAccepted,
      provenance.correctGenesisIsRetained time observer parent.author
        observerInRange observerCorrect parentAuthorInRange parentAuthorCorrect, ?_⟩
    · exact Or.inl rfl
  · have bodyAccepted := historiesAccepted observer observerInRange
      observerCorrect holder holderMember capsule capsulePresent parentBody
      parentInHistory
    have bodyRetained := bodyStorage.acceptedBodyIsRetained observer parentBody
      time bodyAccepted
    refine ⟨?_, ?_, gcUsable⟩
    · rw [← parentReference, ← mapping.acceptedBodyMatches observer parentBody
        time]
      exact bodyAccepted
    · rw [← parentReference, ← mapping.retainedBodyMatches observer parentBody
        time]
      exact bodyRetained

/-- Accepted protected histories persist. -/
theorem every_recovery_history_acceptance_persists
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {syncTrace : BlockSyncTrace (ValidatorBlock BlockId)}
    {sources : FixedCausalRecoverySources BlockId CommitId config}
    (storage : BlockSyncStorageRules syncTrace)
    {earlier later : Time}
    (earlierBeforeLater : earlier ≤ later)
    (accepted : EveryRecoveryHistoryAcceptedAt faults syncTrace sources earlier) :
    EveryRecoveryHistoryAcceptedAt faults syncTrace sources later := by
  intro observer observerInRange observerCorrect holder holderMember capsule
      capsulePresent block blockMember
  exact storage.acceptedMonotone observer block earlier later earlierBeforeLater
    (accepted observer observerInRange observerCorrect holder holderMember capsule
      capsulePresent block blockMember)

/-- Protected retry, service, and acceptance actions deliver every correct
source history to every correct, available validator. -/
theorem every_recovery_history_eventually_accepted
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
    (requests : FairProtectedRequestActions config faults protocolPacket
      syncTrace)
    (serving : FairProtectedServeActions config faults protocolPacket
      syncTrace)
    (accepting : FairProtectedAcceptActions config faults protocolPacket
      syncTrace)
    (base : InitialRecoveryExecutionBase config faults syncTrace
      recoveryTrace sources start) :
    ∃ finish,
      start ≤ finish ∧
      EveryRecoveryHistoryAcceptedAt faults syncTrace sources finish := by
  let observerDone := fun observer time =>
    start ≤ time ∧
      ∀ holder,
        holder ∈ sources.validators →
        ∀ capsule,
          (sources.source holder).capsule = some capsule →
          AllHistoryItemsAcceptedAt syncTrace observer capsule.history time
  have observerDonePersists : ∀ observer earlier later,
      earlier ≤ later → observerDone observer earlier →
        observerDone observer later := by
    intro observer earlier later earlierBeforeLater doneEarlier
    refine ⟨Nat.le_trans doneEarlier.1 earlierBeforeLater, ?_⟩
    intro holder holderMember capsule capsulePresent block blockMember
    exact storage.acceptedMonotone observer block earlier later
      earlierBeforeLater
      (doneEarlier.2 holder holderMember capsule capsulePresent block blockMember)
  have eachObserver : ∀ observer,
      observer < config.authorityCount →
      faults.correctAvailable observer = true →
      ∃ finish, start ≤ finish ∧ observerDone observer finish := by
    intro observer observerInRange observerCorrect
    let holderDone := fun holder time =>
      start ≤ time ∧
        ∀ capsule,
          (sources.source holder).capsule = some capsule →
          AllHistoryItemsAcceptedAt syncTrace observer capsule.history time
    have holderDonePersists : ∀ holder earlier later,
        earlier ≤ later → holderDone holder earlier →
          holderDone holder later := by
      intro holder earlier later earlierBeforeLater doneEarlier
      refine ⟨Nat.le_trans doneEarlier.1 earlierBeforeLater, ?_⟩
      intro capsule capsulePresent block blockMember
      exact storage.acceptedMonotone observer block earlier later
        earlierBeforeLater
        (doneEarlier.2 capsule capsulePresent block blockMember)
    have eachHolder : ∀ holder,
        holder ∈ sources.validators →
        ∃ finish, start ≤ finish ∧ holderDone holder finish := by
      intro holder holderMember
      cases capsulePresent : (sources.source holder).capsule with
      | none =>
          refine ⟨start, Nat.le_refl _, Nat.le_refl _, ?_⟩
          intro capsule impossible
          rw [capsulePresent] at impossible
          contradiction
      | some capsule =>
          have sourceHistory := base.protectedCapsuleAtStart holder holderMember
            capsule capsulePresent
          have sourceRequests := base.requestRules holder holderMember capsule
            capsulePresent
          rcases retained_causal_history_eventually_accepted
              storage.toBlockSyncStorageRules requests serving accepting
              sourceHistory sourceRequests afterGst observerInRange
              observerCorrect with
            ⟨finish, startBeforeFinish, historyAccepted⟩
          refine ⟨finish, startBeforeFinish, startBeforeFinish, ?_⟩
          intro otherCapsule otherPresent
          have sameCapsule : otherCapsule = capsule := by
            rw [capsulePresent] at otherPresent
            exact Option.some.inj otherPresent.symm
          simpa [sameCapsule] using historyAccepted
    rcases eventually_all_list_members sources.validators holderDone start
        holderDonePersists eachHolder with
      ⟨finish, startBeforeFinish, allHolders⟩
    exact ⟨finish, startBeforeFinish, startBeforeFinish, by
      intro holder holderMember
      exact (allHolders holder holderMember).2⟩
  rcases eventually_every_selected_validator faults.correctAvailable
      observerDone start observerDonePersists eachObserver with
    ⟨finish, startBeforeFinish, allObservers⟩
  refine ⟨finish, startBeforeFinish, ?_⟩
  intro observer observerInRange observerCorrect
  exact (allObservers observer observerInRange observerCorrect).2

/-- One correct requester eventually accepts one correct recovery-base block.
The proof uses the block's concrete causal parent provenance. -/
theorem correct_requester_eventually_accepts_recovery_base_block
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {worldTrace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {bodySyncTrace : BlockSyncTrace (ValidatorBlock ScheduledBlockId)}
    {recoveryTrace : LowestPendingRecoveryTrace}
    {sources :
      FixedCausalRecoverySources ScheduledBlockId CommitId config}
    {blockProtocolPacket :
      AddressedPacket
        (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) → Prop}
    {blockNetwork : AddressedPartialSynchrony config faults blockProtocolPacket}
    (blockRequestCursor : Time → Nat → Nat)
    (blockStorage : BlockSyncStorageRules
      (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockRequests : FairProtectedRequestActions config faults
      blockProtocolPacket (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockServing : FairProtectedServeActions config faults blockProtocolPacket
      (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockAccepting : FairProtectedAcceptActions config faults
      blockProtocolPacket (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (bodyStorage : RecoveryBodyStorageRules bodySyncTrace)
    (exactRules : ExactNextBlockProductionRules config faults worldTrace)
    (genesis : CanonicalGenesisRules config faults worldTrace)
    (provenance :
      RecoveryBlockCausalProvenance config faults worldTrace sources)
    {recoveryStart current author requester : Time}
    (base : InitialRecoveryExecutionBase config faults bodySyncTrace
      recoveryTrace sources recoveryStart)
    (gcRules : RecoveryGcProtectionRules recoveryTrace sources recoveryStart)
    (mapping : LowestPendingRecoveryExecutionMapping worldTrace bodySyncTrace
      recoveryTrace)
    (recoveryBeforeCurrent : recoveryStart ≤ current)
    (afterGst : blockNetwork.gst ≤ current)
    (historiesAccepted : EveryRecoveryHistoryAcceptedAt faults bodySyncTrace
      sources current)
    (produced : EveryCorrectAvailableValidatorProduced faults
      (worldTrace current) sources.maximumRound)
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true) :
    ∃ finish,
      current ≤ finish ∧
      ((worldTrace finish).validatorState requester).acceptedRepresentative
          sources.maximumRound author =
        some (scheduledBlockRef author sources.maximumRound) := by
  let item := scheduledBlockRef author sources.maximumRound
  have authorProduced := produced author authorInRange authorCorrect
  have authorOwn :
      ((worldTrace current).validatorState author).ownBlockAt
          sources.maximumRound = some item := by
    cases ownValue : ((worldTrace current).validatorState author).ownBlockAt
        sources.maximumRound with
    | none => simp [ownValue] at authorProduced
    | some reference =>
        have canonical := exactRules.ownBlockIsCanonical current author
          sources.maximumRound reference authorInRange authorCorrect ownValue
        simpa [item, canonical] using ownValue
  have ownerServable := exactRules.durableOwnBlockIsServable current author
    sources.maximumRound authorInRange authorCorrect authorOwn
  have source : RetainedCorrectOwnerItem config faults
      (blockSyncTraceOfWorld worldTrace blockRequestCursor) author item current := by
    refine ⟨authorInRange, authorCorrect, ?_, ?_⟩
    · simpa [RetainedAt, blockSyncTraceOfWorld, item] using ownerServable.1
    · simpa [AcceptedAt, blockSyncTraceOfWorld, item] using ownerServable.2
  obtain ⟨block, _catalog, blockReference, blockValid, parentProvenance⟩ :=
    provenance.ownBlockHasCausalParents current author sources.maximumRound
      authorInRange authorCorrect authorOwn
  by_cases alreadyAccepted :
      ((worldTrace current).validatorState requester).accepted item = true
  · have representative := exactRules.acceptedInstallsRepresentative current
      requester author sources.maximumRound requesterInRange requesterCorrect
      authorInRange authorCorrect (by simpa [item] using alreadyAccepted)
    exact ⟨current, Nat.le_refl _, representative⟩
  · have notAccepted :
        ((worldTrace current).validatorState requester).accepted item = false := by
      cases acceptedValue :
          ((worldTrace current).validatorState requester).accepted item
      · rfl
      · exact False.elim (alreadyAccepted acceptedValue)
    have requesterProduced :=
      (produced requester requesterInRange requesterCorrect).1
    have requesterNeeds : requester ≠ author →
        NeededAt (blockSyncTraceOfWorld worldTrace blockRequestCursor)
          requester item current := by
      intro requesterNotAuthor
      have requested := exactRules.missingBlockIsRequested current requester
        author sources.maximumRound requesterInRange requesterCorrect
        authorInRange authorCorrect requesterNotAuthor requesterProduced
        (by simpa [item] using notAccepted)
      simpa [NeededAt, blockSyncTraceOfWorld] using requested
    have parentsReady : ∀ parent,
        parent ∈ block.parents →
        RecoveryParentReadyAt
          ((worldTrace current).validatorState requester) parent := by
      intro parent parentMember
      exact recovery_block_parent_is_ready bodyStorage base gcRules mapping
        exactRules genesis provenance recoveryBeforeCurrent historiesAccepted
        requesterInRange requesterCorrect authorInRange authorCorrect
        blockReference blockValid parentProvenance parentMember
    have earlierAccepted : EarlierHistoryAccepted
        (blockSyncTraceOfWorld worldTrace blockRequestCursor) requester
        block.parents current := by
      intro parent parentMember
      have ready := parentsReady parent parentMember
      simpa [AcceptedAt, blockSyncTraceOfWorld] using ready.1
    have historyOrder : block.parents ++ [item] =
        block.parents ++ item :: [] := rfl
    rcases retained_owner_history_item_eventually_accepted blockStorage
        blockRequests blockServing blockAccepting source requesterInRange
        requesterCorrect afterGst requesterNeeds historyOrder earlierAccepted with
      ⟨finish, currentBeforeFinish, acceptedAtFinish⟩
    have representative := exactRules.acceptedInstallsRepresentative finish
      requester author sources.maximumRound requesterInRange requesterCorrect
      authorInRange authorCorrect
      (by simpa [AcceptedAt, blockSyncTraceOfWorld, item] using acceptedAtFinish)
    exact ⟨finish, currentBeforeFinish, representative⟩

/-- Every correct requester eventually accepts one correct author's recovery
base block. -/
theorem correct_author_recovery_base_block_eventually_accepted_by_all
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {worldTrace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {bodySyncTrace : BlockSyncTrace (ValidatorBlock ScheduledBlockId)}
    {recoveryTrace : LowestPendingRecoveryTrace}
    {sources :
      FixedCausalRecoverySources ScheduledBlockId CommitId config}
    {blockProtocolPacket :
      AddressedPacket
        (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) → Prop}
    {blockNetwork : AddressedPartialSynchrony config faults blockProtocolPacket}
    (blockRequestCursor : Time → Nat → Nat)
    (blockStorage : BlockSyncStorageRules
      (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockRequests : FairProtectedRequestActions config faults
      blockProtocolPacket (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockServing : FairProtectedServeActions config faults blockProtocolPacket
      (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockAccepting : FairProtectedAcceptActions config faults
      blockProtocolPacket (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (bodyStorage : RecoveryBodyStorageRules bodySyncTrace)
    (exactRules : ExactNextBlockProductionRules config faults worldTrace)
    (genesis : CanonicalGenesisRules config faults worldTrace)
    (provenance :
      RecoveryBlockCausalProvenance config faults worldTrace sources)
    {recoveryStart current author : Time}
    (base : InitialRecoveryExecutionBase config faults bodySyncTrace
      recoveryTrace sources recoveryStart)
    (gcRules : RecoveryGcProtectionRules recoveryTrace sources recoveryStart)
    (mapping : LowestPendingRecoveryExecutionMapping worldTrace bodySyncTrace
      recoveryTrace)
    (recoveryBeforeCurrent : recoveryStart ≤ current)
    (active : ∀ time, current ≤ time →
      (worldTrace time).epochActive = true)
    (afterGst : blockNetwork.gst ≤ current)
    (historiesAccepted : EveryRecoveryHistoryAcceptedAt faults bodySyncTrace
      sources current)
    (produced : EveryCorrectAvailableValidatorProduced faults
      (worldTrace current) sources.maximumRound)
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true) :
    ∃ finish,
      current ≤ finish ∧
      ∀ requester,
        requester < config.authorityCount →
        faults.correctAvailable requester = true →
        ((worldTrace finish).validatorState requester).acceptedRepresentative
            sources.maximumRound author =
          some (scheduledBlockRef author sources.maximumRound) := by
  let acceptedBy := fun requester time =>
    current ≤ time ∧
      ((worldTrace time).validatorState requester).acceptedRepresentative
          sources.maximumRound author =
        some (scheduledBlockRef author sources.maximumRound)
  have acceptedByPersists : ∀ requester earlier later,
      earlier ≤ later → acceptedBy requester earlier →
        acceptedBy requester later := by
    intro requester earlier later earlierBeforeLater acceptedEarlier
    refine ⟨Nat.le_trans acceptedEarlier.1 earlierBeforeLater, ?_⟩
    exact exactRules.acceptedRepresentativePersists requester
      sources.maximumRound author
      (scheduledBlockRef author sources.maximumRound) earlier later
      earlierBeforeLater
      (by
        intro time earlierBeforeTime _
        exact active time (Nat.le_trans acceptedEarlier.1 earlierBeforeTime))
      acceptedEarlier.2
  have eachRequester : ∀ requester,
      requester < config.authorityCount →
      faults.correctAvailable requester = true →
      ∃ finish, current ≤ finish ∧ acceptedBy requester finish := by
    intro requester requesterInRange requesterCorrect
    rcases correct_requester_eventually_accepts_recovery_base_block
        blockRequestCursor blockStorage blockRequests blockServing blockAccepting
        bodyStorage exactRules genesis provenance base gcRules mapping
        recoveryBeforeCurrent afterGst historiesAccepted produced authorInRange
        authorCorrect requesterInRange requesterCorrect with
      ⟨finish, currentBeforeFinish, accepted⟩
    exact ⟨finish, currentBeforeFinish, currentBeforeFinish, accepted⟩
  rcases eventually_every_selected_validator faults.correctAvailable acceptedBy
      current acceptedByPersists eachRequester with
    ⟨finish, currentBeforeFinish, allAccepted⟩
  refine ⟨finish, currentBeforeFinish, ?_⟩
  intro requester requesterInRange requesterCorrect
  exact (allAccepted requester requesterInRange requesterCorrect).2

/-- Every correct, available validator eventually accepts every correct
recovery-base block. -/
theorem every_recovery_base_block_eventually_accepted
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {worldTrace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {bodySyncTrace : BlockSyncTrace (ValidatorBlock ScheduledBlockId)}
    {recoveryTrace : LowestPendingRecoveryTrace}
    {sources :
      FixedCausalRecoverySources ScheduledBlockId CommitId config}
    {blockProtocolPacket :
      AddressedPacket
        (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) → Prop}
    {blockNetwork : AddressedPartialSynchrony config faults blockProtocolPacket}
    (blockRequestCursor : Time → Nat → Nat)
    (blockStorage : BlockSyncStorageRules
      (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockRequests : FairProtectedRequestActions config faults
      blockProtocolPacket (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockServing : FairProtectedServeActions config faults blockProtocolPacket
      (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockAccepting : FairProtectedAcceptActions config faults
      blockProtocolPacket (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (bodyStorage : RecoveryBodyStorageRules bodySyncTrace)
    (exactRules : ExactNextBlockProductionRules config faults worldTrace)
    (genesis : CanonicalGenesisRules config faults worldTrace)
    (provenance :
      RecoveryBlockCausalProvenance config faults worldTrace sources)
    {recoveryStart current : Time}
    (base : InitialRecoveryExecutionBase config faults bodySyncTrace
      recoveryTrace sources recoveryStart)
    (gcRules : RecoveryGcProtectionRules recoveryTrace sources recoveryStart)
    (mapping : LowestPendingRecoveryExecutionMapping worldTrace bodySyncTrace
      recoveryTrace)
    (recoveryBeforeCurrent : recoveryStart ≤ current)
    (active : ∀ time, current ≤ time →
      (worldTrace time).epochActive = true)
    (afterGst : blockNetwork.gst ≤ current)
    (historiesAccepted : EveryRecoveryHistoryAcceptedAt faults bodySyncTrace
      sources current)
    (produced : EveryCorrectAvailableValidatorProduced faults
      (worldTrace current) sources.maximumRound) :
    ∃ finish,
      current ≤ finish ∧
      EveryCorrectAvailableValidatorAccepted faults (worldTrace finish)
        sources.maximumRound := by
  let authorDone := fun author time =>
    current ≤ time ∧
      ∀ requester,
        requester < config.authorityCount →
        faults.correctAvailable requester = true →
        ((worldTrace time).validatorState requester).acceptedRepresentative
            sources.maximumRound author =
          some (scheduledBlockRef author sources.maximumRound)
  have authorDonePersists : ∀ author earlier later,
      earlier ≤ later → authorDone author earlier →
        authorDone author later := by
    intro author earlier later earlierBeforeLater doneEarlier
    refine ⟨Nat.le_trans doneEarlier.1 earlierBeforeLater, ?_⟩
    intro requester requesterInRange requesterCorrect
    exact exactRules.acceptedRepresentativePersists requester
      sources.maximumRound author
      (scheduledBlockRef author sources.maximumRound) earlier later
      earlierBeforeLater
      (by
        intro time earlierBeforeTime _
        exact active time (Nat.le_trans doneEarlier.1 earlierBeforeTime))
      (doneEarlier.2 requester requesterInRange requesterCorrect)
  have eachAuthor : ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ∃ finish, current ≤ finish ∧ authorDone author finish := by
    intro author authorInRange authorCorrect
    rcases correct_author_recovery_base_block_eventually_accepted_by_all
        blockRequestCursor blockStorage blockRequests blockServing blockAccepting
        bodyStorage exactRules genesis provenance base gcRules mapping
        recoveryBeforeCurrent active afterGst historiesAccepted produced
        authorInRange authorCorrect with
      ⟨finish, currentBeforeFinish, accepted⟩
    exact ⟨finish, currentBeforeFinish, currentBeforeFinish, accepted⟩
  rcases eventually_every_selected_validator faults.correctAvailable authorDone
      current authorDonePersists eachAuthor with
    ⟨finish, currentBeforeFinish, allAuthors⟩
  refine ⟨finish, currentBeforeFinish, ?_⟩
  intro observer author observerInRange observerCorrect authorInRange authorCorrect
  have representative := (allAuthors author authorInRange authorCorrect).2
    observer observerInRange observerCorrect
  simp [representative]

/-- Causal delivery and fair local recovery establish one complete common round
above all initial recovery sources. The common round is a ghost proof value. -/
theorem causal_recovery_establishes_complete_base
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {worldTrace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {bodySyncTrace : BlockSyncTrace (ValidatorBlock ScheduledBlockId)}
    {recoveryTrace : LowestPendingRecoveryTrace}
    {sources :
      FixedCausalRecoverySources ScheduledBlockId CommitId config}
    {bodyProtocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlock ScheduledBlockId)) →
        Prop}
    {bodyNetwork : AddressedPartialSynchrony config faults bodyProtocolPacket}
    {blockProtocolPacket :
      AddressedPacket
        (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) → Prop}
    {blockNetwork : AddressedPartialSynchrony config faults blockProtocolPacket}
    (bodyStorage : RecoveryBodyStorageRules bodySyncTrace)
    (bodyRequests : FairProtectedRequestActions config faults bodyProtocolPacket
      bodySyncTrace)
    (bodyServing : FairProtectedServeActions config faults bodyProtocolPacket
      bodySyncTrace)
    (bodyAccepting : FairProtectedAcceptActions config faults bodyProtocolPacket
      bodySyncTrace)
    (blockRequestCursor : Time → Nat → Nat)
    (blockStorage : BlockSyncStorageRules
      (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockRequests : FairProtectedRequestActions config faults
      blockProtocolPacket (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockServing : FairProtectedServeActions config faults blockProtocolPacket
      (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockAccepting : FairProtectedAcceptActions config faults
      blockProtocolPacket (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (exactRules : ExactNextBlockProductionRules config faults worldTrace)
    (genesis : CanonicalGenesisRules config faults worldTrace)
    (provenance :
      RecoveryBlockCausalProvenance config faults worldTrace sources)
    {recoveryStart : Time}
    (bodyAfterGst : bodyNetwork.gst ≤ recoveryStart)
    (blockAfterGst : blockNetwork.gst ≤ recoveryStart)
    (active : ∀ time, recoveryStart ≤ time →
      (worldTrace time).epochActive = true)
    (base : InitialRecoveryExecutionBase config faults bodySyncTrace
      recoveryTrace sources recoveryStart)
    (gcRules : RecoveryGcProtectionRules recoveryTrace sources recoveryStart)
    (barrier : CausalAcceptanceBarrierRules (config := config) bodySyncTrace
      recoveryTrace)
    (localRules : DurableLowestPendingRules recoveryTrace)
    (proposalHistory : RecoveryProposalHistoryRules recoveryTrace sources
      recoveryStart)
    (proposalFair : FairLowestPendingProposalActions faults recoveryTrace)
    (genesisFair : FairCanonicalGenesisActions faults recoveryTrace)
    (resendFair : FairDurableOwnBlockActions faults recoveryTrace)
    (mapping : LowestPendingRecoveryExecutionMapping worldTrace bodySyncTrace
      recoveryTrace) :
    ∃ finish,
      recoveryStart ≤ finish ∧
      EveryCorrectAvailableValidatorProduced faults (worldTrace finish)
        sources.maximumRound ∧
      EveryCorrectAvailableValidatorAccepted faults (worldTrace finish)
        sources.maximumRound := by
  rcases maximum_initial_recovery_round_eventually_owned_in_validator_execution
      bodyAfterGst bodyStorage bodyRequests bodyServing bodyAccepting base gcRules
      barrier localRules proposalHistory proposalFair genesisFair resendFair
      mapping with
    ⟨productionFinish, recoveryBeforeProduction, allOwned⟩
  rcases every_recovery_history_eventually_accepted bodyAfterGst bodyStorage
      bodyRequests bodyServing bodyAccepting base with
    ⟨historyFinish, recoveryBeforeHistory, historiesAccepted⟩
  let common := Nat.max productionFinish historyFinish
  have productionBeforeCommon : productionFinish ≤ common := Nat.le_max_left _ _
  have historyBeforeCommon : historyFinish ≤ common := Nat.le_max_right _ _
  have recoveryBeforeCommon : recoveryStart ≤ common :=
    Nat.le_trans recoveryBeforeProduction productionBeforeCommon
  have producedAtProduction : EveryCorrectAvailableValidatorProduced faults
      (worldTrace productionFinish) sources.maximumRound := by
    intro validator validatorInRange validatorCorrect
    exact allOwned validator validatorInRange validatorCorrect
  have producedAtCommon : EveryCorrectAvailableValidatorProduced faults
      (worldTrace common) sources.maximumRound := by
    exact every_correct_available_production_persists exactRules
      productionBeforeCommon
      (by
        intro time productionBeforeTime _
        exact active time
          (Nat.le_trans recoveryBeforeProduction productionBeforeTime))
      producedAtProduction
  have historiesAtCommon : EveryRecoveryHistoryAcceptedAt faults bodySyncTrace
      sources common :=
    every_recovery_history_acceptance_persists
      bodyStorage.toBlockSyncStorageRules historyBeforeCommon historiesAccepted
  have activeAfterCommon : ∀ time, common ≤ time →
      (worldTrace time).epochActive = true := by
    intro time commonBeforeTime
    exact active time (Nat.le_trans recoveryBeforeCommon commonBeforeTime)
  have blockGstBeforeCommon : blockNetwork.gst ≤ common :=
    Nat.le_trans blockAfterGst recoveryBeforeCommon
  rcases every_recovery_base_block_eventually_accepted blockRequestCursor
      blockStorage blockRequests blockServing blockAccepting bodyStorage
      exactRules genesis provenance base gcRules mapping recoveryBeforeCommon
      activeAfterCommon blockGstBeforeCommon historiesAtCommon producedAtCommon with
    ⟨finish, commonBeforeFinish, acceptedAtFinish⟩
  have producedAtFinish : EveryCorrectAvailableValidatorProduced faults
      (worldTrace finish) sources.maximumRound := by
    exact every_correct_available_production_persists exactRules
      commonBeforeFinish
      (by
        intro time commonBeforeTime _
        exact activeAfterCommon time commonBeforeTime)
      producedAtCommon
  exact ⟨finish, Nat.le_trans recoveryBeforeCommon commonBeforeFinish,
    producedAtFinish, acceptedAtFinish⟩

/-- The complete causal-recovery base advances to any requested finite
exact-next window. Quorum layers are derived from pointwise correct-validator
production and acceptance. -/
theorem causal_recovery_establishes_exact_next_window
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {worldTrace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {bodySyncTrace : BlockSyncTrace (ValidatorBlock ScheduledBlockId)}
    {recoveryTrace : LowestPendingRecoveryTrace}
    {sources :
      FixedCausalRecoverySources ScheduledBlockId CommitId config}
    {bodyProtocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlock ScheduledBlockId)) →
        Prop}
    {bodyNetwork : AddressedPartialSynchrony config faults bodyProtocolPacket}
    {blockProtocolPacket :
      AddressedPacket
        (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) → Prop}
    {blockNetwork : AddressedPartialSynchrony config faults blockProtocolPacket}
    (bodyStorage : RecoveryBodyStorageRules bodySyncTrace)
    (bodyRequests : FairProtectedRequestActions config faults bodyProtocolPacket
      bodySyncTrace)
    (bodyServing : FairProtectedServeActions config faults bodyProtocolPacket
      bodySyncTrace)
    (bodyAccepting : FairProtectedAcceptActions config faults bodyProtocolPacket
      bodySyncTrace)
    (blockRequestCursor : Time → Nat → Nat)
    (blockStorage : BlockSyncStorageRules
      (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockRequests : FairProtectedRequestActions config faults
      blockProtocolPacket (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockServing : FairProtectedServeActions config faults blockProtocolPacket
      (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockAccepting : FairProtectedAcceptActions config faults
      blockProtocolPacket (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (exactRules : ExactNextBlockProductionRules config faults worldTrace)
    (exactProposalFair : FairExactNextProposalActions config faults worldTrace)
    (genesis : CanonicalGenesisRules config faults worldTrace)
    (provenance :
      RecoveryBlockCausalProvenance config faults worldTrace sources)
    {recoveryStart : Time}
    (bodyAfterGst : bodyNetwork.gst ≤ recoveryStart)
    (blockAfterGst : blockNetwork.gst ≤ recoveryStart)
    (active : ∀ time, recoveryStart ≤ time →
      (worldTrace time).epochActive = true)
    (base : InitialRecoveryExecutionBase config faults bodySyncTrace
      recoveryTrace sources recoveryStart)
    (gcRules : RecoveryGcProtectionRules recoveryTrace sources recoveryStart)
    (barrier : CausalAcceptanceBarrierRules (config := config) bodySyncTrace
      recoveryTrace)
    (localRules : DurableLowestPendingRules recoveryTrace)
    (proposalHistory : RecoveryProposalHistoryRules recoveryTrace sources
      recoveryStart)
    (proposalFair : FairLowestPendingProposalActions faults recoveryTrace)
    (genesisFair : FairCanonicalGenesisActions faults recoveryTrace)
    (resendFair : FairDurableOwnBlockActions faults recoveryTrace)
    (mapping : LowestPendingRecoveryExecutionMapping worldTrace bodySyncTrace
      recoveryTrace)
    (minimumRound count : Nat) :
    ∃ finish baseRound,
      recoveryStart ≤ finish ∧
      minimumRound ≤ baseRound ∧
      ∀ offset,
        offset < count →
        EveryCorrectAvailableValidatorProduced faults (worldTrace finish)
            (baseRound + offset) ∧
          EveryCorrectAvailableValidatorAccepted faults (worldTrace finish)
            (baseRound + offset) ∧
          ProducedCorrectQuorumLayer config faults (worldTrace finish)
            (baseRound + offset) ∧
          CommonAcceptedCorrectQuorumLayer config faults (worldTrace finish)
            (baseRound + offset) := by
  rcases causal_recovery_establishes_complete_base bodyStorage bodyRequests
      bodyServing bodyAccepting blockRequestCursor blockStorage blockRequests
      blockServing blockAccepting exactRules genesis provenance bodyAfterGst
      blockAfterGst active base gcRules barrier localRules proposalHistory
      proposalFair genesisFair resendFair mapping with
    ⟨recoveryFinish, recoveryBeforeFinish, baseProduced, baseAccepted⟩
  let targetRound := Nat.max minimumRound sources.maximumRound
  let targetOffset := targetRound - sources.maximumRound
  have maximumBeforeTarget : sources.maximumRound ≤ targetRound :=
    Nat.le_max_right _ _
  have targetIdentity : sources.maximumRound + targetOffset = targetRound := by
    dsimp [targetOffset]
    omega
  have activeAfterRecovery : ∀ time, recoveryFinish ≤ time →
      (worldTrace time).epochActive = true := by
    intro time recoveryFinishBeforeTime
    exact active time
      (Nat.le_trans recoveryBeforeFinish recoveryFinishBeforeTime)
  have blockGstBeforeRecovery : blockNetwork.gst ≤ recoveryFinish :=
    Nat.le_trans blockAfterGst recoveryBeforeFinish
  rcases complete_base_round_eventually_reaches_offset blockRequestCursor
      blockStorage blockRequests blockServing blockAccepting exactRules
      exactProposalFair activeAfterRecovery blockGstBeforeRecovery baseProduced
      baseAccepted targetOffset with
    ⟨targetFinish, recoveryBeforeTarget, targetProduced, targetAccepted⟩
  have activeAfterTarget : ∀ time, targetFinish ≤ time →
      (worldTrace time).epochActive = true := by
    intro time targetBeforeTime
    exact activeAfterRecovery time
      (Nat.le_trans recoveryBeforeTarget targetBeforeTime)
  have blockGstBeforeTarget : blockNetwork.gst ≤ targetFinish :=
    Nat.le_trans blockGstBeforeRecovery recoveryBeforeTarget
  rw [targetIdentity] at targetProduced targetAccepted
  rcases complete_base_round_eventually_produces_window blockRequestCursor
      blockStorage blockRequests blockServing blockAccepting exactRules
      exactProposalFair activeAfterTarget blockGstBeforeTarget targetProduced
      targetAccepted count with
    ⟨finish, targetBeforeFinish, completeWindow⟩
  refine ⟨finish, targetRound,
    Nat.le_trans recoveryBeforeFinish
      (Nat.le_trans recoveryBeforeTarget targetBeforeFinish),
    Nat.le_max_left _ _, ?_⟩
  intro offset offsetInWindow
  have complete := completeWindow offset offsetInWindow
  exact ⟨complete.1, complete.2,
    every_correct_available_validator_produced_gives_quorum_layer config faults
      (worldTrace finish) (targetRound + offset) complete.1,
    every_correct_available_validator_accepted_gives_common_layer config faults
      (worldTrace finish) (targetRound + offset) complete.2⟩

/-- From one arbitrary recovery start, each correct, available validator later
stores and sends an own block above both its start floor and any requested
minimum round. -/
theorem causal_recovery_has_unbounded_validator_block_production_from_start
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {worldTrace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {bodySyncTrace : BlockSyncTrace (ValidatorBlock ScheduledBlockId)}
    {recoveryTrace : LowestPendingRecoveryTrace}
    {sources :
      FixedCausalRecoverySources ScheduledBlockId CommitId config}
    {bodyProtocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlock ScheduledBlockId)) →
        Prop}
    {bodyNetwork : AddressedPartialSynchrony config faults bodyProtocolPacket}
    {blockProtocolPacket :
      AddressedPacket
        (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) → Prop}
    {blockNetwork : AddressedPartialSynchrony config faults blockProtocolPacket}
    (bodyStorage : RecoveryBodyStorageRules bodySyncTrace)
    (bodyRequests : FairProtectedRequestActions config faults bodyProtocolPacket
      bodySyncTrace)
    (bodyServing : FairProtectedServeActions config faults bodyProtocolPacket
      bodySyncTrace)
    (bodyAccepting : FairProtectedAcceptActions config faults bodyProtocolPacket
      bodySyncTrace)
    (blockRequestCursor : Time → Nat → Nat)
    (blockStorage : BlockSyncStorageRules
      (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockRequests : FairProtectedRequestActions config faults
      blockProtocolPacket (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockServing : FairProtectedServeActions config faults blockProtocolPacket
      (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (blockAccepting : FairProtectedAcceptActions config faults
      blockProtocolPacket (blockSyncTraceOfWorld worldTrace blockRequestCursor))
    (exactRules : ExactNextBlockProductionRules config faults worldTrace)
    (exactProposalFair : FairExactNextProposalActions config faults worldTrace)
    (genesis : CanonicalGenesisRules config faults worldTrace)
    (provenance :
      RecoveryBlockCausalProvenance config faults worldTrace sources)
    {recoveryStart : Time}
    (bodyAfterGst : bodyNetwork.gst ≤ recoveryStart)
    (blockAfterGst : blockNetwork.gst ≤ recoveryStart)
    (active : ∀ time, recoveryStart ≤ time →
      (worldTrace time).epochActive = true)
    (base : InitialRecoveryExecutionBase config faults bodySyncTrace
      recoveryTrace sources recoveryStart)
    (gcRules : RecoveryGcProtectionRules recoveryTrace sources recoveryStart)
    (barrier : CausalAcceptanceBarrierRules (config := config) bodySyncTrace
      recoveryTrace)
    (localRules : DurableLowestPendingRules recoveryTrace)
    (proposalHistory : RecoveryProposalHistoryRules recoveryTrace sources
      recoveryStart)
    (proposalFair : FairLowestPendingProposalActions faults recoveryTrace)
    (genesisFair : FairCanonicalGenesisActions faults recoveryTrace)
    (resendFair : FairDurableOwnBlockActions faults recoveryTrace)
    (mapping : LowestPendingRecoveryExecutionMapping worldTrace bodySyncTrace
      recoveryTrace)
    (validator minimumRound : Nat)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrect : faults.correctAvailable validator = true) :
    ∃ finish round,
      recoveryStart ≤ finish ∧
      minimumRound ≤ round ∧
      ((worldTrace recoveryStart).validatorState validator).highestSignedRound <
        round ∧
      (((worldTrace finish).validatorState validator).ownBlockAt round).isSome =
          true ∧
      ((worldTrace finish).validatorState validator).sentOwnBlockAt round = true := by
  let target := Nat.max minimumRound
    (((worldTrace recoveryStart).validatorState validator).highestSignedRound + 1)
  rcases causal_recovery_establishes_exact_next_window bodyStorage bodyRequests
      bodyServing bodyAccepting blockRequestCursor blockStorage blockRequests
      blockServing blockAccepting exactRules exactProposalFair genesis provenance
      bodyAfterGst blockAfterGst active base gcRules barrier localRules
      proposalHistory proposalFair genesisFair resendFair mapping target 1 with
    ⟨finish, round, recoveryBeforeFinish, targetBeforeRound, complete⟩
  have roundComplete := complete 0 (by omega)
  have validatorProduced := roundComplete.1 validator validatorInRange
    validatorCorrect
  refine ⟨finish, round, recoveryBeforeFinish, ?_, ?_, ?_⟩
  · exact Nat.le_trans (Nat.le_max_left _ _) targetBeforeRound
  · have nextBeforeTarget :
        ((worldTrace recoveryStart).validatorState validator).highestSignedRound +
            1 ≤ target := Nat.le_max_right _ _
    omega
  · simpa using validatorProduced

end Mysticeti
