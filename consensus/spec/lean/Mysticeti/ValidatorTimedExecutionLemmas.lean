/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorTimedExecution

namespace Mysticeti

/-- A protected concrete local action has one bounded completion timestamp. -/
theorem protected_validator_action_completes_within_bound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {validator : Nat} {latchedAt : Time}
    {action : ValidatorLocalAction BlockId CommitId}
    (validatorInRange : validator < config.authorityCount)
    (correctAvailable : faults.correctAvailable validator = true)
    (latched : timed.protectedAction latchedAt validator action) :
    ∃ completion : ValidatorActionCompletion timed.execution
        timed.localActionBound validator action latchedAt,
      latchedAt ≤ completion.event.completedAt ∧
        completion.event.completedAt ≤ latchedAt + timed.localActionBound ∧
        ValidatorLocalActionOccurs
          (timed.execution.events completion.event.completedAt) validator
          action := by
  let completion := timed.completeProtectedAction validator action latchedAt
    validatorInRange correctAvailable latched
  exact ⟨completion, by simpa [completion.sameEnableTime] using
    completion.enableBeforeCompletion,
    by simpa [completion.sameEnableTime] using completion.completesWithinBound,
    completion.occurs⟩

/-- Protected work remains enabled while it is latched and has not run. -/
theorem protected_validator_action_remains_enabled_until_run
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {start finish : Time} {validator : Nat}
    {action : ValidatorLocalAction BlockId CommitId}
    (latched : timed.protectedAction start validator action)
    (ordered : start ≤ finish)
    (notRun : ValidatorActionAbsentBefore timed.execution start finish validator
      action) :
    ValidatorActionEnabledAt timed.execution finish validator action := by
  apply timed.protectedActionIsEnabled
  exact timed.protectedActionPersistsUntilRun start finish validator action
    latched ordered notRun

/-- A transient action uses weak fairness only while it stays enabled. -/
theorem continuously_enabled_validator_action_eventually_runs
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {start : Time} {validator : Nat}
    {action : ValidatorLocalAction BlockId CommitId}
    (validatorInRange : validator < config.authorityCount)
    (correctAvailable : faults.correctAvailable validator = true)
    (continuouslyEnabled : ValidatorActionContinuouslyEnabled config program
      execution.trace start validator action) :
    ∃ finish,
      start ≤ finish ∧
        ValidatorLocalActionOccurs (execution.events finish) validator action :=
  execution.fairConcreteActions validator action start validatorInRange
    correctAvailable continuouslyEnabled

/-- Split one batch at an appended event list. -/
theorem validator_world_step_append_split
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {headEvents tailEvents :
      List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      (headEvents ++ tailEvents) after) :
    ∃ middle,
      ValidatorWorldStep config faults protocolPacket program time before
        headEvents middle ∧
      ValidatorWorldStep config faults protocolPacket program time middle
        tailEvents after := by
  induction headEvents generalizing before with
  | nil =>
      exact ⟨before, .nil, by simpa using step⟩
  | cons event remaining ih =>
      cases step with
      | cons firstStep remainingSteps =>
          rcases ih remainingSteps with ⟨middle, prefixStep, suffixStep⟩
          exact ⟨middle, .cons firstStep prefixStep, suffixStep⟩

/-- A local action occurrence exposes its atomic step and the remaining batch. -/
theorem validator_world_step_local_action_with_suffix
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {validator : Nat}
    {action : ValidatorLocalAction BlockId CommitId}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (occurs : ValidatorLocalActionOccurs events validator action) :
    ∃ actionBefore actionAfter suffix,
      ValidatorAtomicStep config faults protocolPacket program time actionBefore
        (.localAction validator action) actionAfter ∧
      ValidatorWorldStep config faults protocolPacket program time actionAfter
        suffix after := by
  rcases occurs with ⟨headEvents, tailEvents, rfl⟩
  rcases validator_world_step_append_split step with
    ⟨actionBefore, _, actionAndSuffix⟩
  cases actionAndSuffix with
  | cons actionStep suffixStep =>
      exact ⟨actionBefore, _, tailEvents, actionStep, suffixStep⟩

