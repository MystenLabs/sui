/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorCurrentTipSubscriptionExecution
import Mysticeti.ValidatorOperationalFrontierSuccessor

namespace Mysticeti

/-! Local normal max-timeout progress at an operational frontier.

The current Rust max-timeout is one-shot. A forced attempt can wait for only two
transient inputs: the recovered last-known own round and an acceptable
propagation delay. Each input has a Core-thread watcher which makes another
forced attempt when the blocker clears. All other failed forced paths mean that
the target was already signed or that the threshold clock moved higher.

The rule below abstracts this event-driven case split. It does not require a
periodic timeout re-arm. The disposition is local only. It is an actual proposal
action, protected proposal work, a signer-floor crossing, or a higher
threshold-clock observation. It does not state a future block, packet, quorum,
layer, or commit.
-/

/-- One eventual local disposition of protected normal frontier work. -/
inductive ValidatorNormalFrontierPacemakerDispositionAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (start validator targetRound : Time)
    (parents : List (ValidatorBlockRef BlockId)) : Prop where
  | proposalOccurred
      (actionAt : Time)
      (startBeforeAction : start ≤ actionAt)
      (occurs : ValidatorLocalActionOccurs
        (timed.execution.events actionAt) validator
          (.proposeNormal targetRound parents)) :
      ValidatorNormalFrontierPacemakerDispositionAt timed start validator
        targetRound parents
  | proposalProtected
      (scheduledAt : Time)
      (startBeforeScheduled : start ≤ scheduledAt)
      (proposal : ValidatorNormalProposalAt timed scheduledAt validator)
      (targetExact : proposal.targetRound = targetRound)
      (parentsExact : proposal.parents = parents) :
      ValidatorNormalFrontierPacemakerDispositionAt timed start validator
        targetRound parents
  | signerFloorReached
      (reachedAt : Time)
      (startBeforeReached : start ≤ reachedAt)
      (targetReached : targetRound ≤
        ((timed.execution.trace reachedAt).validatorState
          validator).highestSignedRound) :
      ValidatorNormalFrontierPacemakerDispositionAt timed start validator
        targetRound parents
  | clockAdvanced
      (advancedAt : Time)
      (startBeforeAdvanced : start ≤ advancedAt)
      (targetPassed : targetRound <
        ((timed.execution.trace advancedAt).validatorState
          validator).thresholdClockRound) :
      ValidatorNormalFrontierPacemakerDispositionAt timed start validator
        targetRound parents

/-- Local event-driven fairness rules for the normal max-timeout pacemaker.

`protectedParentWork` is current scheduler ownership of one exact target and
parent list. It includes a pending max timeout or a watcher retry for a transient
`should_propose` blocker. It is not a future result. The first rule obtains that
ownership from an exact active ready frontier. The second rule classifies the
eventual forced-attempt result. -/
structure ValidatorNormalFrontierPacemakerRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  protectedParentWork : Time → Nat → Nat →
    List (ValidatorBlockRef BlockId) → Prop
  readyFrontierStartsProtectedWork : ∀ time validator targetRound parents,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (timed.execution.trace time).epochActive = true →
    ((timed.execution.trace time).validatorState
      validator).thresholdClockRound = targetRound →
    ((timed.execution.trace time).validatorState
      validator).highestSignedRound < targetRound →
    ValidatorProposalParentListReady .normal config
      ((timed.execution.trace time).validatorState validator) targetRound
        parents →
    protectedParentWork time validator targetRound parents
  protectedWorkEventuallyDisposes : ∀ start validator targetRound parents,
    protectedParentWork start validator targetRound parents →
    (∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) →
    ValidatorNormalFrontierPacemakerDispositionAt timed start validator
      targetRound parents

