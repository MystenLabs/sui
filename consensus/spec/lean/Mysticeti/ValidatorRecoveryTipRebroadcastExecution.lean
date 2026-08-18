/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorRecoveryMode
import Mysticeti.ValidatorRecoverySourcePinExecution

namespace Mysticeti

/-! Durable one-host rebroadcast work for the current recovery tip.

A validator in commit progress recovery sends its current durable tip to every
other validator. The local work can retry. Network delivery is not a field of
this execution.
-/

/-- Durable send goals for locally owned recovery tips. -/
structure ValidatorRecoveryTipRebroadcastState (BlockId : Type) where
  pending : ValidatorBlockRef BlockId → Nat → Bool

/-- One batch of local tip-rebroadcast inputs. -/
structure ValidatorRecoveryTipRebroadcastEvent (BlockId : Type) where
  latchTip : Option (ValidatorBlockRef BlockId)
  sent : ValidatorBlockRef BlockId → Nat → Bool

/-- One exact update of local rebroadcast work. A new latch can add all peer
goals. An actual send clears only its matching goal in the same batch. -/
structure ValidatorRecoveryTipRebroadcastTransition
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (validator : Nat)
    (before : ValidatorRecoveryTipRebroadcastState BlockId)
    (event : ValidatorRecoveryTipRebroadcastEvent BlockId)
    (after : ValidatorRecoveryTipRebroadcastState BlockId) : Prop where
  pendingExact : ∀ reference receiver,
    after.pending reference receiver = true ↔
      (before.pending reference receiver = true ∨
        (event.latchTip = some reference ∧
          receiver < config.authorityCount ∧ receiver ≠ validator)) ∧
      event.sent reference receiver = false

/-- One source-local recovery-tip rebroadcast execution. -/
structure ValidatorRecoveryTipRebroadcastExecution
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
    (recoveryWait : Time) where
  trace : Time → Nat → ValidatorRecoveryTipRebroadcastState BlockId
  event : Time → Nat → ValidatorRecoveryTipRebroadcastEvent BlockId
  initialPendingEmpty : ∀ validator reference receiver,
    (trace 0 validator).pending reference receiver = false
  transitionsFollowRules : ∀ time validator,
    ValidatorRecoveryTipRebroadcastTransition config validator
      (trace time validator) (event time validator)
      (trace (time + 1) validator)
  /-- Every latch names the exact positive durable tip and its active local
  recovery pin. -/
  latchHasCurrentPinnedTip : ∀ time validator reference,
    (event time validator).latchTip = some reference →
    ValidatorCommitProgressRecoveryModeAt timed recoveryWait time validator ∧
      0 < ((timed.execution.trace time).validatorState
        validator).highestSignedRound ∧
      ((timed.execution.trace time).validatorState validator).ownBlockAt
          ((timed.execution.trace time).validatorState
            validator).highestSignedRound = some reference ∧
      ∃ capsuleId entry block,
        (pins.trace time validator).capsuleAt capsuleId = some entry ∧
          (pins.trace time validator).pinned capsuleId = true ∧
          entry.capsule.targetBlock = block ∧
          block.reference = reference
  /-- Recovery mode keeps the current tip scheduled for each other peer. A
  cleared goal is latched again, which permits a post-GST retry. -/
  recoveryModeSchedulesCurrentTip : ∀ time validator receiver reference,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorCommitProgressRecoveryModeAt timed recoveryWait time validator →
    0 < ((timed.execution.trace time).validatorState
      validator).highestSignedRound →
    ((timed.execution.trace time).validatorState validator).ownBlockAt
        ((timed.execution.trace time).validatorState
          validator).highestSignedRound = some reference →
    receiver < config.authorityCount →
    receiver ≠ validator →
    (trace time validator).pending reference receiver = true ∨
      (event time validator).latchTip = some reference ∨
      (event time validator).sent reference receiver = true
  /-- A sent marker is exactly the same host's concrete block-send action. -/
  sentIffMainAction : ∀ time validator receiver reference,
    (event time validator).sent reference receiver = true ↔
      ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.sendBlock receiver reference)
  /-- Each durable send goal is protected local work. The protected action guard
  keeps the old tip retained and sendable if a commit or GC update races it. -/
  pendingSendIsProtected : ∀ time validator receiver reference,
    (trace time validator).pending reference receiver = true →
    timed.protectedAction time validator (.sendBlock receiver reference)

namespace ValidatorRecoveryTipRebroadcastExecution

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
variable {pins : ValidatorRecoverySourcePinExecution syncRules}
variable {recoveryWait : Time}

