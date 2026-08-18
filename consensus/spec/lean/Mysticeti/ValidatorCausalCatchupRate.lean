/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorOperationalFrontierCollectiveSuccessor
import Mysticeti.ValidatorAdjacentRecoveryPropagation
import Mysticeti.ValidatorActionScopedLeaderParent
import Mysticeti.ValidatorPacing

namespace Mysticeti

/-! Rate bounds for causal-history catch-up.

One work unit is the worst synchronization work introduced by one new causal
round. The envelope does not assume that a future block, layer, or commit
exists. It only bounds how quickly already possible missing-history work can
grow as protocol rounds increase.

The recovery wait has two uses. Its total duration eventually covers the old
backlog. Its next-round increase eventually covers one bounded new causal
round. A higher theorem must map an exact proposal capsule to this proof-only
work envelope.
-/

/-- If no correct, available validator advances after `start`, one selected
correct validator keeps its exact commit head. -/
private theorem no_advance_keeps_exact_head_for_catchup
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start later validator : Time} {prior : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ordered : start ≤ later)
    (headAtStart :
      ((timed.execution.trace start).validatorState validator).commitHead = prior)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start) :
    ((timed.execution.trace later).validatorState validator).commitHead = prior := by
  have durable := timed.execution.durableStateMonotone validator start later
    validatorInRange ordered
  have notStrict : ¬prior.index <
      ((timed.execution.trace later).validatorState validator).commitHead.index := by
    intro advanced
    exact noAdvance ⟨validator, later, validatorInRange,
      validatorCorrectAvailable, ordered, by simpa [headAtStart] using advanced⟩
  have sameIndex :
      ((timed.execution.trace start).validatorState validator).commitHead.index =
        ((timed.execution.trace later).validatorState validator).commitHead.index := by
    have monotone := durable.1
    rw [headAtStart] at monotone ⊢
    omega
  exact (durable.2.2.1 sameIndex).symm.trans headAtStart

/-- A proof-only upper envelope for missing causal-history work.

`missingRoundWork round` counts worst-case round-sized work units. It can count
an exact missing suffix from the previous accepted block and can omit items at
or below the receiver's current GC round. -/
structure ValidatorCausalCatchupEnvelope where
  startRound : Nat
  missingRoundWork : Nat → Nat
  newWorkPerRound : Nat
  workGrowthBound : ∀ round,
    startRound ≤ round →
      missingRoundWork (round + 1) ≤
        missingRoundWork round + newWorkPerRound

namespace ValidatorCausalCatchupEnvelope

/-- Time needed to process the envelope at one round. -/
def catchupCost
    (envelope : ValidatorCausalCatchupEnvelope)
    (costPerWorkUnit round : Nat) : Nat :=
  envelope.missingRoundWork round * costPerWorkUnit

/-- A fixed per-round work-growth bound gives a fixed per-round cost-growth
bound. -/
theorem catchup_cost_has_bounded_increase
    (envelope : ValidatorCausalCatchupEnvelope)
    (costPerWorkUnit : Nat) :
    ∀ round,
      envelope.startRound ≤ round →
        envelope.catchupCost costPerWorkUnit (round + 1) ≤
          envelope.catchupCost costPerWorkUnit round +
            envelope.newWorkPerRound * costPerWorkUnit := by
  intro round startBeforeRound
  have growth := envelope.workGrowthBound round startBeforeRound
  have multiplied := Nat.mul_le_mul_right costPerWorkUnit growth
  simpa [catchupCost, Nat.add_mul, Nat.add_assoc] using multiplied

/-- The growing wait eventually covers the complete old causal-history
backlog and any fixed local overhead. -/
theorem wait_eventually_covers_old_backlog
    {CommitPrefix : Type}
    (schedule : CommonRoundWaitSchedule CommitPrefix)
    (commitHead : CommitPrefix)
    (envelope : ValidatorCausalCatchupEnvelope)
    (costPerWorkUnit fixedOverhead : Nat) :
    ∃ firstRound,
      envelope.startRound ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            envelope.catchupCost costPerWorkUnit round + fixedOverhead ≤
              schedule.wait commitHead round := by
  exact schedule.permanent_margin_eventually_dominates_bounded_increase
    commitHead (envelope.catchupCost costPerWorkUnit) envelope.startRound
      (envelope.newWorkPerRound * costPerWorkUnit) fixedOverhead
        (envelope.catchup_cost_has_bounded_increase costPerWorkUnit)

/-- The next-round wait margin eventually covers the bounded new causal work
introduced by one protocol round, plus a fixed delivery and local-processing
cost. -/
theorem wait_eventually_covers_one_round_delta
    {CommitPrefix : Type}
    (schedule : CommonRoundWaitSchedule CommitPrefix)
    (commitHead : CommitPrefix)
    (envelope : ValidatorCausalCatchupEnvelope)
    (costPerWorkUnit fixedVisibilityCost : Nat) :
    ∃ firstRound,
      ∀ round,
        firstRound ≤ round →
          schedule.wait commitHead round +
              (envelope.newWorkPerRound * costPerWorkUnit +
                fixedVisibilityCost) ≤
            schedule.wait commitHead (round + 1) := by
  exact schedule.eventually_covers_same_head_visibility commitHead
    (envelope.newWorkPerRound * costPerWorkUnit + fixedVisibilityCost)