/-- One exact proposal persistence at or above a requested target gives the
origin-neutral addressed broadcast used by the operational successor proof. -/
private theorem pacemaker_persistence_gives_at_or_above_broadcast
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start persistTime validator targetRound : Time}
    {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (targetAboveStartFloor :
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound < targetRound)
    (startBeforePersistence : start ≤ persistTime)
    (blockAtOrAboveTarget : targetRound ≤ block.reference.round)
    (occurs : ValidatorLocalActionOccurs (timed.execution.events persistTime)
      validator (.persistProposal block)) :
    ValidatorAuthorLocalAtOrAboveBroadcastAt timed obligations start validator
      targetRound := by
  let exact := Classical.choice
    (persist_proposal_occurrence_eventually_produces_exact_broadcast
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable startBeforePersistence
          (Nat.lt_of_lt_of_le targetAboveStartFloor blockAtOrAboveTarget) occurs)
  refine ⟨block.reference.round, blockAtOrAboveTarget, ⟨⟨exact.1, ?_⟩⟩⟩
  rw [exact.2.2]

/-- Protected normal frontier work gives an exact-or-higher addressed
broadcast, unless its local threshold clock already advanced.

The signer-floor branch uses the concrete persistence which crossed the target.
The actual-action branch covers a proposal that ran in the same batch before a
later trace-state obligation could be observed. -/
theorem normal_frontier_pacemaker_eventually_broadcasts_or_clock_advances
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    (pacemaker : ValidatorNormalFrontierPacemakerRules timed)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator targetRound : Time}
    {parents : List (ValidatorBlockRef BlockId)}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (targetAboveStartFloor :
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound < targetRound)
    (protectedWork : pacemaker.protectedParentWork start validator targetRound
      parents)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorAuthorLocalAtOrAboveBroadcastAt timed obligations start validator
        targetRound ∨
      ∃ advancedAt,
        start ≤ advancedAt ∧
          targetRound <
            ((timed.execution.trace advancedAt).validatorState
              validator).thresholdClockRound := by
  cases pacemaker.protectedWorkEventuallyDisposes start validator
      targetRound parents protectedWork active with
  | proposalOccurred actionAt startBeforeAction occurs =>
      left
      let normal := Classical.choice
        (ValidatorCoreProposalAttemptContinuationRules.normal_proposal_occurrence_eventually_produces_exact_broadcast
          latchSource effects authorityCountAtLeastTwo occurs)
      apply pacemaker_persistence_gives_at_or_above_broadcast latchSource
        effects authorityCountAtLeastTwo validatorInRange
          validatorCorrectAvailable targetAboveStartFloor
            (Nat.le_trans startBeforeAction
              (Nat.le_trans normal.1.startBeforeProposalAction
                (Nat.le_trans (Nat.le_succ _)
                  normal.1.proposalBeforePersistence)))
              (Nat.le_of_eq normal.1.proposalRound.symm)
                normal.1.persistenceOccurs
  | proposalProtected scheduledAt startBeforeScheduled proposal targetExact
      _parentsExact =>
      left
      let normal := Classical.choice
        (protected_normal_proposal_eventually_produces_broadcast latchSource
          effects authorityCountAtLeastTwo validatorInRange
            validatorCorrectAvailable proposal)
      apply pacemaker_persistence_gives_at_or_above_broadcast latchSource
        effects authorityCountAtLeastTwo validatorInRange
          validatorCorrectAvailable targetAboveStartFloor
            (Nat.le_trans startBeforeScheduled
              (Nat.le_trans normal.startBeforeProposalAction
                (Nat.le_trans (Nat.le_succ _)
                  normal.proposalBeforePersistence)))
              (Nat.le_of_eq
                (targetExact.symm.trans normal.proposalRound.symm))
                normal.persistenceOccurs
  | signerFloorReached reachedAt startBeforeReached targetReached =>
      left
      rcases signer_floor_target_reached_has_target_persist_proposal
          timed.execution startBeforeReached targetAboveStartFloor targetReached
        with ⟨persistTime, block, startBeforePersist, _persistBeforeReached,
          persisted, blockAtOrAboveTarget⟩
      exact pacemaker_persistence_gives_at_or_above_broadcast latchSource
        effects authorityCountAtLeastTwo validatorInRange
          validatorCorrectAvailable targetAboveStartFloor startBeforePersist
            blockAtOrAboveTarget persisted
  | clockAdvanced advancedAt startBeforeAdvanced targetPassed =>
      exact Or.inr ⟨advancedAt, startBeforeAdvanced, targetPassed⟩

/-- A current pinned own tip has its exact accepted immediate-parent quorum.

Positive parents come from the pinned causal history. Round-zero parents use
the same canonical genesis list as the operational frontier. -/
theorem pinned_current_tip_parents_are_ready
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    {time author frontier : Time}
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (frontierSource : ValidatorOperationalQuorumFrontierAt config
      (timed.execution.trace time) author frontier canonicalGenesisParents)
    {capsuleId : ValidatorRecoveryCapsuleKey BlockId}
    {entry : ValidatorPinnedRecoveryCapsule (BlockId := BlockId) config}
    {block : ValidatorBlock BlockId}
    (stored : (pins.trace time author).capsuleAt capsuleId = some entry)
    (pinned : (pins.trace time author).pinned capsuleId = true)
    (targetBlock : entry.capsule.targetBlock = block) :
    ValidatorParentListReady config
      ((timed.execution.trace time).validatorState author)
        block.reference.round block.parents := by
  have genesisExact := pins.correctCapsuleUsesCanonicalGenesis time author
    capsuleId entry authorInRange authorCorrectAvailable stored
  have valid : block.HasQuorumImmediateParents config := by
    rw [← targetBlock]
    exact entry.capsule.targetValid
  refine ⟨valid.1, ?_, valid.2.2⟩
  intro parent parentMember
  refine ⟨valid.2.1 parent parentMember, ?_⟩
  have capsuleParentMember : parent ∈ entry.capsule.targetBlock.parents := by
    simpa [targetBlock] using parentMember
  rcases entry.capsule.targetParentsInHistory parent capsuleParentMember with
    genesisParent | ⟨parentBlock, parentBlockMember, parentReference⟩
  · have canonicalMember : parent ∈ canonicalGenesisParents := by
      rw [← sameGenesis, ← genesisExact]
      exact genesisParent
    exact (frontierSource.canonicalGenesisReady.2.1 parent canonicalMember).2
  · have localParent := pins.pinnedHistoryIsLocal time author capsuleId entry
      stored pinned parentBlock parentBlockMember
    simpa [parentReference] using localParent.1