/-- One execution batch keeps an existing block-catalog entry. -/
private theorem recovery_tip_world_step_preserves_catalog
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    {blockId : BlockId} {block : ValidatorBlock BlockId}
    (present : before.blockCatalog blockId = some block) :
    after.blockCatalog blockId = some block := by
  induction step with
  | nil => exact present
  | cons firstStep remainingSteps inductionHypothesis =>
      have presentAfterFirst :=
        (validator_atomic_step_history_monotone firstStep).1 blockId block
          present
      exact inductionHypothesis presentAfterFirst

/-- A send action for one cataloged block sends that exact block body. -/
private theorem recovery_tip_exact_send_creates_packet
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {time validator receiver : Time}
    {block : ValidatorBlock BlockId}
    (catalog : (timed.execution.trace time).blockCatalog block.reference.id =
      some block)
    (occurs : ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.sendBlock receiver block.reference)) :
    ∃ (packetId : PacketId)
        (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      (timed.execution.trace (time + 1)).packets packetId = some packet ∧
        protocolPacket packet ∧
        packet.sender = validator ∧
        packet.receiver = receiver ∧
        packet.payload = .block block ∧
        packet.sentAt = time + 1 := by
  rcases occurs with ⟨headEvents, tailEvents, eventsEqual⟩
  have completeStep := timed.execution.stepsFollowRules time
  rw [eventsEqual] at completeStep
  rcases validator_world_step_append_split completeStep with
    ⟨actionBefore, headStep, actionAndSuffix⟩
  cases actionAndSuffix with
  | cons actionStep suffixStep =>
      have catalogAtAction := recovery_tip_world_step_preserves_catalog
        headStep catalog
      rcases effects.sendBlockCreatesPacket time actionBefore _ validator
          receiver block.reference actionStep with
        ⟨packetId, sentBlock, packet, sentCatalog, sentReference,
          _packetAbsent, packetAfterAction, packetProtocol, packetSender,
          packetReceiver, packetPayload, packetSentAt⟩
      have sameBlock : sentBlock = block := by
        have sameCatalog : some block = some sentBlock := by
          rw [← catalogAtAction, sentCatalog]
        exact Option.some.inj sameCatalog.symm
      subst sentBlock
      have packetAfterBatch := validator_world_step_packet_persists suffixStep
        packetAfterAction
      exact ⟨packetId, packet, packetAfterBatch, packetProtocol, packetSender,
        packetReceiver, packetPayload, packetSentAt⟩

/-- A latch creates the matching peer goal unless that send already ran in the
same batch. -/
theorem latch_creates_peer_goal
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    {time validator receiver : Time}
    {reference : ValidatorBlockRef BlockId}
    (latched : (broadcast.event time validator).latchTip = some reference)
    (receiverInRange : receiver < config.authorityCount)
    (differentValidator : receiver ≠ validator)
    (notSent : (broadcast.event time validator).sent reference receiver =
      false) :
    (broadcast.trace (time + 1) validator).pending reference receiver = true :=
  (broadcast.transitionsFollowRules time validator).pendingExact reference
    receiver |>.2 ⟨Or.inr ⟨latched, receiverInRange, differentValidator⟩,
      notSent⟩

/-- A protected rebroadcast goal creates one exact addressed block packet in a
bounded local time. -/
theorem pending_tip_send_creates_addressed_packet
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {start validator receiver : Time}
    {reference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (pending : (broadcast.trace start validator).pending reference receiver =
      true) :
    ∃ completion : ValidatorActionCompletion timed.execution
        timed.localActionBound validator (.sendBlock receiver reference) start,
      ∃ (packetId : PacketId) (block : ValidatorBlock BlockId)
          (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
        start ≤ completion.event.completedAt ∧
          completion.event.completedAt ≤ start + timed.localActionBound ∧
          (timed.execution.trace
            (completion.event.completedAt + 1)).packets packetId = some packet ∧
          block.reference = reference ∧
          protocolPacket packet ∧
          packet.sender = validator ∧
          packet.receiver = receiver ∧
          packet.payload = .block block ∧
          packet.sentAt = completion.event.completedAt + 1 := by
  rcases protected_validator_action_completes_within_bound timed
      validatorInRange validatorCorrectAvailable
      (broadcast.pendingSendIsProtected start validator receiver reference
        pending) with
    ⟨completion, afterStart, withinBound, occurs⟩
  rcases send_block_occurrence_creates_addressed_packet effects occurs with
    ⟨packetId, block, packet, stored, blockReference, protocol, sender,
      addressedReceiver, payload, sentAt⟩
  exact ⟨completion, packetId, block, packet, afterStart, withinBound, stored,
    blockReference, protocol, sender, addressedReceiver, payload, sentAt⟩

/-- A protected rebroadcast goal for one cataloged block sends that exact block
body in a bounded local time. -/
theorem pending_tip_send_creates_exact_addressed_packet
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {start validator receiver : Time}
    {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (catalog : (timed.execution.trace start).blockCatalog block.reference.id =
      some block)
    (pending : (broadcast.trace start validator).pending block.reference receiver =
      true) :
    ∃ completion : ValidatorActionCompletion timed.execution
        timed.localActionBound validator (.sendBlock receiver block.reference)
          start,
      ∃ (packetId : PacketId)
          (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
        start ≤ completion.event.completedAt ∧
          completion.event.completedAt ≤ start + timed.localActionBound ∧
          (timed.execution.trace
            (completion.event.completedAt + 1)).packets packetId = some packet ∧
          protocolPacket packet ∧
          packet.sender = validator ∧
          packet.receiver = receiver ∧
          packet.payload = .block block ∧
          packet.sentAt = completion.event.completedAt + 1 := by
  rcases protected_validator_action_completes_within_bound timed
      validatorInRange validatorCorrectAvailable
      (broadcast.pendingSendIsProtected start validator receiver block.reference
        pending) with
    ⟨completion, afterStart, withinBound, occurs⟩
  have catalogAtSend := timed.execution.blockCatalogMonotone start
    completion.event.completedAt afterStart block.reference.id block catalog
  rcases recovery_tip_exact_send_creates_packet effects catalogAtSend occurs with
    ⟨packetId, packet, stored, protocol, sender, addressedReceiver, payload,
      sentAt⟩
  exact ⟨completion, packetId, packet, afterStart, withinBound, stored, protocol,
    sender, addressedReceiver, payload, sentAt⟩

/-- In recovery mode, one current positive tip is sent to one peer now or after
one durable latch. This theorem does not assume packet delivery. -/
theorem recovery_mode_sends_current_tip_packet
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {time validator receiver : Time}
    {reference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (receiverInRange : receiver < config.authorityCount)
    (differentValidator : receiver ≠ validator)
    (recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
      time validator)
    (positiveTip : 0 < ((timed.execution.trace time).validatorState
      validator).highestSignedRound)
    (currentTip :
      ((timed.execution.trace time).validatorState validator).ownBlockAt
          ((timed.execution.trace time).validatorState
            validator).highestSignedRound = some reference) :
    ∃ (sentAt : Time) (packetId : PacketId) (block : ValidatorBlock BlockId)
        (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      time ≤ sentAt ∧
        sentAt ≤ time + 1 + timed.localActionBound ∧
        (timed.execution.trace (sentAt + 1)).packets packetId = some packet ∧
        block.reference = reference ∧
        protocolPacket packet ∧
        packet.sender = validator ∧
        packet.receiver = receiver ∧
        packet.payload = .block block ∧
        packet.sentAt = sentAt + 1 := by
  have scheduled := broadcast.recoveryModeSchedulesCurrentTip time validator
    receiver reference validatorInRange validatorCorrectAvailable recoveryMode
    positiveTip currentTip receiverInRange differentValidator
  rcases scheduled with pending | latched | sentNow
  · rcases broadcast.pending_tip_send_creates_addressed_packet effects
        validatorInRange validatorCorrectAvailable pending with
      ⟨completion, packetId, block, packet, afterStart, withinBound, stored,
        blockReference, protocol, sender, addressedReceiver, payload, sentAt⟩
    refine ⟨completion.event.completedAt, packetId, block, packet, afterStart,
      ?_, stored, blockReference, protocol, sender, addressedReceiver, payload,
      sentAt⟩
    exact Nat.le_trans withinBound (by
      simp [Nat.add_comm, Nat.add_left_comm])
  · cases sentState : (broadcast.event time validator).sent reference receiver
      with
    | true =>
        have occurs := (broadcast.sentIffMainAction time validator receiver
          reference).1 sentState
        rcases send_block_occurrence_creates_addressed_packet effects occurs with
          ⟨packetId, block, packet, stored, blockReference, protocol, sender,
            addressedReceiver, payload, sentAt⟩
        exact ⟨time, packetId, block, packet, Nat.le_refl _, by
          simp [Nat.add_assoc], stored,
          blockReference, protocol, sender, addressedReceiver, payload, sentAt⟩
    | false =>
        have pendingAfter := broadcast.latch_creates_peer_goal latched
          receiverInRange differentValidator sentState
        rcases broadcast.pending_tip_send_creates_addressed_packet effects
            validatorInRange validatorCorrectAvailable pendingAfter with
          ⟨completion, packetId, block, packet, afterLatch, withinBound, stored,
            blockReference, protocol, sender, addressedReceiver, payload,
            sentAt⟩
        refine ⟨completion.event.completedAt, packetId, block, packet,
          Nat.le_trans (Nat.le_add_right _ 1) afterLatch, ?_, stored,
          blockReference, protocol, sender, addressedReceiver, payload, sentAt⟩
        simpa using withinBound
  · have occurs := (broadcast.sentIffMainAction time validator receiver
      reference).1 sentNow
    rcases send_block_occurrence_creates_addressed_packet effects occurs with
      ⟨packetId, block, packet, stored, blockReference, protocol, sender,
        addressedReceiver, payload, sentAt⟩
    exact ⟨time, packetId, block, packet, Nat.le_refl _, by
      simp [Nat.add_assoc], stored,
      blockReference, protocol, sender, addressedReceiver, payload, sentAt⟩

/-- In recovery mode, one exact cataloged current-tip body is sent to one peer
now or after one durable latch. -/
theorem recovery_mode_sends_exact_current_tip_packet
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {time validator receiver : Time}
    {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (receiverInRange : receiver < config.authorityCount)
    (differentValidator : receiver ≠ validator)
    (recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
      time validator)
    (positiveTip : 0 < ((timed.execution.trace time).validatorState
      validator).highestSignedRound)
    (currentTip :
      ((timed.execution.trace time).validatorState validator).ownBlockAt
          ((timed.execution.trace time).validatorState
            validator).highestSignedRound = some block.reference)
    (catalog : (timed.execution.trace time).blockCatalog block.reference.id =
      some block) :
    ∃ (sentAt : Time) (packetId : PacketId)
        (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      time ≤ sentAt ∧
        sentAt ≤ time + 1 + timed.localActionBound ∧
        (timed.execution.trace (sentAt + 1)).packets packetId = some packet ∧
        protocolPacket packet ∧
        packet.sender = validator ∧
        packet.receiver = receiver ∧
        packet.payload = .block block ∧
        packet.sentAt = sentAt + 1 := by
  have scheduled := broadcast.recoveryModeSchedulesCurrentTip time validator
    receiver block.reference validatorInRange validatorCorrectAvailable
    recoveryMode positiveTip currentTip receiverInRange differentValidator
  rcases scheduled with pending | latched | sentNow
  · rcases broadcast.pending_tip_send_creates_exact_addressed_packet effects
        validatorInRange validatorCorrectAvailable catalog pending with
      ⟨completion, packetId, packet, afterStart, withinBound, stored, protocol,
        sender, addressedReceiver, payload, sentAt⟩
    refine ⟨completion.event.completedAt, packetId, packet, afterStart, ?_, stored,
      protocol, sender, addressedReceiver, payload, sentAt⟩
    exact Nat.le_trans withinBound (by
      simp [Nat.add_comm, Nat.add_left_comm])
  · cases sentState :
        (broadcast.event time validator).sent block.reference receiver with
    | true =>
        have occurs := (broadcast.sentIffMainAction time validator receiver
          block.reference).1 sentState
        rcases recovery_tip_exact_send_creates_packet effects catalog occurs with
          ⟨packetId, packet, stored, protocol, sender, addressedReceiver,
            payload, sentAt⟩
        exact ⟨time, packetId, packet, Nat.le_refl _, by simp [Nat.add_assoc],
          stored, protocol, sender, addressedReceiver, payload, sentAt⟩
    | false =>
        have pendingAfter := broadcast.latch_creates_peer_goal latched
          receiverInRange differentValidator sentState
        have catalogAfter := timed.execution.blockCatalogMonotone time (time + 1)
          (Nat.le_add_right _ 1) block.reference.id block catalog
        rcases broadcast.pending_tip_send_creates_exact_addressed_packet effects
            validatorInRange validatorCorrectAvailable catalogAfter pendingAfter
            with
          ⟨completion, packetId, packet, afterLatch, withinBound, stored,
            protocol, sender, addressedReceiver, payload, sentAt⟩
        refine ⟨completion.event.completedAt, packetId, packet,
          Nat.le_trans (Nat.le_add_right _ 1) afterLatch, ?_, stored, protocol,
          sender, addressedReceiver, payload, sentAt⟩
        simpa using withinBound
  · have occurs := (broadcast.sentIffMainAction time validator receiver
      block.reference).1 sentNow
    rcases recovery_tip_exact_send_creates_packet effects catalog occurs with
      ⟨packetId, packet, stored, protocol, sender, addressedReceiver, payload,
        sentAt⟩
    exact ⟨time, packetId, packet, Nat.le_refl _, by simp [Nat.add_assoc], stored,
      protocol, sender, addressedReceiver, payload, sentAt⟩

/-- After GST, the network delivers the exact current-tip packet to one correct
peer. Delivery follows from partial synchrony, not from rebroadcast state. -/
theorem recovery_mode_delivers_current_tip_packet
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {time validator receiver : Time}
    {reference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (differentValidator : receiver ≠ validator)
    (afterGst : network.gst ≤ time)
    (recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
      time validator)
    (positiveTip : 0 < ((timed.execution.trace time).validatorState
      validator).highestSignedRound)
    (currentTip :
      ((timed.execution.trace time).validatorState validator).ownBlockAt
          ((timed.execution.trace time).validatorState
            validator).highestSignedRound = some reference) :
    ∃ (packetId : PacketId) (block : ValidatorBlock BlockId)
        (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      time ≤ packet.deliveredAt ∧
        (timed.execution.trace packet.deliveredAt).packets packetId =
          some packet ∧
        block.reference = reference ∧
        protocolPacket packet ∧
        packet.sender = validator ∧
        packet.receiver = receiver ∧
        packet.payload = .block block ∧
        ValidatorPacketDeliveryOccurs
          (timed.execution.events packet.deliveredAt) packetId := by
  rcases broadcast.recovery_mode_sends_current_tip_packet effects
      validatorInRange validatorCorrectAvailable receiverInRange
      differentValidator recoveryMode positiveTip currentTip with
    ⟨sentAt, packetId, block, packet, timeBeforeSend, _sendBound, stored,
      blockReference, protocol, sender, addressedReceiver, payload,
      packetSentAt⟩
  have present :
      (timed.execution.trace packet.sentAt).packets packetId = some packet := by
    rw [packetSentAt]
    exact stored
  have senderRange : packet.sender < config.authorityCount := by
    simpa only [sender] using validatorInRange
  have addressedRange : packet.receiver < config.authorityCount := by
    simpa only [addressedReceiver] using receiverInRange
  have senderCorrect : faults.correctAvailable packet.sender = true := by
    simpa only [sender] using validatorCorrectAvailable
  have addressedCorrect : faults.correctAvailable packet.receiver = true := by
    simpa only [addressedReceiver] using receiverCorrectAvailable
  have packetAfterGst : network.gst ≤ packet.sentAt := by
    rw [packetSentAt]
    exact Nat.le_trans afterGst
      (Nat.le_trans timeBeforeSend (Nat.le_add_right _ 1))
  have delivered := validator_protocol_packet_is_delivered timed.execution
    present protocol senderRange addressedRange senderCorrect addressedCorrect
      packetAfterGst
  have timing := network.postGstDelivery packet protocol senderRange
    addressedRange senderCorrect addressedCorrect packetAfterGst
  have presentAtDelivery := timed.execution.packetHistoryMonotone packet.sentAt
    packet.deliveredAt timing.1 packetId packet present
  have timeBeforeDelivery : time ≤ packet.deliveredAt := by
    exact Nat.le_trans timeBeforeSend (Nat.le_trans (Nat.le_add_right _ 1) (by
      simpa only [packetSentAt] using timing.1))
  exact ⟨packetId, block, packet, timeBeforeDelivery, presentAtDelivery,
    blockReference, protocol, sender, addressedReceiver, payload, delivered⟩

/-- After GST, the network delivers the exact cataloged current-tip body to one
correct peer. -/
theorem recovery_mode_delivers_exact_current_tip_packet
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {time validator receiver : Time}
    {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (differentValidator : receiver ≠ validator)
    (afterGst : network.gst ≤ time)
    (recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
      time validator)
    (positiveTip : 0 < ((timed.execution.trace time).validatorState
      validator).highestSignedRound)
    (currentTip :
      ((timed.execution.trace time).validatorState validator).ownBlockAt
          ((timed.execution.trace time).validatorState
            validator).highestSignedRound = some block.reference)
    (catalog : (timed.execution.trace time).blockCatalog block.reference.id =
      some block) :
    ∃ (packetId : PacketId)
        (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      time ≤ packet.deliveredAt ∧
        (timed.execution.trace packet.deliveredAt).packets packetId = some packet ∧
        protocolPacket packet ∧
        packet.sender = validator ∧
        packet.receiver = receiver ∧
        packet.payload = .block block ∧
        ValidatorPacketDeliveryOccurs
          (timed.execution.events packet.deliveredAt) packetId := by
  rcases broadcast.recovery_mode_sends_exact_current_tip_packet effects
      validatorInRange validatorCorrectAvailable receiverInRange
      differentValidator recoveryMode positiveTip currentTip catalog with
    ⟨sentAt, packetId, packet, timeBeforeSend, _sendBound, stored, protocol,
      sender, addressedReceiver, payload, packetSentAt⟩
  have present :
      (timed.execution.trace packet.sentAt).packets packetId = some packet := by
    rw [packetSentAt]
    exact stored
  have senderRange : packet.sender < config.authorityCount := by
    simpa only [sender] using validatorInRange
  have addressedRange : packet.receiver < config.authorityCount := by
    simpa only [addressedReceiver] using receiverInRange
  have senderCorrect : faults.correctAvailable packet.sender = true := by
    simpa only [sender] using validatorCorrectAvailable
  have addressedCorrect : faults.correctAvailable packet.receiver = true := by
    simpa only [addressedReceiver] using receiverCorrectAvailable
  have packetAfterGst : network.gst ≤ packet.sentAt := by
    rw [packetSentAt]
    exact Nat.le_trans afterGst
      (Nat.le_trans timeBeforeSend (Nat.le_add_right _ 1))
  have delivered := validator_protocol_packet_is_delivered timed.execution
    present protocol senderRange addressedRange senderCorrect addressedCorrect
      packetAfterGst
  have timing := network.postGstDelivery packet protocol senderRange
    addressedRange senderCorrect addressedCorrect packetAfterGst
  have presentAtDelivery := timed.execution.packetHistoryMonotone packet.sentAt
    packet.deliveredAt timing.1 packetId packet present
  have timeBeforeDelivery : time ≤ packet.deliveredAt := by
    exact Nat.le_trans timeBeforeSend (Nat.le_trans (Nat.le_add_right _ 1) (by
      simpa only [packetSentAt] using timing.1))
  exact ⟨packetId, packet, timeBeforeDelivery, presentAtDelivery, protocol,
    sender, addressedReceiver, payload, delivered⟩

/-- An active pin identifies the exact tip body in the delivered recovery
packet. -/
theorem recovery_mode_delivers_pinned_tip_body
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {time validator receiver : Time}
    {capsuleId : ValidatorRecoveryCapsuleKey BlockId}
    {entry : ValidatorPinnedRecoveryCapsule (BlockId := BlockId) config}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (differentValidator : receiver ≠ validator)
    (afterGst : network.gst ≤ time)
    (recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
      time validator)
    (positiveTip : 0 < ((timed.execution.trace time).validatorState
      validator).highestSignedRound)
    (currentTip :
      ((timed.execution.trace time).validatorState validator).ownBlockAt
          ((timed.execution.trace time).validatorState
              validator).highestSignedRound =
        some entry.capsule.targetBlock.reference)
    (stored : (pins.trace time validator).capsuleAt capsuleId = some entry)
    (pinned : (pins.trace time validator).pinned capsuleId = true) :
    ∃ (packetId : PacketId)
        (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      time ≤ packet.deliveredAt ∧
        (timed.execution.trace packet.deliveredAt).packets packetId = some packet ∧
        protocolPacket packet ∧
        packet.sender = validator ∧
        packet.receiver = receiver ∧
        packet.payload = .block entry.capsule.targetBlock ∧
        ValidatorPacketDeliveryOccurs
          (timed.execution.events packet.deliveredAt) packetId := by
  have targetMember := entry.capsule.target_and_parents_in_history.1
  have localTarget := pins.pinnedHistoryIsLocal time validator capsuleId entry
    stored pinned entry.capsule.targetBlock targetMember
  exact broadcast.recovery_mode_delivers_exact_current_tip_packet effects
    validatorInRange validatorCorrectAvailable receiverInRange
    receiverCorrectAvailable differentValidator afterGst recoveryMode positiveTip
    currentTip localTarget.2.2.1

end ValidatorRecoveryTipRebroadcastExecution

end Mysticeti