/-- One late suffix both clears the old backlog during a round wait and covers
the bounded causal-history delta before the next-round snapshot. -/
theorem wait_eventually_covers_backlog_and_next_delta
    {CommitPrefix : Type}
    (schedule : CommonRoundWaitSchedule CommitPrefix)
    (commitHead : CommitPrefix)
    (envelope : ValidatorCausalCatchupEnvelope)
    (costPerWorkUnit oldBacklogOverhead nextVisibilityCost : Nat) :
    ∃ firstRound,
      envelope.startRound ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            envelope.catchupCost costPerWorkUnit round +
                oldBacklogOverhead ≤ schedule.wait commitHead round ∧
              schedule.wait commitHead round +
                  (envelope.newWorkPerRound * costPerWorkUnit +
                    nextVisibilityCost) ≤
                schedule.wait commitHead (round + 1) := by
  rcases envelope.wait_eventually_covers_old_backlog schedule commitHead
      costPerWorkUnit oldBacklogOverhead with
    ⟨backlogStart, envelopeBeforeBacklog, backlogCovered⟩
  rcases envelope.wait_eventually_covers_one_round_delta schedule commitHead
      costPerWorkUnit nextVisibilityCost with
    ⟨deltaStart, deltaCovered⟩
  let firstRound := max backlogStart deltaStart
  refine ⟨firstRound,
    Nat.le_trans envelopeBeforeBacklog (Nat.le_max_left _ _), ?_⟩
  intro round firstBeforeRound
  exact ⟨
    backlogCovered round
      (Nat.le_trans (Nat.le_max_left _ _) firstBeforeRound),
    deltaCovered round
      (Nat.le_trans (Nat.le_max_right _ _) firstBeforeRound)⟩

end ValidatorCausalCatchupEnvelope

/-- Current source-to-rate mapping for a protected causal capsule.

This is a local size and processing envelope. It does not state that a future
request, response, acceptance, proposal, layer, or commit exists. -/
structure ValidatorCausalCapsuleCatchupRateRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (syncRules : ValidatorBlockSyncExecutionRules timed) where
  envelope : ValidatorCausalCatchupEnvelope
  capsuleHistoryWithinEnvelope : ∀
    {capsule : CausalRecoveryCapsule (BlockId := BlockId) config}
    {holder time round : Nat},
    CausalRecoveryCapsuleExecutionSource syncRules capsule holder time →
    capsule.targetRound = round →
    envelope.startRound ≤ round →
    capsule.history.length ≤ envelope.missingRoundWork round

/-- The quantitative rate comparison between causal catch-up and protocol
round advancement.

This record does not state that a future packet, block, layer, or commit
exists. It states only that sufficiently late adjacent-round wait margins can
pay for one already-known causal catch-up envelope and any fixed pipeline
cost. -/
structure ValidatorCausalCatchupRateDominatesWait
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
    (rate : ValidatorCausalCapsuleCatchupRateRules syncRules)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)) where
  eventualCatchupMargin : ∀ commitHead fixedPipelineCost,
    ∃ firstRound,
      rate.envelope.startRound ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            waits.wait commitHead round +
                (fixedPipelineCost + rate.envelope.catchupCost
                  (validatorBlockSyncAcceptanceBound timed syncRules) round) ≤
              waits.wait commitHead (round + 1)

namespace ValidatorCausalCapsuleCatchupRateRules

/-- The rate comparison supplies the exact adjacent-round visibility margin
used by causal parent propagation. -/
theorem eventual_adjacent_catchup_margin
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
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (rate : ValidatorCausalCapsuleCatchupRateRules syncRules)
    (dominates : ValidatorCausalCatchupRateDominatesWait rate waits)
    (commitHead : ValidatorCommitHead CommitId)
    (startDifference : Nat) :
    ∃ firstRound,
      rate.envelope.startRound ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            waits.wait commitHead round +
                (startDifference + 3 * (timed.localActionBound + 1) +
                  network.delta + 1 + rate.envelope.catchupCost
                    (validatorBlockSyncAcceptanceBound timed syncRules) round +
                  timed.localActionBound + 1) ≤
              waits.wait commitHead (round + 1) := by
  let fixedPipelineCost := startDifference +
    3 * (timed.localActionBound + 1) + network.delta + 1 +
      timed.localActionBound + 1
  rcases dominates.eventualCatchupMargin commitHead fixedPipelineCost with
    ⟨firstRound, envelopeBeforeFirst, margin⟩
  refine ⟨firstRound, envelopeBeforeFirst, ?_⟩
  intro round firstBeforeRound
  have covered := margin round firstBeforeRound
  simpa only [fixedPipelineCost, Nat.add_assoc, Nat.add_comm,
    Nat.add_left_comm] using covered

/-- If no correct validator installs a commit, one validator keeps the same
commit head and the growing wait eventually pays the complete adjacent-round
catch-up cost.

This is the stable-head branch of the commit-liveness proof. Network-round
progress can select a round after `firstRound`. A favorable leader window at
or after that round then has enough time to deliver and accept the leader
before the next proposal snapshot. If a correct validator installs a commit,
the caller uses that install as commit progress and starts again from the new
head. -/
theorem no_commit_suffix_eventually_covers_adjacent_catchup
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
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (rate : ValidatorCausalCapsuleCatchupRateRules syncRules)
    (dominates : ValidatorCausalCatchupRateDominatesWait rate waits)
    {start validator startDifference : Nat}
    {prior : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (headAtStart :
      ((timed.execution.trace start).validatorState validator).commitHead =
        prior)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start) :
    ∃ firstRound,
      rate.envelope.startRound ≤ firstRound ∧
        (∀ later, start ≤ later →
          ((timed.execution.trace later).validatorState validator).commitHead =
            prior) ∧
        ∀ round,
          firstRound ≤ round →
            waits.wait prior round +
                (startDifference + 3 * (timed.localActionBound + 1) +
                  network.delta + 1 + rate.envelope.catchupCost
                    (validatorBlockSyncAcceptanceBound timed syncRules) round +
                  timed.localActionBound + 1) ≤
              waits.wait prior (round + 1) := by
  rcases rate.eventual_adjacent_catchup_margin dominates prior
      startDifference with
    ⟨firstRound, envelopeBeforeFirst, margin⟩
  refine ⟨firstRound, envelopeBeforeFirst, ?_, margin⟩
  intro later startBeforeLater
  exact no_advance_keeps_exact_head_for_catchup validatorInRange
    validatorCorrectAvailable startBeforeLater headAtStart noAdvance