/-- A correct maximum owner cannot have signed past the successor of its
current operational quorum frontier.

For a positive signer floor, the current pinned own block supplies an accepted
quorum in the preceding round. The operational frontier bounds that quorum. -/
theorem operational_maximum_owner_signer_floor_le_successor
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    {time author frontier : Time}
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (active : (timed.execution.trace time).epochActive = true)
    (frontierSource : ValidatorOperationalQuorumFrontierAt config
      (timed.execution.trace time) author frontier canonicalGenesisParents) :
    ((timed.execution.trace time).validatorState author).highestSignedRound ≤
      frontier + 1 := by
  let signerFloor := ((timed.execution.trace time).validatorState
    author).highestSignedRound
  by_cases floorZero : signerFloor = 0
  · omega
  have floorPositive : 0 < signerFloor := Nat.pos_of_ne_zero floorZero
  rcases pins.current_positive_tip_has_pinned_capsule_source authorInRange
      authorCorrectAvailable active floorPositive with
    ⟨block, capsuleId, entry, ownTip, targetBlock, stored, pinned, _source⟩
  have blockRound : block.reference.round = signerFloor :=
    (timed.execution.statesWellFormed time author authorInRange
      ).ownBlockIsSound signerFloor block.reference ownTip |>.2.1
  have parentsReady := pinned_current_tip_parents_are_ready pins sameGenesis
    authorInRange authorCorrectAvailable frontierSource stored pinned targetBlock
  have previousQuorum :=
    validator_parent_list_ready_gives_previous_quorum parentsReady |>.2
  have previousAtMostFrontier := frontierSource.upperBound
    (block.reference.round - 1) previousQuorum
  omega