/-- A packet delivery occurrence exposes its atomic step and remaining batch. -/
theorem validator_world_step_delivery_with_suffix
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {packetId : PacketId}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (occurs : ValidatorPacketDeliveryOccurs events packetId) :
    ∃ deliveryBefore deliveryAfter headEvents tailEvents,
      ValidatorWorldStep config faults protocolPacket program time before
        headEvents deliveryBefore ∧
      ValidatorAtomicStep config faults protocolPacket program time
        deliveryBefore (.deliverPacket packetId) deliveryAfter ∧
      ValidatorWorldStep config faults protocolPacket program time deliveryAfter
        tailEvents after := by
  rcases occurs with ⟨headEvents, tailEvents, rfl⟩
  rcases validator_world_step_append_split step with
    ⟨deliveryBefore, headStep, deliveryAndSuffix⟩
  cases deliveryAndSuffix with
  | cons deliveryStep suffixStep =>
      exact ⟨deliveryBefore, _, headEvents, tailEvents, headStep, deliveryStep,
        suffixStep⟩

/-- One atomic step preserves all earlier block and packet history. -/
theorem validator_atomic_step_history_monotone
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {event : ValidatorAtomicEvent BlockId CommitId PacketId}
  (step : ValidatorAtomicStep config faults protocolPacket program time before
      event after) :
    ValidatorWorldHistoryMonotone before after := by
  cases step with
  | localAction _ _ _ _ _ _ _ historyMonotone => exact historyMonotone
  | deliverPacket _ _ _ _ _ _ _ _ _ _ catalogSame packetsSame =>
      constructor
      · intro key value present
        rw [catalogSame]
        exact present
      · intro key value present
        rw [packetsSame]
        exact present
  | clockTick _ clockUpdate =>
      subst after
      constructor <;> intro key value present <;>
        simpa [ValidatorWorldState.updateClocks] using present

/-- Packet history persists through the rest of one event batch. -/
theorem validator_world_step_packet_persists
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    (present : before.packets packetId = some packet) :
    after.packets packetId = some packet := by
  induction step with
  | nil => exact present
  | cons firstStep remainingSteps ih =>
      have firstPresent :=
        (validator_atomic_step_history_monotone firstStep).2 packetId packet
          present
      exact ih firstPresent

/-- One atomic step preserves durable fields for each in-range validator. -/
theorem validator_atomic_step_durable_monotone
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {event : ValidatorAtomicEvent BlockId CommitId PacketId}
  (step : ValidatorAtomicStep config faults protocolPacket program time before
      event after)
    (validator : Nat) :
    ValidatorDurableStateMonotone (before.validatorState validator)
      (after.validatorState validator) := by
  cases step
  case localAction =>
    rename_i actingValidator action _ _ _ _ structuralEffect otherUnchanged _ _
    by_cases same : validator = actingValidator
    · subst actingValidator
      exact structuralEffect.2.1
    · rw [otherUnchanged validator same]
      simp [ValidatorDurableStateMonotone, OptionMapMonotone,
        BinaryOptionMapMonotone, BoolMapMonotone]
  case deliverPacket =>
    rename_i _ packet _ _ _ _ _ _ _ structuralEffect otherUnchanged _ _ _
    by_cases same : validator = packet.receiver
    · subst validator
      exact structuralEffect.2.1
    · rw [otherUnchanged validator same]
      simp [ValidatorDurableStateMonotone, OptionMapMonotone,
        BinaryOptionMapMonotone, BoolMapMonotone]
  case clockTick =>
    rename_i _ _ clockUpdate
    subst after
    simp [ValidatorWorldState.updateClocks, ValidatorDurableStateMonotone,
      OptionMapMonotone, BinaryOptionMapMonotone, BoolMapMonotone]

/-- An accepted block persists through the rest of one event batch. -/
theorem validator_world_step_accepted_block_persists
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    {validator : Nat} {reference : ValidatorBlockRef BlockId}
    (accepted : (before.validatorState validator).accepted reference = true) :
    (after.validatorState validator).accepted reference = true := by
  induction step with
  | nil => exact accepted
  | cons firstStep remainingSteps ih =>
      have firstAccepted :=
        (validator_atomic_step_durable_monotone firstStep validator)
          |>.accepted_block_persists accepted
      exact ih firstAccepted