/-- The concrete sequential parent-sync bound for one source capsule is no
larger than the rate envelope's catch-up cost. -/
theorem capsule_sync_cost_within_envelope
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
    (rules : ValidatorCausalCapsuleCatchupRateRules syncRules)
    {capsule : CausalRecoveryCapsule (BlockId := BlockId) config}
    {holder time round : Nat}
    (source : CausalRecoveryCapsuleExecutionSource syncRules capsule holder time)
    (targetRound : capsule.targetRound = round)
    (envelopeStarted : rules.envelope.startRound ≤ round) :
    capsule.history.length *
          validatorBlockSyncAcceptanceBound timed syncRules ≤
      rules.envelope.catchupCost
        (validatorBlockSyncAcceptanceBound timed syncRules) round := by
  exact Nat.mul_le_mul_right _
    (rules.capsuleHistoryWithinEnvelope source targetRound envelopeStarted)

/-- Erase timer-specific packet data while retaining the exact block packet and
source-side catalog entry. -/
theorem timer_paced_peer_broadcast_is_completed
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {validator receiver round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      validator receiver production.proposalActionAt) :
    ValidatorCompletedBlockBroadcast timed production.snapshot.block validator
      receiver broadcast.packetId broadcast.packet := by
  refine {
    packetInTrace := broadcast.packetInTrace
    packetIsProtocol := broadcast.packetIsProtocol
    packetSender := broadcast.packetSender
    packetReceiver := broadcast.packetReceiver
    packetPayload := broadcast.packetPayload
    blockAuthor := production.snapshot.blockIsOwnProposal.trans production.proposer
    blockCataloguedAtSend := ?_ }
  exact timed.execution.blockCatalogMonotone production.snapshot.storedAt
    broadcast.packet.sentAt broadcast.storedBeforeSend
      production.snapshot.block.reference.id production.snapshot.block
        production.snapshot.blockInCatalog

/-- One timer-paced block is accepted, or becomes a current GC root, within the
rate envelope for its exact source-local causal capsule.

This theorem turns the abstract rate assumption into the concrete bound needed
by the later leader-before-next-snapshot proof. -/
theorem timer_paced_peer_broadcast_resolves_within_catchup_envelope
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (rate : ValidatorCausalCapsuleCatchupRateRules syncRules)
    {validator receiver round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      validator receiver production.proposalActionAt)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt)
    (active : ∀ time, production.persistTime + 1 ≤ time →
      (timed.execution.trace time).epochActive = true)
    (envelopeStarted : rate.envelope.startRound ≤ round) :
    ∃ acceptedAt,
      broadcast.packet.deliveredAt + 1 ≤ acceptedAt ∧
      acceptedAt ≤ broadcast.packet.deliveredAt + 1 +
          rate.envelope.catchupCost
            (validatorBlockSyncAcceptanceBound timed syncRules) round +
            timed.localActionBound + 1 ∧
      (((timed.execution.trace acceptedAt).validatorState receiver).accepted
          production.snapshot.block.reference = true ∨
        production.snapshot.block.reference.round ≤
          ((timed.execution.trace acceptedAt).validatorState receiver).gcRound) := by
  have validatorInRange : validator < config.authorityCount := by
    simpa [production.proposer] using production.snapshot.proposerInRange
  have validatorCorrectAvailable :
      faults.correctAvailable validator = true := by
    simpa [production.proposer] using
      production.snapshot.proposerCorrectAvailable
  have sourceActive :
      (timed.execution.trace (production.persistTime + 1)).epochActive = true :=
    active _ (Nat.le_refl _)
  rcases timer_paced_round_production_has_pinned_capsule_source pins production
      sourceActive with
    ⟨capsuleId, entry, targetBlock, stored, pinned, capsuleSource⟩
  have sourceBeforeSend : production.persistTime + 1 ≤
      broadcast.packet.sentAt := by
    rw [← production.storedAfterPersistence]
    exact broadcast.storedBeforeSend
  have activeFromSource : ∀ time, production.persistTime + 1 ≤ time →
      (timed.execution.trace time).epochActive = true := active
  let completed := timer_paced_peer_broadcast_is_completed production broadcast
  have syncSource := pinned_successor_and_live_goals_give_block_parent_sync_source
    pins frontiers sameGenesis validatorInRange validatorCorrectAvailable
      receiverInRange receiverCorrectAvailable targetBlock stored pinned completed
        sourceBeforeSend sentAfterGst activeFromSource
  rcases completed_block_broadcast_accepted_or_gc_root_within_parent_sync_bound
      timed acceptance syncRules validatorInRange validatorCorrectAvailable
        receiverInRange receiverCorrectAvailable completed production.validParents
          sentAfterGst (by
            intro time deliveryBeforeTime
            exact active time (Nat.le_trans
              (Nat.le_trans sourceBeforeSend
                (Nat.le_trans
                  (network.postGstDelivery broadcast.packet
                    broadcast.packetIsProtocol
                    (by simpa [broadcast.packetSender] using validatorInRange)
                    (by simpa [broadcast.packetReceiver] using receiverInRange)
                    (by simpa [broadcast.packetSender] using
                      validatorCorrectAvailable)
                    (by simpa [broadcast.packetReceiver] using
                      receiverCorrectAvailable)
                    sentAfterGst).1
                  (Nat.le_add_right _ _)))
              deliveryBeforeTime))
          syncSource with
    ⟨acceptedAt, deliveryBeforeAccepted, acceptedBound, acceptedOrRoot⟩
  have targetRound : entry.capsule.targetRound = round := by
    unfold CausalRecoveryCapsule.targetRound
    rw [targetBlock, production.blockRound]
  have historyCostBound := rate.capsule_sync_cost_within_envelope capsuleSource
    targetRound envelopeStarted
  have envelopeBound :
      broadcast.packet.deliveredAt + 1 + entry.capsule.history.length *
            validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1 ≤
        broadcast.packet.deliveredAt + 1 +
            rate.envelope.catchupCost
              (validatorBlockSyncAcceptanceBound timed syncRules) round +
              timed.localActionBound + 1 := by
    have withStart := Nat.add_le_add_left historyCostBound
      (broadcast.packet.deliveredAt + 1)
    have withAcceptance := Nat.add_le_add_right withStart
      (timed.localActionBound + 1)
    simpa only [Nat.add_assoc] using withAcceptance
  exact ⟨acceptedAt, deliveryBeforeAccepted,
    Nat.le_trans acceptedBound envelopeBound, acceptedOrRoot⟩