/-- One already-signed exact successor tip remains a current pinned source and
is rebroadcast to every other validator after the observation time. -/
structure ValidatorOperationalMaximumSignedTipCarrier
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (recoveryWait start author maximum : Time) where
  source : ValidatorStableRecoveryTipSource pins recoveryWait start author
    (maximum + 1)
  tipAccepted : ((timed.execution.trace start).validatorState author).accepted
    source.block.reference = true
  tipRetained : ((timed.execution.trace start).validatorState author).retained
    source.block.reference = true
  tipCatalogued : (timed.execution.trace start).blockCatalog
    source.block.reference.id = some source.block
  parentsReady : ValidatorParentListReady config
    ((timed.execution.trace start).validatorState author) (maximum + 1)
      source.block.parents
  rebroadcasts : ∀ receiver,
    receiver < config.authorityCount →
    receiver ≠ author →
    ∃ (sentAt : Time) (packetId : PacketId)
        (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      start ≤ sentAt ∧
        sentAt ≤ start + 1 + timed.localActionBound ∧
        (timed.execution.trace (sentAt + 1)).packets packetId = some packet ∧
        protocolPacket packet ∧
        packet.sender = author ∧
        packet.receiver = receiver ∧
        packet.payload = .block source.block ∧
        packet.sentAt = sentAt + 1

/-- Pacemaker progress at one maximum owner. A newly produced block uses the
ordinary successor result. An already-signed exact successor uses its pinned
current-tip source and actual post-observation rebroadcast work. -/
inductive ValidatorOperationalMaximumPacemakerResult
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (recoveryWait start author maximum : Time) : Prop where
  | produced
      (result : ValidatorOperationalMaximumSuccessorResult timed obligations
        start author maximum) :
      ValidatorOperationalMaximumPacemakerResult timed obligations pins
        recoveryWait start author maximum
  | alreadySigned
      (carrier : ValidatorOperationalMaximumSignedTipCarrier pins recoveryWait
        start author maximum) :
      ValidatorOperationalMaximumPacemakerResult timed obligations pins
        recoveryWait start author maximum

/-- An already-signed exact successor at an active recovery owner gives its
current pinned body, accepted parent quorum, and actual peer rebroadcasts. -/
theorem operational_maximum_already_signed_tip_gives_carrier
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {recoveryWait start author maximum : Time}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (active : (timed.execution.trace start).epochActive = true)
    (frontierSource : ValidatorOperationalQuorumFrontierAt config
      (timed.execution.trace start) author maximum canonicalGenesisParents)
    (currentFloor : ((timed.execution.trace start).validatorState
      author).highestSignedRound = maximum + 1)
    (recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
      start author) :
    Nonempty (ValidatorOperationalMaximumSignedTipCarrier pins recoveryWait
      start author maximum) := by
  have successorPositive : 0 < maximum + 1 := Nat.zero_lt_succ maximum
  let source := Classical.choice
    (current_exact_round_gives_stable_recovery_tip_source pins authorInRange
      authorCorrectAvailable active successorPositive currentFloor recoveryMode)
  have targetMember : source.block ∈ source.entry.capsule.history := by
    have member := source.entry.capsule.target_and_parents_in_history.1
    simpa [source.targetBlock] using member
  have localTip := pins.pinnedHistoryIsLocal start author source.capsuleKey
    source.entry source.stored source.pinned source.block targetMember
  have parentsReadyRaw := pinned_current_tip_parents_are_ready pins sameGenesis
    authorInRange authorCorrectAvailable frontierSource source.stored
      source.pinned source.targetBlock
  have parentsReady : ValidatorParentListReady config
      ((timed.execution.trace start).validatorState author) (maximum + 1)
        source.block.parents := by
    simpa [source.exactRound] using parentsReadyRaw
  refine ⟨{
    source := source
    tipAccepted := localTip.1
    tipRetained := localTip.2.1
    tipCatalogued := localTip.2.2.1
    parentsReady := parentsReady
    rebroadcasts := ?_ }⟩
  intro receiver receiverInRange differentValidator
  exact broadcast.recovery_mode_sends_exact_current_tip_packet effects
    authorInRange authorCorrectAvailable receiverInRange differentValidator
      source.recoveryMode (by simpa [currentFloor] using source.roundPositive)
        (by simpa [currentFloor] using source.ownTip) localTip.2.2.1

/-- An already-signed successor carrier which does not depend on recovery
mode. Its exact current pin supplies the body and causal source. Each correct
receiver's subscription replays this block or a newer current tip. -/
structure ValidatorOperationalMaximumCurrentTipCarrier
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (start author maximum : Time) where
  block : ValidatorBlock BlockId
  capsuleKey : ValidatorRecoveryCapsuleKey BlockId
  entry : ValidatorPinnedRecoveryCapsule (BlockId := BlockId) config
  ownTip : ((timed.execution.trace start).validatorState author).ownBlockAt
    (maximum + 1) = some block.reference
  exactRound : block.reference.round = maximum + 1
  targetBlock : entry.capsule.targetBlock = block
  stored : (pins.trace start author).capsuleAt capsuleKey = some entry
  pinned : (pins.trace start author).pinned capsuleKey = true
  source : CausalRecoveryCapsuleExecutionSource syncRules entry.capsule author
    start
  tipAccepted : ((timed.execution.trace start).validatorState author).accepted
    block.reference = true
  tipRetained : ((timed.execution.trace start).validatorState author).retained
    block.reference = true
  tipCatalogued : (timed.execution.trace start).blockCatalog
    block.reference.id = some block
  parentsReady : ValidatorParentListReady config
    ((timed.execution.trace start).validatorState author) (maximum + 1)
      block.parents
  subscriptionReplays : ∀ receiver,
    receiver < config.authorityCount →
    faults.correctAvailable receiver = true →
    receiver ≠ author →
    ValidatorCurrentTipSubscriptionDispositionAt timed start author receiver
      block

/-- Arbitrary-mode pacemaker progress at one operational maximum owner. -/
inductive ValidatorOperationalMaximumCurrentPacemakerResult
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (start author maximum : Time) : Prop where
  | produced
      (result : ValidatorOperationalMaximumSuccessorResult timed obligations
        start author maximum) :
      ValidatorOperationalMaximumCurrentPacemakerResult timed obligations pins
        start author maximum
  | alreadySigned
      (carrier : ValidatorOperationalMaximumCurrentTipCarrier pins start author
        maximum) :
      ValidatorOperationalMaximumCurrentPacemakerResult timed obligations pins
        start author maximum

/-- An already-signed exact successor gives a current pinned carrier in normal
or recovery mode. -/
theorem operational_maximum_already_signed_current_tip_gives_carrier
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {start author maximum : Time}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (subscription : ValidatorCurrentTipSubscriptionExecution pins)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (frontierSource : ValidatorOperationalQuorumFrontierAt config
      (timed.execution.trace start) author maximum canonicalGenesisParents)
    (currentFloor : ((timed.execution.trace start).validatorState
      author).highestSignedRound = maximum + 1) :
    Nonempty (ValidatorOperationalMaximumCurrentTipCarrier pins start author
      maximum) := by
  have successorPositive : 0 <
      ((timed.execution.trace start).validatorState
        author).highestSignedRound := by
    rw [currentFloor]
    exact Nat.zero_lt_succ maximum
  rcases pins.current_positive_tip_has_pinned_capsule_source authorInRange
      authorCorrectAvailable (active start (Nat.le_refl _)) successorPositive with
    ⟨block, capsuleKey, entry, ownAtFloor, targetBlock, stored, pinned, source⟩
  have ownTip : ((timed.execution.trace start).validatorState author).ownBlockAt
      (maximum + 1) = some block.reference := by
    rw [← currentFloor]
    exact ownAtFloor
  have exactRound : block.reference.round = maximum + 1 :=
    (timed.execution.statesWellFormed start author authorInRange
      ).ownBlockIsSound (maximum + 1) block.reference ownTip |>.2.1
  have targetMember : block ∈ entry.capsule.history := by
    have member := entry.capsule.target_and_parents_in_history.1
    simpa [targetBlock] using member
  have localTip := pins.pinnedHistoryIsLocal start author capsuleKey entry
    stored pinned block targetMember
  have parentsReadyRaw := pinned_current_tip_parents_are_ready pins sameGenesis
    authorInRange authorCorrectAvailable frontierSource stored pinned targetBlock
  have parentsReady : ValidatorParentListReady config
      ((timed.execution.trace start).validatorState author) (maximum + 1)
        block.parents := by
    simpa [exactRound] using parentsReadyRaw
  refine ⟨{
    block := block
    capsuleKey := capsuleKey
    entry := entry
    ownTip := ownTip
    exactRound := exactRound
    targetBlock := targetBlock
    stored := stored
    pinned := pinned
    source := source
    tipAccepted := localTip.1
    tipRetained := localTip.2.1
    tipCatalogued := localTip.2.2.1
    parentsReady := parentsReady
    subscriptionReplays := ?_ }⟩
  intro receiver receiverInRange receiverCorrectAvailable differentValidator
  exact subscription.currentPinnedTipHasSubscriptionDisposition start author
    receiver block capsuleKey entry authorInRange authorCorrectAvailable
      receiverInRange receiverCorrectAvailable differentValidator afterGst active
        successorPositive ownAtFloor targetBlock stored pinned

/-- The correct owner of the current operational maximum either broadcasts the
exact successor child or exposes a public total-quorum layer at that successor
or later.

The theorem covers the branch in which the owner's signer floor is below the
successor target. A separate current-tip source handles the case in which that
owner already persisted the successor. -/
theorem operational_maximum_pacemaker_gives_successor
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (pacemaker : ValidatorNormalFrontierPacemakerRules timed)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start : Time}
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (maximumOwnerFloorBelowSuccessor : ∀ holder,
      holder < config.authorityCount →
      faults.correctAvailable holder = true →
      frontiers.frontier start holder =
        correctOperationalQuorumFrontierMaximumUpTo frontiers start
          config.authorityCount →
      ((timed.execution.trace start).validatorState
        holder).highestSignedRound <
          correctOperationalQuorumFrontierMaximumUpTo frontiers start
            config.authorityCount + 1) :
    ∃ holder,
      holder < config.authorityCount ∧
        faults.correctAvailable holder = true ∧
        ValidatorOperationalMaximumSuccessorResult timed obligations start
          holder (correctOperationalQuorumFrontierMaximumUpTo frontiers start
            config.authorityCount) := by
  let maximum := correctOperationalQuorumFrontierMaximumUpTo frontiers start
    config.authorityCount
  rcases correct_operational_quorum_frontier_maximum_gives_successor_source
      frontiers (active start (Nat.le_refl start)) with
    ⟨holder, localSource, holderInRange, holderCorrect, holderMaximum,
      clockAtTarget, parentsReady⟩
  have floorBelowTarget := maximumOwnerFloorBelowSuccessor holder holderInRange
    holderCorrect holderMaximum
  have protectedWork := pacemaker.readyFrontierStartsProtectedWork start holder
    (maximum + 1) localSource.quorum.references holderInRange holderCorrect
      (active start (Nat.le_refl start)) (by simpa [maximum] using clockAtTarget)
        (by simpa [maximum] using floorBelowTarget)
          (by simpa [maximum] using parentsReady)
  rcases normal_frontier_pacemaker_eventually_broadcasts_or_clock_advances
      pacemaker latchSource effects authorityCountAtLeastTwo holderInRange
        holderCorrect (by simpa [maximum] using floorBelowTarget) protectedWork
          active with
    broadcast | ⟨advancedAt, startBeforeAdvanced, targetPassed⟩
  · exact ⟨holder, holderInRange, holderCorrect,
      at_or_above_broadcast_gives_operational_maximum_successor frontiers
        holderInRange holderCorrect active broadcast⟩
  · have activeAtAdvanced := active advancedAt startBeforeAdvanced
    change maximum + 1 <
      ((timed.execution.trace advancedAt).validatorState
        holder).thresholdClockRound at targetPassed
    rcases frontiers.currentSource advancedAt holder holderInRange holderCorrect
        activeAtAdvanced with ⟨advancedSource⟩
    have successorAtMostFrontier : maximum + 1 ≤
        frontiers.frontier advancedAt holder := by
      have clockExact := advancedSource.thresholdClockAtSuccessor
      omega
    have frontierPositive : 0 < frontiers.frontier advancedAt holder := by
      exact Nat.lt_of_lt_of_le (Nat.zero_lt_succ maximum)
        successorAtMostFrontier
    have layer :=
      positive_operational_frontier_gives_correct_held_total_quorum_layer
        holderInRange holderCorrect frontierPositive advancedSource
    exact ⟨holder, holderInRange, holderCorrect,
      .laterLayer advancedAt (frontiers.frontier advancedAt holder)
        startBeforeAdvanced successorAtMostFrontier layer⟩