/-- An own block persists through the rest of one event batch. -/
theorem validator_world_step_own_block_persists
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    {validator round : Nat} {reference : ValidatorBlockRef BlockId}
    (stored : (before.validatorState validator).ownBlockAt round =
      some reference) :
    (after.validatorState validator).ownBlockAt round = some reference := by
  induction step with
  | nil => exact stored
  | cons firstStep remainingSteps ih =>
      have firstStored :=
        (validator_atomic_step_durable_monotone firstStep validator)
          |>.own_block_persists stored
      exact ih firstStored

/-- A sent-own-block record persists through the rest of one event batch. -/
theorem validator_world_step_sent_own_block_persists
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    {validator round : Nat}
    (sent : (before.validatorState validator).sentOwnBlockAt round = true) :
    (after.validatorState validator).sentOwnBlockAt round = true := by
  induction step with
  | nil => exact sent
  | cons firstStep remainingSteps ih =>
      have firstSent :=
        (validator_atomic_step_durable_monotone firstStep validator)
          |>.sent_own_block_persists sent
      exact ih firstSent

/-- A local action occurrence is at one correct, available validator. -/
theorem validator_local_action_occurrence_is_correct_available
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {validator : Nat}
    {action : ValidatorLocalAction BlockId CommitId}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (occurs : ValidatorLocalActionOccurs events validator action) :
    validator < config.authorityCount ∧
      faults.correctAvailable validator = true := by
  rcases validator_world_step_local_action_with_suffix step occurs with
    ⟨_, _, _, actionStep, _⟩
  cases actionStep
  exact ⟨by assumption, by assumption⟩

/-- A local action atomic step satisfies its basic action guard. -/
theorem validator_atomic_local_action_has_basic_guard
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {validator : Nat}
    {action : ValidatorLocalAction BlockId CommitId}
    (step : ValidatorAtomicStep config faults protocolPacket program time before
      (.localAction validator action) after) :
    BasicValidatorActionGuard config validator action
      (before.validatorState validator) := by
  cases step with
  | localAction _ _ concreteEnabled _ _ _ _ _ =>
      exact concreteEnabled.1