/-- If the rate-envelope bound finishes before a later proposal snapshot and
the target is still above GC there, the exact timer-paced block is accepted at
that snapshot. -/
theorem timer_paced_peer_broadcast_is_accepted_before_snapshot
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (rate : ValidatorCausalCapsuleCatchupRateRules syncRules)
    {validator receiver round snapshotAt : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      validator receiver production.proposalActionAt)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt)
    (active : ∀ time, production.persistTime + 1 ≤ time →
      (timed.execution.trace time).epochActive = true)
    (envelopeStarted : rate.envelope.startRound ≤ round)
    (resolutionBeforeSnapshot :
      broadcast.packet.deliveredAt + 1 +
            rate.envelope.catchupCost
              (validatorBlockSyncAcceptanceBound timed syncRules) round +
            timed.localActionBound + 1 ≤ snapshotAt)
    (aboveGcAtSnapshot :
      ((timed.execution.trace snapshotAt).validatorState receiver).gcRound <
        production.snapshot.block.reference.round) :
    ((timed.execution.trace snapshotAt).validatorState receiver).accepted
        production.snapshot.block.reference = true := by
  rcases rate.timer_paced_peer_broadcast_resolves_within_catchup_envelope pins
      frontiers sameGenesis acceptance production broadcast receiverInRange
        receiverCorrectAvailable sentAfterGst active envelopeStarted with
    ⟨acceptedAt, _deliveryBeforeAccepted, acceptedBound, acceptedOrRoot⟩
  have acceptedBeforeSnapshot : acceptedAt ≤ snapshotAt :=
    Nat.le_trans acceptedBound resolutionBeforeSnapshot
  rcases acceptedOrRoot with accepted | atRoot
  · exact timed.execution.accepted_block_persists receiverInRange
      acceptedBeforeSnapshot accepted
  · have gcMonotone :=
      ValidatorRecoveryCapsuleSyncExecution.validator_gc_round_mono
        (timed := timed) receiverInRange acceptedBeforeSnapshot
    omega