/-- At a recovery snapshot, the operational maximum owner has a complete
successor disposition.

If its signer floor is below `H + 1`, the normal pacemaker produces the usual
successor result. If it already signed `H + 1`, the current pinned exact tip is
rebroadcast after the snapshot. The signer floor cannot be above `H + 1`
because its own valid parent quorum would contradict the local frontier. -/
theorem operational_maximum_recovery_pacemaker_gives_successor
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {recoveryWait : Time}
    (tipRebroadcast : ValidatorRecoveryTipRebroadcastExecution pins
      recoveryWait)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    (pacemaker : ValidatorNormalFrontierPacemakerRules timed)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed recoveryWait start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ∃ holder,
      holder < config.authorityCount ∧
        faults.correctAvailable holder = true ∧
        ValidatorOperationalMaximumPacemakerResult timed obligations pins
          recoveryWait start holder
            (correctOperationalQuorumFrontierMaximumUpTo frontiers start
              config.authorityCount) := by
  let maximum := correctOperationalQuorumFrontierMaximumUpTo frontiers start
    config.authorityCount
  rcases correct_operational_quorum_frontier_maximum_gives_successor_source
      frontiers (active start (Nat.le_refl start)) with
    ⟨holder, localSource, holderInRange, holderCorrect, holderMaximum,
      clockAtTarget, parentsReady⟩
  have floorAtMostTarget :
      ((timed.execution.trace start).validatorState
        holder).highestSignedRound ≤ maximum + 1 := by
    have bounded := operational_maximum_owner_signer_floor_le_successor pins
      sameGenesis holderInRange holderCorrect (active start (Nat.le_refl start))
        localSource
    simpa [maximum, holderMaximum] using bounded
  by_cases floorBelowTarget :
      ((timed.execution.trace start).validatorState
        holder).highestSignedRound < maximum + 1
  · have protectedWork := pacemaker.readyFrontierStartsProtectedWork start
      holder (maximum + 1) localSource.quorum.references holderInRange
        holderCorrect (active start (Nat.le_refl start))
          (by simpa [maximum] using clockAtTarget)
            floorBelowTarget (by simpa [maximum] using parentsReady)
    rcases normal_frontier_pacemaker_eventually_broadcasts_or_clock_advances
        pacemaker latchSource effects authorityCountAtLeastTwo holderInRange
          holderCorrect floorBelowTarget protectedWork active with
      broadcast | ⟨advancedAt, startBeforeAdvanced, targetPassed⟩
    · exact ⟨holder, holderInRange, holderCorrect, .produced
        (at_or_above_broadcast_gives_operational_maximum_successor frontiers
          holderInRange holderCorrect active broadcast)⟩
    · have activeAtAdvanced := active advancedAt startBeforeAdvanced
      change maximum + 1 <
        ((timed.execution.trace advancedAt).validatorState
          holder).thresholdClockRound at targetPassed
      rcases frontiers.currentSource advancedAt holder holderInRange
          holderCorrect activeAtAdvanced with ⟨advancedSource⟩
      have successorAtMostFrontier : maximum + 1 ≤
          frontiers.frontier advancedAt holder := by
        have clockExact := advancedSource.thresholdClockAtSuccessor
        omega
      have frontierPositive : 0 < frontiers.frontier advancedAt holder :=
        Nat.lt_of_lt_of_le (Nat.zero_lt_succ maximum)
          successorAtMostFrontier
      have layer :=
        positive_operational_frontier_gives_correct_held_total_quorum_layer
          holderInRange holderCorrect frontierPositive advancedSource
      exact ⟨holder, holderInRange, holderCorrect, .produced
        (.laterLayer advancedAt (frontiers.frontier advancedAt holder)
          startBeforeAdvanced successorAtMostFrontier layer)⟩
  · have floorAtTarget :
        ((timed.execution.trace start).validatorState
          holder).highestSignedRound = maximum + 1 := by
      omega
    have floorAtLocalTarget :
        ((timed.execution.trace start).validatorState
          holder).highestSignedRound =
            frontiers.frontier start holder + 1 := by
      rw [holderMaximum]
      exact floorAtTarget
    let localCarrier := Classical.choice
      (operational_maximum_already_signed_tip_gives_carrier pins tipRebroadcast
        effects sameGenesis holderInRange holderCorrect
          (active start (Nat.le_refl start)) localSource floorAtLocalTarget
            (recovery.recovering holder holderInRange holderCorrect))
    have carrier : ValidatorOperationalMaximumSignedTipCarrier pins recoveryWait
        start holder maximum := by
      simpa only [holderMaximum] using localCarrier
    exact ⟨holder, holderInRange, holderCorrect, .alreadySigned carrier⟩