/-- A send action creates one exact addressed block packet. -/
theorem send_block_occurrence_creates_addressed_packet
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program execution)
    {time : Time} {validator receiver : Nat}
    {reference : ValidatorBlockRef BlockId}
    (occurs : ValidatorLocalActionOccurs (execution.events time) validator
      (.sendBlock receiver reference)) :
    ∃ (packetId : PacketId) (block : ValidatorBlock BlockId)
      (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      (execution.trace (time + 1)).packets packetId = some packet ∧
      block.reference = reference ∧
      protocolPacket packet ∧
      packet.sender = validator ∧
      packet.receiver = receiver ∧
      packet.payload = .block block ∧
      packet.sentAt = time + 1 := by
  rcases validator_world_step_local_action_with_suffix
      (execution.stepsFollowRules time) occurs with
    ⟨actionBefore, actionAfter, tailEvents, actionStep, tailStep⟩
  rcases effects.sendBlockCreatesPacket time actionBefore actionAfter validator
      receiver reference actionStep with
    ⟨packetId, block, packet, _, blockReference, _, packetAfter, protocol,
      sender, addressedReceiver, payload, sentAt⟩
  have packetFinal := validator_world_step_packet_persists tailStep packetAfter
  exact ⟨packetId, block, packet, packetFinal, blockReference, protocol, sender,
    addressedReceiver, payload, sentAt⟩

/-- Proposal persistence stores the exact own-block reference. -/
theorem persist_proposal_occurrence_stores_own_block
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {time : Time} {validator : Nat} {block : ValidatorBlock BlockId}
    (occurs : ValidatorLocalActionOccurs (execution.events time) validator
      (.persistProposal block)) :
    ((execution.trace (time + 1)).validatorState validator).ownBlockAt
      block.reference.round = some block.reference := by
  rcases validator_world_step_local_action_with_suffix
      (execution.stepsFollowRules time) occurs with
    ⟨_, actionAfter, _, actionStep, tailStep⟩
  have structural :=
    validator_atomic_local_action_has_structural_effect actionStep
  have ownEffect := structural.2.2.1
  have storedAfter :
      (actionAfter.validatorState validator).ownBlockAt block.reference.round =
        some block.reference := by
    exact ownEffect.2.1.1
  exact validator_world_step_own_block_persists tailStep storedAfter

/-- A send occurrence records that the exact own-block round was sent. -/
theorem send_block_occurrence_records_sent_own_block
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {time : Time} {validator receiver : Nat}
    {reference : ValidatorBlockRef BlockId}
    (occurs : ValidatorLocalActionOccurs (execution.events time) validator
      (.sendBlock receiver reference)) :
    ((execution.trace (time + 1)).validatorState validator).sentOwnBlockAt
      reference.round = true := by
  rcases validator_world_step_local_action_with_suffix
      (execution.stepsFollowRules time) occurs with
    ⟨_, actionAfter, _, actionStep, tailStep⟩
  have structural :=
    validator_atomic_local_action_has_structural_effect actionStep
  have sendEffect := structural.2.2.2.1
  have sentAfter :
      (actionAfter.validatorState validator).sentOwnBlockAt reference.round =
        true := by
    exact sendEffect.1
  exact validator_world_step_sent_own_block_persists tailStep sentAfter

/-- `proposeNext` with one parent list leads to persistence of that exact block.
-/
theorem propose_next_parents_lead_to_persisted_block
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {time : Time} {validator : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (proposalIsProtected : ∀ block,
      block.reference.author = validator →
      block.parents = parents →
      timed.protectedAction (time + 1) validator (.persistProposal block))
    (occurs : ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.proposeNext parents)) :
    ∃ (block : ValidatorBlock BlockId)
      (completion : ValidatorActionCompletion timed.execution
        timed.localActionBound validator (.persistProposal block) (time + 1)),
      block.reference.author = validator ∧
      block.parents = parents ∧
      completion.event.completedAt ≤ time + 1 + timed.localActionBound ∧
      ((timed.execution.trace (completion.event.completedAt + 1)).validatorState
        validator).ownBlockAt block.reference.round = some block.reference ∧
      (timed.execution.trace
        (completion.event.completedAt + 1)).blockCatalog block.reference.id =
          some block := by
  rcases effects.proposalEnablesPersistence time validator parents occurs with
    ⟨block, author, sameParents, enabled⟩
  have correct := validator_local_action_occurrence_is_correct_available
    (timed.execution.stepsFollowRules time) occurs
  have latched := proposalIsProtected block author sameParents
  let completion := timed.completeProtectedAction validator
    (.persistProposal block) (time + 1) correct.1 correct.2 latched
  have stored := persist_proposal_occurrence_stores_own_block timed.execution
    completion.occurs
  have catalog := effects.persistedProposalStoresBlock
    completion.event.completedAt validator block completion.occurs
  refine ⟨block, completion, author, sameParents, ?_, stored, catalog⟩
  simpa [completion.sameEnableTime] using completion.completesWithinBound

/-- A committer action returns the exact scan result for its local pending data.
-/
theorem run_committer_occurrence_returns_exact_pending_result
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program execution)
    {time : Time} {validator : Nat}
    (occurs : ValidatorLocalActionOccurs (execution.events time) validator
      .runCommitter) :
    ∃ input : ValidatorLocalState BlockId CommitId,
      input.committer.pendingRounds ≠ [] ∧
      effects.committerReturned
        { time
          validator
          input
          result := validatorPendingCommitterResult input } := by
  rcases validator_world_step_local_action_with_suffix
      (execution.stepsFollowRules time) occurs with
    ⟨actionBefore, actionAfter, _, actionStep, _⟩
  have returned := effects.runCommitterReturnsExactResult time actionBefore
    actionAfter validator actionStep
  have guard := validator_atomic_local_action_has_basic_guard actionStep
  exact ⟨actionBefore.validatorState validator, by
    simpa [BasicValidatorActionGuard] using guard, returned⟩

end Mysticeti