/-- An accepted and retained correct timer-paced block gives the complete
exact parent evidence at the next-round proposal snapshot. -/
theorem accepted_retained_timer_paced_block_gives_parent_evidence
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {author receiver round : Nat}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (accepted :
      ((timed.execution.trace next.snapshot.snapshotAt).validatorState
        receiver).accepted previous.snapshot.block.reference = true)
    (retained :
      ((timed.execution.trace next.snapshot.snapshotAt).validatorState
        receiver).retained previous.snapshot.block.reference = true) :
    ValidatorAdjacentTimerPacedParentEvidence previous next := by
  have authorInRange : author < config.authorityCount := by
    simpa [previous.proposer] using previous.snapshot.proposerInRange
  have authorCorrect : faults.correctAvailable author = true := by
    simpa [previous.proposer] using
      previous.snapshot.proposerCorrectAvailable
  have authorNotByzantine : faults.byzantine author = false := by
    have notNonProgress : faults.nonProgress author = false := by
      simpa [FixedFaultInterval.correctAvailable, VoterSet.diff,
        VoterSet.full] using authorCorrect
    have separated : faults.byzantine author = false ∧
        faults.unavailable author = false := by
      simpa [FixedFaultInterval.nonProgress, VoterSet.union] using
        notNonProgress
    exact separated.1
  have receiverInRange : receiver < config.authorityCount := by
    simpa [next.proposer] using next.snapshot.proposerInRange
  have receiverCorrect : faults.correctAvailable receiver = true := by
    simpa [next.proposer] using next.snapshot.proposerCorrectAvailable
  have recorded := representatives.acceptedCorrectReferenceIsRecorded
    next.snapshot.snapshotAt receiver previous.snapshot.block.reference
      receiverInRange receiverCorrect
      (by simpa [previous.snapshot.blockIsOwnProposal, previous.proposer] using
        authorInRange)
      (by simpa [previous.snapshot.blockIsOwnProposal, previous.proposer] using
        authorNotByzantine)
      accepted
  have included := timer_paced_round_includes_retained_current_parent next
    authorInRange
    (by simpa [previous.snapshot.blockIsOwnProposal, previous.proposer,
      previous.blockRound] using recorded)
    retained
  have sound :=
    (timed.execution.statesWellFormed next.snapshot.snapshotAt receiver
      receiverInRange).acceptedRepresentativeIsSound round
        previous.snapshot.block.reference.author
        previous.snapshot.block.reference
        (by simpa [previous.blockRound] using recorded)
  rcases sound.2.2.2 with ⟨catalogBlock, catalogAtNext,
    _catalogBlockReference⟩
  have exactCatalog :
      (timed.execution.trace next.snapshot.snapshotAt).blockCatalog
          previous.snapshot.block.reference.id = some previous.snapshot.block := by
    have catalogBlockExact : catalogBlock = previous.snapshot.block := by
      by_cases storedBeforeNext : previous.snapshot.storedAt ≤
          next.snapshot.snapshotAt
      · have previousAtNext := timed.execution.blockCatalogMonotone
          previous.snapshot.storedAt next.snapshot.snapshotAt storedBeforeNext
            previous.snapshot.block.reference.id previous.snapshot.block
              previous.snapshot.blockInCatalog
        exact Option.some.inj (catalogAtNext.symm.trans previousAtNext)
      · have nextBeforeStored : next.snapshot.snapshotAt ≤
            previous.snapshot.storedAt := Nat.le_of_not_ge storedBeforeNext
        have catalogAtStored := timed.execution.blockCatalogMonotone
          next.snapshot.snapshotAt previous.snapshot.storedAt nextBeforeStored
            previous.snapshot.block.reference.id catalogBlock catalogAtNext
        exact Option.some.inj
          (catalogAtStored.symm.trans previous.snapshot.blockInCatalog)
    simpa [catalogBlockExact] using catalogAtNext
  exact {
    included
    representative := by simpa [previous.blockRound] using recorded
    retained
    catalog := exactCatalog }

/-- A large enough adjacent-round wait margin places the complete causal
catch-up bound before the next proposal snapshot. -/
theorem adjacent_wait_margin_covers_timer_paced_catchup
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (rate : ValidatorCausalCapsuleCatchupRateRules syncRules)
    {author receiver round startDifference : Nat}
    {previousPrior receiverPrior : ValidatorCommitHead CommitId}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (broadcast : ValidatorTimerPacedPeerBroadcast timed previous.snapshot
      author receiver previous.proposalActionAt)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt)
    (previousHead : previous.commitHead = previousPrior)
    (nextHead : next.commitHead = receiverPrior)
    (previousStartBound :
      previous.timerStartedAt ≤ next.timerStartedAt + startDifference)
    (visibilityMargin :
      waits.wait previousPrior round +
          (startDifference + 3 * (timed.localActionBound + 1) + network.delta +
            1 + rate.envelope.catchupCost
              (validatorBlockSyncAcceptanceBound timed syncRules) round +
            timed.localActionBound + 1) ≤
        waits.wait receiverPrior (round + 1)) :
    broadcast.packet.deliveredAt + 1 +
          rate.envelope.catchupCost
            (validatorBlockSyncAcceptanceBound timed syncRules) round +
          timed.localActionBound + 1 ≤ next.snapshot.snapshotAt := by
  have deliveryFacts := timer_paced_peer_broadcast_is_delivered previous
    broadcast receiverInRange receiverCorrectAvailable sentAfterGst
  have deliveredBound := deliveryFacts.2.1
  have sentBound :=
    timer_paced_peer_broadcast_sent_within_round_pipeline previous broadcast
  rw [previousHead] at sentBound
  rw [next.snapshotAtDeadline, nextHead]
  let catchup := rate.envelope.catchupCost
    (validatorBlockSyncAcceptanceBound timed syncRules) round
  let tail := 1 + catchup + timed.localActionBound + 1
  have deliveryStep :
      broadcast.packet.deliveredAt + tail ≤
        (broadcast.packet.sentAt + network.delta) + tail :=
    Nat.add_le_add_right deliveredBound tail
  have sendStep :
      (broadcast.packet.sentAt + network.delta) + tail ≤
        (previous.timerStartedAt + waits.wait previousPrior round +
          3 * (timed.localActionBound + 1) + network.delta) + tail := by
    exact Nat.add_le_add_right
      (Nat.add_le_add_right sentBound network.delta) tail
  have startStep :
      (previous.timerStartedAt + waits.wait previousPrior round +
          3 * (timed.localActionBound + 1) + network.delta) + tail ≤
        (next.timerStartedAt + startDifference +
          waits.wait previousPrior round +
          3 * (timed.localActionBound + 1) + network.delta) + tail := by
    have shifted := Nat.add_le_add_right previousStartBound
      (waits.wait previousPrior round +
        3 * (timed.localActionBound + 1) + network.delta + tail)
    simpa only [Nat.add_assoc] using shifted
  calc
    broadcast.packet.deliveredAt + 1 + catchup +
          timed.localActionBound + 1 =
        broadcast.packet.deliveredAt + tail := by
      simp only [tail, Nat.add_assoc]
    _ ≤ (broadcast.packet.sentAt + network.delta) + tail := deliveryStep
    _ ≤ (previous.timerStartedAt + waits.wait previousPrior round +
          3 * (timed.localActionBound + 1) + network.delta) + tail := sendStep
    _ ≤ (next.timerStartedAt + startDifference +
          waits.wait previousPrior round +
          3 * (timed.localActionBound + 1) + network.delta) + tail := startStep
    _ = next.timerStartedAt +
          (waits.wait previousPrior round +
            (startDifference + 3 * (timed.localActionBound + 1) +
              network.delta + tail)) := by
      ac_rfl
    _ ≤ next.timerStartedAt + waits.wait receiverPrior (round + 1) := by
      apply Nat.add_le_add_left
      simpa only [tail, catchup, Nat.add_assoc] using visibilityMargin