/-- One correct host at an exact operational frontier `H` produces its local
successor carrier, replays an already-signed exact successor, or exposes a
later public layer.

This pointwise theorem is suitable for correct-stake aggregation. It does not
select a global owner and another validator's commit cannot close its result. -/
theorem operational_frontier_host_current_tip_pacemaker_gives_successor
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (tipSubscription : ValidatorCurrentTipSubscriptionExecution pins)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    (pacemaker : ValidatorNormalFrontierPacemakerRules timed)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start holder base : Time}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (frontierExact : frontiers.frontier start holder = base)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorOperationalMaximumCurrentPacemakerResult timed obligations pins
      start holder base := by
  rcases frontiers.currentSource start holder holderInRange
      holderCorrectAvailable (active start (Nat.le_refl start)) with
    ⟨rawSource⟩
  have localSource : ValidatorOperationalQuorumFrontierAt config
      (timed.execution.trace start) holder base canonicalGenesisParents := by
    simpa only [frontierExact] using rawSource
  have floorAtMostTarget :
      ((timed.execution.trace start).validatorState
        holder).highestSignedRound ≤ base + 1 :=
    operational_maximum_owner_signer_floor_le_successor pins sameGenesis
      holderInRange holderCorrectAvailable (active start (Nat.le_refl start))
        localSource
  by_cases floorBelowTarget :
      ((timed.execution.trace start).validatorState
        holder).highestSignedRound < base + 1
  · have protectedWork := pacemaker.readyFrontierStartsProtectedWork start
      holder (base + 1) localSource.quorum.references holderInRange
        holderCorrectAvailable (active start (Nat.le_refl start))
          localSource.thresholdClockAtSuccessor floorBelowTarget
            localSource.successorParentListReady
    rcases normal_frontier_pacemaker_eventually_broadcasts_or_clock_advances
        pacemaker latchSource effects authorityCountAtLeastTwo holderInRange
          holderCorrectAvailable floorBelowTarget protectedWork active with
      broadcast | ⟨advancedAt, startBeforeAdvanced, targetPassed⟩
    · exact .produced
        (at_or_above_broadcast_gives_operational_successor frontiers
          holderInRange holderCorrectAvailable active broadcast)
    · have activeAtAdvanced := active advancedAt startBeforeAdvanced
      change base + 1 <
        ((timed.execution.trace advancedAt).validatorState
          holder).thresholdClockRound at targetPassed
      rcases frontiers.currentSource advancedAt holder holderInRange
          holderCorrectAvailable activeAtAdvanced with ⟨advancedSource⟩
      have successorAtMostFrontier : base + 1 ≤
          frontiers.frontier advancedAt holder := by
        have targetPassedFrontier : base + 1 <
            frontiers.frontier advancedAt holder + 1 := by
          simpa only [advancedSource.thresholdClockAtSuccessor] using
            targetPassed
        have baseBelowFrontier : base <
            frontiers.frontier advancedAt holder := by
          exact Nat.lt_of_succ_lt_succ (by
            simpa [Nat.succ_eq_add_one] using targetPassedFrontier)
        exact Nat.succ_le_iff.mpr baseBelowFrontier
      have frontierPositive : 0 < frontiers.frontier advancedAt holder :=
        Nat.lt_of_lt_of_le (Nat.zero_lt_succ base) successorAtMostFrontier
      have layer :=
        positive_operational_frontier_gives_correct_held_total_quorum_layer
          holderInRange holderCorrectAvailable frontierPositive advancedSource
      exact .produced
        (.laterLayer advancedAt (frontiers.frontier advancedAt holder)
          startBeforeAdvanced successorAtMostFrontier layer)
  · have floorAtTarget :
        ((timed.execution.trace start).validatorState
          holder).highestSignedRound = base + 1 := by
      omega
    exact .alreadySigned (Classical.choice
      (operational_maximum_already_signed_current_tip_gives_carrier pins
        tipSubscription sameGenesis holderInRange holderCorrectAvailable
          afterGst active localSource floorAtTarget))