/-- A concrete causal catch-up bound replaces the old requirement that every
parent was already accepted when the adjacent round started.

If the receiver installs a commit first, the caller restarts from that new
head. Otherwise, the exact previous block is accepted, retained, recorded, and
included in the next-round proposal. -/
theorem adjacent_timer_paced_productions_give_parent_evidence_from_catchup_bound_or_commit_advance
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (rate : ValidatorCausalCapsuleCatchupRateRules syncRules)
    {commonStart author receiver round : Nat}
    {receiverPrior : ValidatorCommitHead CommitId}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (commonStartBeforePrevious : commonStart ≤ previous.timerStartedAt)
    (commonStartBeforeNext : commonStart ≤ next.timerStartedAt)
    (receiverHeadAtStart :
      ((timed.execution.trace commonStart).validatorState receiver).commitHead =
        receiverPrior)
    (referenceAfterPrior : receiverPrior.round < round)
    (previousStartsAfterGst : network.gst ≤ previous.timerStartedAt)
    (active : ∀ time, commonStart ≤ time →
      (timed.execution.trace time).epochActive = true)
    (envelopeStarted : rate.envelope.startRound ≤ round)
    (resolutionBeforeNextSnapshot : ∀
      (broadcast : ValidatorTimerPacedPeerBroadcast timed previous.snapshot
        author receiver previous.proposalActionAt),
      broadcast.packet.deliveredAt + 1 +
            rate.envelope.catchupCost
              (validatorBlockSyncAcceptanceBound timed syncRules) round +
            timed.localActionBound + 1 ≤ next.snapshot.snapshotAt) :
    SomeCorrectAvailableCommitAdvance timed commonStart ∨
      ValidatorAdjacentTimerPacedParentEvidence previous next := by
  by_cases advanced : SomeCorrectAvailableCommitAdvance timed commonStart
  · exact Or.inl advanced
  · right
    have authorInRange : author < config.authorityCount := by
      simpa [previous.proposer] using previous.snapshot.proposerInRange
    have authorCorrect : faults.correctAvailable author = true := by
      simpa [previous.proposer] using
        previous.snapshot.proposerCorrectAvailable
    have receiverInRange : receiver < config.authorityCount := by
      simpa [next.proposer] using next.snapshot.proposerInRange
    have receiverCorrect : faults.correctAvailable receiver = true := by
      simpa [next.proposer] using next.snapshot.proposerCorrectAvailable
    have nextStartBeforeSnapshot : next.timerStartedAt ≤
        next.snapshot.snapshotAt := by
      rw [next.snapshotAtDeadline]
      exact Nat.le_add_right _ _
    have commonStartBeforeNextSnapshot : commonStart ≤
        next.snapshot.snapshotAt :=
      Nat.le_trans commonStartBeforeNext nextStartBeforeSnapshot
    have headAt : ∀ time, commonStart ≤ time →
        ((timed.execution.trace time).validatorState receiver).commitHead =
          receiverPrior := by
      intro time ordered
      exact no_advance_keeps_exact_head_for_catchup receiverInRange
        receiverCorrect ordered receiverHeadAtStart advanced
    have gcBelowPrevious : ∀ time, commonStart ≤ time →
        ((timed.execution.trace time).validatorState receiver).gcRound <
          previous.snapshot.block.reference.round := by
      intro time ordered
      have gcAtMostHead :
          ((timed.execution.trace time).validatorState receiver).gcRound ≤
            ((timed.execution.trace time).validatorState
              receiver).commitHead.round :=
        correct_validator_gc_round_at_most_commit_round
          (time := time) timed.execution receiverInRange receiverCorrect
      rw [headAt time ordered] at gcAtMostHead
      rw [previous.blockRound]
      omega
    by_cases sameAuthor : author = receiver
    · subst author
      have persistenceBeforeNextStart :=
        next_recovery_round_timer_starts_after_previous_persistence previous next
      have storedBeforeNextSnapshot : previous.snapshot.storedAt ≤
          next.snapshot.snapshotAt := by
        rw [previous.storedAfterPersistence]
        exact Nat.le_trans persistenceBeforeNextStart nextStartBeforeSnapshot
      have ownAtNext :=
        (timed.execution.durableStateMonotone receiver
          previous.snapshot.storedAt next.snapshot.snapshotAt receiverInRange
          storedBeforeNextSnapshot).own_block_persists
            (by simpa [previous.proposer] using previous.snapshot.blockStored)
      have ownFacts :=
        (timed.execution.statesWellFormed next.snapshot.snapshotAt receiver
          receiverInRange).ownBlockIsSound round
            previous.snapshot.block.reference (by
              simpa [previous.blockRound] using ownAtNext)
      exact accepted_retained_timer_paced_block_gives_parent_evidence
        representatives previous next ownFacts.2.2.1 ownFacts.2.2.2.1
    · let broadcast := Classical.choice
          (previous.peerBroadcast receiver receiverInRange (by
            intro receiverIsAuthor
            exact sameAuthor receiverIsAuthor.symm))
      have sentAfterGst : network.gst ≤ broadcast.packet.sentAt := by
        exact Nat.le_trans previousStartsAfterGst
          (Nat.le_trans (Nat.le_add_right _ _)
            (Nat.le_trans previous.deadlineBeforeProposal
              (Nat.le_trans (Nat.le_add_right _ 1)
                broadcast.proposalBeforeSend)))
      have deliveryFacts := timer_paced_peer_broadcast_is_delivered previous
        broadcast receiverInRange receiverCorrect sentAfterGst
      have commonStartBeforeDelivery : commonStart ≤
          broadcast.packet.deliveredAt := by
        exact Nat.le_trans commonStartBeforePrevious
          (Nat.le_trans (Nat.le_add_right _ _)
            (Nat.le_trans previous.deadlineBeforeProposal
              (Nat.le_trans (Nat.le_add_right _ 1)
                (Nat.le_trans broadcast.proposalBeforeSend deliveryFacts.1))))
      have activeFromPersistence : ∀ time, previous.persistTime + 1 ≤ time →
          (timed.execution.trace time).epochActive = true := by
        intro time persistenceBeforeTime
        apply active time
        have commonStartBeforePersistence : commonStart ≤
            previous.persistTime + 1 := by
          exact Nat.le_trans commonStartBeforePrevious
            (Nat.le_trans (Nat.le_add_right _ _)
              (Nat.le_trans previous.deadlineBeforeProposal
                (Nat.le_trans (Nat.le_add_right _ 1)
                  (Nat.le_trans previous.proposalBeforePersistence
                    (Nat.le_add_right _ 1)))))
        exact Nat.le_trans commonStartBeforePersistence persistenceBeforeTime
      have acceptedAtNext :=
        rate.timer_paced_peer_broadcast_is_accepted_before_snapshot pins
          frontiers sameGenesis acceptance previous broadcast receiverInRange
            receiverCorrect sentAfterGst activeFromPersistence envelopeStarted
              (resolutionBeforeNextSnapshot broadcast)
              (gcBelowPrevious next.snapshot.snapshotAt
                commonStartBeforeNextSnapshot)
      have packetAtDelivery := timed.execution.packetHistoryMonotone
        broadcast.packet.sentAt broadcast.packet.deliveredAt deliveryFacts.1
        broadcast.packetId broadcast.packet broadcast.packetInTrace
      have localBody : ValidatorLocalBlockBodyAt timed
          broadcast.packet.deliveredAt receiver previous.snapshot.block :=
        .delivered broadcast.packetId broadcast.packet packetAtDelivery
          broadcast.packetIsProtocol broadcast.packetReceiver
          broadcast.packetPayload deliveryFacts.2.2
      have bodyPin := sync.local_body_creates_durable_pin receiverInRange
        receiverCorrect
        (active (broadcast.packet.deliveredAt + 1)
          (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1)))
        (gcBelowPrevious (broadcast.packet.deliveredAt + 1)
          (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1)))
        localBody
      have deliveryBeforeNextSnapshot : broadcast.packet.deliveredAt + 1 ≤
          next.snapshot.snapshotAt :=
        Nat.le_trans (by
          exact Nat.le_add_right _
            (rate.envelope.catchupCost
                (validatorBlockSyncAcceptanceBound timed syncRules) round +
              timed.localActionBound + 1))
          (by simpa [Nat.add_assoc] using
            resolutionBeforeNextSnapshot broadcast)
      have pinAtNext := sync.body_pin_persists_while_head_is_current
        receiverInRange receiverCorrect deliveryBeforeNextSnapshot bodyPin.1
        (by
          intro time startBeforeTime _timeBeforeFinish
          exact active time (Nat.le_trans
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
            startBeforeTime))
        (by
          intro time startBeforeTime _timeBeforeFinish
          exact gcBelowPrevious time (Nat.le_trans
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
            startBeforeTime))
        (by
          intro time startBeforeTime _timeBeforeFinish
          have laterHead := headAt time (Nat.le_trans
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
            startBeforeTime)
          have initialHead := headAt (broadcast.packet.deliveredAt + 1)
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
          rw [laterHead, initialHead]
          exact Nat.le_refl _)
      have retainedAtNext := sync.acceptedPinnedBodyIsRetained
        next.snapshot.snapshotAt receiver previous.snapshot.block.reference
        _ receiverInRange receiverCorrect pinAtNext
        (gcBelowPrevious next.snapshot.snapshotAt commonStartBeforeNextSnapshot)
        acceptedAtNext
      exact accepted_retained_timer_paced_block_gives_parent_evidence
        representatives previous next acceptedAtNext retainedAtNext