/-- At any active observation, the operational maximum owner has a complete
successor disposition.

This theorem removes the recovery-mode restriction from the already-signed
case. Each correct receiver's subscription replays the exact pinned `H + 1`
tip or a newer current tip. No future delivery or layer is an input. -/
theorem operational_maximum_current_tip_pacemaker_gives_successor
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (tipSubscription : ValidatorCurrentTipSubscriptionExecution pins)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    (pacemaker : ValidatorNormalFrontierPacemakerRules timed)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start : Time}
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ∃ holder,
      holder < config.authorityCount ∧
        faults.correctAvailable holder = true ∧
        ValidatorOperationalMaximumCurrentPacemakerResult timed obligations
          pins start holder
            (correctOperationalQuorumFrontierMaximumUpTo frontiers start
              config.authorityCount) := by
  let maximum := correctOperationalQuorumFrontierMaximumUpTo frontiers start
    config.authorityCount
  rcases correct_operational_quorum_frontier_maximum_gives_successor_source
      frontiers (active start (Nat.le_refl start)) with
    ⟨holder, localSource, holderInRange, holderCorrect, holderMaximum,
      clockAtTarget, parentsReady⟩
  have floorAtMostTarget :
      ((timed.execution.trace start).validatorState
        holder).highestSignedRound ≤ maximum + 1 := by
    have bounded := operational_maximum_owner_signer_floor_le_successor pins
      sameGenesis holderInRange holderCorrect (active start (Nat.le_refl start))
        localSource
    simpa [maximum, holderMaximum] using bounded
  by_cases floorBelowTarget :
      ((timed.execution.trace start).validatorState
        holder).highestSignedRound < maximum + 1
  · have protectedWork := pacemaker.readyFrontierStartsProtectedWork start
      holder (maximum + 1) localSource.quorum.references holderInRange
        holderCorrect (active start (Nat.le_refl start))
          (by simpa [maximum] using clockAtTarget)
            floorBelowTarget (by simpa [maximum] using parentsReady)
    rcases normal_frontier_pacemaker_eventually_broadcasts_or_clock_advances
        pacemaker latchSource effects authorityCountAtLeastTwo holderInRange
          holderCorrect floorBelowTarget protectedWork active with
      broadcast | ⟨advancedAt, startBeforeAdvanced, targetPassed⟩
    · exact ⟨holder, holderInRange, holderCorrect, .produced
        (at_or_above_broadcast_gives_operational_maximum_successor frontiers
          holderInRange holderCorrect active broadcast)⟩
    · have activeAtAdvanced := active advancedAt startBeforeAdvanced
      change maximum + 1 <
        ((timed.execution.trace advancedAt).validatorState
          holder).thresholdClockRound at targetPassed
      rcases frontiers.currentSource advancedAt holder holderInRange
          holderCorrect activeAtAdvanced with ⟨advancedSource⟩
      have successorAtMostFrontier : maximum + 1 ≤
          frontiers.frontier advancedAt holder := by
        have clockExact := advancedSource.thresholdClockAtSuccessor
        omega
      have frontierPositive : 0 < frontiers.frontier advancedAt holder :=
        Nat.lt_of_lt_of_le (Nat.zero_lt_succ maximum)
          successorAtMostFrontier
      have layer :=
        positive_operational_frontier_gives_correct_held_total_quorum_layer
          holderInRange holderCorrect frontierPositive advancedSource
      exact ⟨holder, holderInRange, holderCorrect, .produced
        (.laterLayer advancedAt (frontiers.frontier advancedAt holder)
          startBeforeAdvanced successorAtMostFrontier layer)⟩
  · have floorAtTarget :
        ((timed.execution.trace start).validatorState
          holder).highestSignedRound = maximum + 1 := by
      omega
    have floorAtLocalTarget :
        ((timed.execution.trace start).validatorState
          holder).highestSignedRound =
            frontiers.frontier start holder + 1 := by
      rw [holderMaximum]
      exact floorAtTarget
    let localCarrier := Classical.choice
      (operational_maximum_already_signed_current_tip_gives_carrier pins
        tipSubscription sameGenesis holderInRange holderCorrect afterGst active
          localSource floorAtLocalTarget)
    have carrier : ValidatorOperationalMaximumCurrentTipCarrier pins start
        holder maximum := by
      simpa only [holderMaximum] using localCarrier
    exact ⟨holder, holderInRange, holderCorrect, .alreadySigned carrier⟩

end Mysticeti