/-- The adjacent wait-margin form of causal catch-up. This theorem derives the
packet-specific deadline and then applies the exact catch-up composition. -/
theorem adjacent_timer_paced_productions_give_parent_evidence_from_catchup_margin_or_commit_advance
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (rate : ValidatorCausalCapsuleCatchupRateRules syncRules)
    {commonStart author receiver round startDifference : Nat}
    {previousPrior receiverPrior : ValidatorCommitHead CommitId}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (commonStartBeforePrevious : commonStart ≤ previous.timerStartedAt)
    (commonStartBeforeNext : commonStart ≤ next.timerStartedAt)
    (receiverHeadAtStart :
      ((timed.execution.trace commonStart).validatorState receiver).commitHead =
        receiverPrior)
    (previousHead : previous.commitHead = previousPrior)
    (nextHead : next.commitHead = receiverPrior)
    (referenceAfterPrior : receiverPrior.round < round)
    (previousStartBound :
      previous.timerStartedAt ≤ next.timerStartedAt + startDifference)
    (visibilityMargin :
      waits.wait previousPrior round +
          (startDifference + 3 * (timed.localActionBound + 1) + network.delta +
            1 + rate.envelope.catchupCost
              (validatorBlockSyncAcceptanceBound timed syncRules) round +
            timed.localActionBound + 1) ≤
        waits.wait receiverPrior (round + 1))
    (previousStartsAfterGst : network.gst ≤ previous.timerStartedAt)
    (active : ∀ time, commonStart ≤ time →
      (timed.execution.trace time).epochActive = true)
    (envelopeStarted : rate.envelope.startRound ≤ round) :
    SomeCorrectAvailableCommitAdvance timed commonStart ∨
      ValidatorAdjacentTimerPacedParentEvidence previous next := by
  apply rate.adjacent_timer_paced_productions_give_parent_evidence_from_catchup_bound_or_commit_advance
    pins frontiers sameGenesis acceptance sync representatives previous next
      commonStartBeforePrevious commonStartBeforeNext receiverHeadAtStart
        referenceAfterPrior previousStartsAfterGst active envelopeStarted
  intro broadcast
  have receiverInRange : receiver < config.authorityCount := by
    simpa [next.proposer] using next.snapshot.proposerInRange
  have receiverCorrectAvailable : faults.correctAvailable receiver = true := by
    simpa [next.proposer] using next.snapshot.proposerCorrectAvailable
  have sentAfterGst : network.gst ≤ broadcast.packet.sentAt := by
    exact Nat.le_trans previousStartsAfterGst
      (Nat.le_trans (Nat.le_add_right _ _)
        (Nat.le_trans previous.deadlineBeforeProposal
          (Nat.le_trans (Nat.le_add_right _ 1)
            broadcast.proposalBeforeSend)))
  exact rate.adjacent_wait_margin_covers_timer_paced_catchup previous next
    broadcast receiverInRange receiverCorrectAvailable sentAfterGst previousHead
      nextHead previousStartBound visibilityMargin

/-- Causal catch-up and exact-head schedule matching identify one exact first
leader as both a next-round proposal parent and the first slot of an actual
prepared FlexCommitter scan.

An intervening correct commit install ends the old-head comparison. The caller
must start a new comparison from the installed head. -/
theorem causal_catchup_and_same_head_flex_scan_share_first_leader_or_commit_advance
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq BlockId]
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (pendingSource : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (rate : ValidatorCausalCapsuleCatchupRateRules syncRules)
    {commonStart author receiver round startDifference : Nat}
    {previousPrior receiverPrior : ValidatorCommitHead CommitId}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (commonStartBeforePrevious : commonStart ≤ previous.timerStartedAt)
    (commonStartBeforeNext : commonStart ≤ next.timerStartedAt)
    (receiverHeadAtStart :
      ((timed.execution.trace commonStart).validatorState receiver).commitHead =
        receiverPrior)
    (previousHead : previous.commitHead = previousPrior)
    (nextHead : next.commitHead = receiverPrior)
    (referenceAfterPrior : receiverPrior.round < round)
    (previousStartBound :
      previous.timerStartedAt ≤ next.timerStartedAt + startDifference)
    (visibilityMargin :
      waits.wait previousPrior round +
          (startDifference + 3 * (timed.localActionBound + 1) + network.delta +
            1 + rate.envelope.catchupCost
              (validatorBlockSyncAcceptanceBound timed syncRules) round +
            timed.localActionBound + 1) ≤
        waits.wait receiverPrior (round + 1))
    (previousStartsAfterGst : network.gst ≤ previous.timerStartedAt)
    (active : ∀ time, commonStart ≤ time →
      (timed.execution.trace time).epochActive = true)
    (envelopeStarted : rate.envelope.startRound ≤ round)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId)
    (occurs : observation.OccursIn timed)
    (scanHead : observation.input.commitHead = next.commitHead)
    (index : Nat)
    (indexInRange :
      index < (validatorFlexPreparedInputAt source schedule
        pendingSource.cacheAt pendingSource.highestAcceptedRound
          observation).pending.roundCount)
    (roundAtIndex :
      ((validatorFlexPreparedInputAt source schedule pendingSource.cacheAt
        pendingSource.highestAcceptedRound observation).pending.rounds
          index).round = round)
    (first :
      (config.selectedLeaderOrder next.commitHead.id round).head? =
        some previous.snapshot.block.reference.author) :
    SomeCorrectAvailableCommitAdvance timed commonStart ∨
      (previous.snapshot.block.reference ∈ next.snapshot.block.parents ∧
        (((validatorFlexPreparedInputAt source schedule pendingSource.cacheAt
            pendingSource.highestAcceptedRound observation).pending.rounds
              index).selectedSlots.map ReferenceSelectedSlotView.slot).head? =
          some (ExactSelectedLeaderSlot.mk round
            previous.snapshot.block.reference.author)) := by
  rcases rate.adjacent_timer_paced_productions_give_parent_evidence_from_catchup_margin_or_commit_advance
      pins frontiers sameGenesis acceptance sync representatives previous next
        commonStartBeforePrevious commonStartBeforeNext receiverHeadAtStart
          previousHead nextHead referenceAfterPrior previousStartBound
            visibilityMargin previousStartsAfterGst active envelopeStarted with
    advanced | evidence
  · exact Or.inl advanced
  · exact Or.inr
      (same_head_timer_proposal_and_prepared_flex_scan_share_first_leader
        pendingSource next observation occurs scanHead index indexInRange
          roundAtIndex first evidence.representative evidence.retained)

end ValidatorCausalCapsuleCatchupRateRules

end Mysticeti
