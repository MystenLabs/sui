/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorProposalObligation

namespace Mysticeti

/-! Connect a durable proposal latch to the main validator execution.

The latch source map below is one-host state mapping. It does not state that a
future block, quorum layer, or common round exists. After the latch, bounded
local execution proves persistence and one exact addressed send to each other
correct, available validator.
-/

/-- One local or synchronized commit-install action occurs in a batch. -/
def ValidatorCommitInstallOccurs
    {BlockId CommitId PacketId : Type}
    (events : List (ValidatorAtomicEvent BlockId CommitId PacketId))
    (validator : Nat) (head : ValidatorCommitHead CommitId) : Prop :=
  ValidatorLocalActionOccurs events validator (.recordCommit head) ∨
    ValidatorLocalActionOccurs events validator (.applySyncedCommit head)

/-- The concrete same-host action that created one proposal latch. -/
def ValidatorProposalLatchMainOriginAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Time) (proposal : ValidatorReadyProposal BlockId) : Prop :=
  (proposal.origin = .commitProgressRecovery ∧
    ∃ parents,
      ValidatorLocalActionOccurs (timed.execution.events time) validator
          (.proposeNext parents) ∧
        proposal.block.parents = parents ∧
        ValidatorActionEnabledAt timed.execution (time + 1) validator
          (.persistProposal proposal.block)) ∨
  (proposal.origin = .normal ∧
    ∃ targetRound parents,
      ValidatorLocalActionOccurs (timed.execution.events time) validator
          (.proposeNormal targetRound parents) ∧
        proposal.block.reference.round = targetRound ∧
        proposal.block.parents = parents ∧
        ValidatorActionEnabledAt timed.execution (time + 1) validator
          (.persistProposal proposal.block))

/-- Bind normal and recovery proposal latches to their local target rules. -/
structure ValidatorProposalLatchSourceMap
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (obligations : ValidatorProposalObligationExecution timed) : Prop where
  proposeNextResultIsLatched : ∀ time validator parents block,
    ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.proposeNext parents) →
    block.reference.author = validator →
    block.parents = parents →
    ValidatorActionEnabledAt timed.execution (time + 1) validator
        (.persistProposal block) →
    ∃ proposal,
      obligations.event time validator = .latchProposal proposal ∧
      proposal.latchedAt = time + 1 ∧
      proposal.origin = .commitProgressRecovery ∧
      proposal.block = block
  normalProposalResultIsLatched : ∀ time validator targetRound parents block,
    ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.proposeNormal targetRound parents) →
    block.reference.author = validator →
    block.reference.round = targetRound →
    block.parents = parents →
    ValidatorActionEnabledAt timed.execution (time + 1) validator
        (.persistProposal block) →
    ∃ proposal,
      obligations.event time validator = .latchProposal proposal ∧
      proposal.latchedAt = time + 1 ∧
      proposal.origin = .normal ∧
      proposal.block = block
  /-- A latch event cannot exist without the same host's concrete normal or
  recovery proposal action. -/
  latchEventHasConcreteOrigin : ∀ time validator proposal,
    obligations.event time validator = .latchProposal proposal →
    proposal.latchedAt = time + 1 ∧
      ValidatorProposalLatchMainOriginAt timed time validator proposal
  /-- A plain commit-obligation event comes from the same host's concrete
  commit-install action. -/
  installCommitEventHasOrigin : ∀ time validator head,
    obligations.event time validator = .installCommit head →
    ValidatorCommitInstallOccurs (timed.execution.events time) validator head
  commitWithoutPersistenceIsReflected : ∀ time validator head,
    ValidatorCommitInstallOccurs (timed.execution.events time) validator head →
    (∀ block,
      ¬ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.persistProposal block)) →
    obligations.event time validator = .installCommit head

/-- Every current ready proposal has a concrete recovery proposal action and
cannot have a future latch time. -/
theorem ready_proposal_has_main_action_origin
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (obligations : ValidatorProposalObligationExecution timed)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    {time validator : Time} {proposal : ValidatorReadyProposal BlockId}
    (ready : (obligations.trace time validator).readyProposal = some proposal) :
    ∃ latchTime,
      latchTime < time ∧
        proposal.latchedAt = latchTime + 1 ∧
        proposal.latchedAt ≤ time ∧
        ValidatorProposalLatchMainOriginAt timed latchTime validator proposal := by
  rcases ready_proposal_has_latch_event obligations ready with
    ⟨latchTime, latchBefore, latched⟩
  rcases latchSource.latchEventHasConcreteOrigin latchTime validator proposal
      latched with ⟨latchedAt, origin⟩
  exact ⟨latchTime, latchBefore, latchedAt,
    by
      rw [latchedAt]
      exact Nat.succ_le_iff.mpr latchBefore,
    origin⟩

/-- The exact local target selected by a normal or recovery latch. -/
def ValidatorLatchedProposalTarget
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (readyAt validator : Nat)
    (proposal : ValidatorReadyProposal BlockId) : Prop :=
  proposal.origin = .commitProgressRecovery ∧
    proposal.block.reference.round =
      ((timed.execution.trace readyAt).validatorState
        validator).highestSignedRound + 1

/-- Commit installation and higher-round observation are cancellation attempts.
They are not proposal-completion events. -/
def ValidatorProposalCancellationAttempt
    {BlockId CommitId : Type}
    (proposal : ValidatorReadyProposal BlockId) :
    ValidatorProposalObligationEvent BlockId CommitId → Prop
  | .observeRound observedRound =>
      proposal.block.reference.round < observedRound
  | .installCommit _ => True
  | _ => False

/-- One exact proposal latch makes that proposal ready in the next state. -/
theorem latch_event_sets_ready_proposal
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {validator time : Nat}
    {proposal : ValidatorReadyProposal BlockId}
    {obligationsTrace : Time → Nat → ValidatorProposalObligationState BlockId}
    {obligationsEvent :
      Time → Nat → ValidatorProposalObligationEvent BlockId CommitId}
    (steps : ∀ currentTime currentValidator,
      ValidatorProposalObligationTransition config currentValidator
        (obligationsTrace currentTime currentValidator)
        (obligationsEvent currentTime currentValidator)
        (obligationsTrace (currentTime + 1) currentValidator))
    (latched : obligationsEvent time validator = .latchProposal proposal) :
    (obligationsTrace (time + 1) validator).readyProposal = some proposal := by
  have step := steps time validator
  rw [latched] at step
  cases step
  assumption

/-- A commit or higher-round observation cannot remove a ready proposal. -/
theorem cancellation_attempt_preserves_ready_proposal
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {validator : Nat}
    {before after : ValidatorProposalObligationState BlockId}
    {event : ValidatorProposalObligationEvent BlockId CommitId}
    {proposal : ValidatorReadyProposal BlockId}
    (step : ValidatorProposalObligationTransition config validator before event
      after)
    (ready : before.readyProposal = some proposal)
    (cancellation : ValidatorProposalCancellationAttempt proposal event) :
    after.readyProposal = some proposal := by
  cases event with
  | idle => simp [ValidatorProposalCancellationAttempt] at cancellation
  | latchProposal newProposal =>
      simp [ValidatorProposalCancellationAttempt] at cancellation
  | persistProposal block =>
      simp [ValidatorProposalCancellationAttempt] at cancellation
  | markBlockSent receiver reference =>
      simp [ValidatorProposalCancellationAttempt] at cancellation
  | observeRound observedRound =>
      rw [observe_round_preserves_ready_proposal step, ready]
  | installCommit head =>
      rw [install_commit_preserves_ready_proposal step, ready]

/-- Commit installation cannot clear an existing send obligation. -/
theorem commit_obligation_preserves_send_goal
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {validator receiver : Nat}
    {reference : ValidatorBlockRef BlockId}
    {head : ValidatorCommitHead CommitId}
    {before after : ValidatorProposalObligationState BlockId}
    (step : ValidatorProposalObligationTransition config validator before
      (.installCommit head) after)
    (sendGoal : before.sendGoal reference receiver = true) :
    after.sendGoal reference receiver = true := by
  exact send_goal_persists_without_matching_send step sendGoal (by simp)

/-- A ready quorum parent list is not empty. -/
private theorem proposal_latch_parent_list_nonempty
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorLocalState BlockId CommitId}
    {targetRound : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (ready : ValidatorParentListReady config state targetRound parents) :
    parents ≠ [] := by
  intro empty
  subst parents
  have quorumPositive := config.thresholds.quorum_positive
  have quorumParents := ready.2.2
  have noParentAuthors :
      validatorParentAuthors ([] : List (ValidatorBlockRef BlockId)) =
        VoterSet.empty := by
    rfl
  rw [noParentAuthors, weight_empty] at quorumParents
  omega

/-- A main-execution commit either shares its batch with exact proposal
persistence or preserves the ready proposal. -/
theorem commit_install_preserves_or_completes_ready_proposal
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
    (source : ValidatorProposalLatchSourceMap obligations)
    {time validator : Nat}
    {head : ValidatorCommitHead CommitId}
    {proposal : ValidatorReadyProposal BlockId}
    (ready : (obligations.trace time validator).readyProposal = some proposal)
    (commitOccurs : ValidatorCommitInstallOccurs
      (timed.execution.events time) validator head) :
    ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.persistProposal proposal.block) ∨
      (obligations.trace (time + 1) validator).readyProposal = some proposal := by
  by_cases persistenceOccurs : ∃ block,
      ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.persistProposal block)
  · rcases persistenceOccurs with ⟨block, occurs⟩
    have reflected := obligations.persistActionIsReflected time validator block
      occurs
    have step := obligations.transitionsFollowRules time validator
    rw [reflected] at step
    have sameBlock := ready_proposal_forces_matching_persistence step ready
    subst block
    exact Or.inl occurs
  · right
    have noPersistence : ∀ block,
        ¬ValidatorLocalActionOccurs (timed.execution.events time) validator
          (.persistProposal block) := by
      intro block occurs
      exact persistenceOccurs ⟨block, occurs⟩
    have reflected := source.commitWithoutPersistenceIsReflected time validator
      head commitOccurs noPersistence
    have step := obligations.transitionsFollowRules time validator
    rw [reflected] at step
    exact cancellation_attempt_preserves_ready_proposal step ready (by
      simp [ValidatorProposalCancellationAttempt])

/-- The concrete proposal action fixes the exact local target. -/
theorem latch_event_has_exact_local_target
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
    {time validator : Nat}
    {proposal : ValidatorReadyProposal BlockId}
    (latched : obligations.event time validator = .latchProposal proposal)
    (recoveryOrigin : proposal.origin = .commitProgressRecovery)
    (exactNext : proposal.block.reference.round =
      ((timed.execution.trace (time + 1)).validatorState
        validator).highestSignedRound + 1) :
    ValidatorLatchedProposalTarget timed (time + 1) validator
      proposal := by
  exact ⟨recoveryOrigin, exactNext⟩

/-- One world step keeps an existing block-catalog entry. -/
private theorem proposal_latch_world_step_preserves_catalog
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

/-- One world step keeps an existing sent-own-block record. -/
private theorem proposal_latch_world_step_preserves_sent_record
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
  | cons firstStep remainingSteps inductionHypothesis =>
      have sentAfterFirst :=
        (validator_atomic_step_durable_monotone firstStep validator)
          |>.sent_own_block_persists sent
      exact inductionHypothesis sentAfterFirst

/-- A completed send action records the sent own-block round. -/
private theorem proposal_latch_send_records_sent_round
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {time validator receiver : Nat}
    {reference : ValidatorBlockRef BlockId}
    (occurs : ValidatorLocalActionOccurs (execution.events time) validator
      (.sendBlock receiver reference)) :
    ((execution.trace (time + 1)).validatorState validator).sentOwnBlockAt
      reference.round = true := by
  rcases validator_world_step_local_action_with_suffix
      (execution.stepsFollowRules time) occurs with
    ⟨_actionBefore, actionAfter, _suffix, actionStep, suffixStep⟩
  have structural := validator_atomic_local_action_has_structural_effect
    actionStep
  have sentAfterAction := structural.2.2.2.1.1
  exact proposal_latch_world_step_preserves_sent_record suffixStep
    sentAfterAction

/-- A send action for a cataloged proposal creates a packet with that exact
proposal as its payload. -/
private theorem exact_proposal_send_creates_packet
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
    {time validator receiver : Nat}
    {block : ValidatorBlock BlockId}
    (catalog : (execution.trace time).blockCatalog block.reference.id =
      some block)
    (occurs : ValidatorLocalActionOccurs (execution.events time) validator
      (.sendBlock receiver block.reference)) :
    ∃ (packetId : PacketId)
        (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      (execution.trace (time + 1)).packets packetId = some packet ∧
      protocolPacket packet ∧
      packet.sender = validator ∧
      packet.receiver = receiver ∧
      packet.payload = .block block ∧
      packet.sentAt = time + 1 := by
  rcases occurs with ⟨headEvents, tailEvents, eventsEqual⟩
  have completeStep := execution.stepsFollowRules time
  rw [eventsEqual] at completeStep
  rcases validator_world_step_append_split completeStep with
    ⟨actionBefore, headStep, actionAndSuffix⟩
  cases actionAndSuffix with
  | cons actionStep suffixStep =>
      have catalogAtAction := proposal_latch_world_step_preserves_catalog
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

/-- One completed broadcast of an exact latched proposal to one peer. -/
structure ValidatorLatchedProposalBroadcast
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
    (readyAt validator receiver : Nat)
    (proposal : ValidatorReadyProposal BlockId) where
  persistedAt : Time
  sendActionAt : Time
  packetId : PacketId
  packet : AddressedPacket (ValidatorMessage BlockId CommitId)
  readyBeforePersistence : readyAt ≤ persistedAt
  persistenceWithinBound :
    persistedAt ≤ readyAt + timed.localActionBound
  persistenceOccurs : ValidatorLocalActionOccurs
    (timed.execution.events persistedAt) validator
    (.persistProposal proposal.block)
  ownBlockStored :
    ((timed.execution.trace (persistedAt + 1)).validatorState validator).ownBlockAt
      proposal.block.reference.round = some proposal.block.reference
  proposalCataloged :
    (timed.execution.trace (persistedAt + 1)).blockCatalog
      proposal.block.reference.id = some proposal.block
  sendGoalCreated :
    (obligations.trace (persistedAt + 1) validator).sendGoal
      proposal.block.reference receiver = true
  sendGoalProtected : timed.protectedAction (persistedAt + 1) validator
    (.sendBlock receiver proposal.block.reference)
  persistenceBeforeSend : persistedAt + 1 ≤ sendActionAt
  sendWithinBound :
    sendActionAt ≤ persistedAt + 1 + timed.localActionBound
  sendOccurs : ValidatorLocalActionOccurs
    (timed.execution.events sendActionAt) validator
    (.sendBlock receiver proposal.block.reference)
  sentOwnBlockRecorded :
    ((timed.execution.trace (sendActionAt + 1)).validatorState
      validator).sentOwnBlockAt proposal.block.reference.round = true
  packetInTrace :
    (timed.execution.trace (sendActionAt + 1)).packets packetId = some packet
  packetIsProtocol : protocolPacket packet
  packetSender : packet.sender = validator
  packetReceiver : packet.receiver = receiver
  packetPayload : packet.payload = .block proposal.block
  packetSentAt : packet.sentAt = sendActionAt + 1

/-- The local result after one exact proposal is latched. -/
def ValidatorLatchedProposalResult
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
    (readyAt validator : Nat)
    (proposal : ValidatorReadyProposal BlockId) : Prop :=
  ValidatorLatchedProposalTarget timed readyAt validator proposal ∧
    ValidatorProposalParentListReady proposal.origin config
      ((timed.execution.trace readyAt).validatorState validator)
      proposal.block.reference.round proposal.block.parents ∧
    (∀ currentTime,
      (obligations.trace currentTime validator).readyProposal = some proposal →
      ValidatorProposalParentListReady proposal.origin config
        ((timed.execution.trace currentTime).validatorState validator)
        proposal.block.reference.round proposal.block.parents) ∧
    (∀ currentTime,
      (obligations.trace currentTime validator).readyProposal = some proposal →
      (∀ block,
        obligations.event currentTime validator ≠ .persistProposal block) →
      (obligations.trace (currentTime + 1) validator).readyProposal =
          some proposal ∧
        ValidatorProposalParentListReady proposal.origin config
          ((timed.execution.trace (currentTime + 1)).validatorState validator)
          proposal.block.reference.round proposal.block.parents) ∧
    (∀ currentTime,
      (obligations.trace currentTime validator).readyProposal = some proposal →
      ValidatorProposalCancellationAttempt proposal
        (obligations.event currentTime validator) →
      (obligations.trace (currentTime + 1) validator).readyProposal =
        some proposal) ∧
    (∀ currentTime head,
      (obligations.trace currentTime validator).readyProposal = some proposal →
      ValidatorCommitInstallOccurs (timed.execution.events currentTime)
        validator head →
      ValidatorLocalActionOccurs (timed.execution.events currentTime) validator
          (.persistProposal proposal.block) ∨
        (obligations.trace (currentTime + 1) validator).readyProposal =
          some proposal) ∧
    ∀ receiver,
      receiver < config.authorityCount →
      receiver ≠ validator →
      Nonempty (ValidatorLatchedProposalBroadcast timed obligations readyAt
        validator receiver proposal)

/-- A ready proposal persists and creates one addressed block send to every
other validator. This result does not depend on the proposal origin or target
selection rule. -/
theorem ready_proposal_broadcasts_to_every_other_validator
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
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {readyAt validator : Nat}
    {proposal : ValidatorReadyProposal BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ready : (obligations.trace readyAt validator).readyProposal =
      some proposal) :
    ∀ receiver,
      receiver < config.authorityCount →
      receiver ≠ validator →
      Nonempty (ValidatorLatchedProposalBroadcast timed obligations readyAt
        validator receiver proposal) := by
  intro receiver receiverInRange differentValidator
  rcases latched_proposal_run_creates_send_goal obligations validatorInRange
      validatorCorrectAvailable receiverInRange differentValidator ready with
    ⟨persistCompletion, readyBeforePersistence, persistenceWithinBound,
      sendGoal⟩
  let persistedAt := persistCompletion.event.completedAt
  have persistenceOccurs := persistCompletion.occurs
  have ownBlockStored := persist_proposal_occurrence_stores_own_block
    timed.execution persistenceOccurs
  have proposalCataloged := effects.persistedProposalStoresBlock persistedAt
    validator proposal.block persistenceOccurs
  have sendGoalProtected := obligations.sendGoalIsProtected
    (persistedAt + 1) validator receiver proposal.block.reference sendGoal
  rcases latched_send_goal_runs_within_bound obligations validatorInRange
      validatorCorrectAvailable sendGoal with
    ⟨sendCompletion, persistenceBeforeSend, sendWithinBound, sendOccurs⟩
  let sendActionAt := sendCompletion.event.completedAt
  have catalogAtSend := timed.execution.blockCatalogMonotone
    (persistedAt + 1) sendActionAt persistenceBeforeSend
    proposal.block.reference.id proposal.block proposalCataloged
  rcases exact_proposal_send_creates_packet effects catalogAtSend sendOccurs with
    ⟨packetId, packet, packetInTrace, packetProtocol, packetSender,
      packetReceiver, packetPayload, packetSentAt⟩
  have sentOwnBlockRecorded := proposal_latch_send_records_sent_round sendOccurs
  exact ⟨
    { persistedAt
      sendActionAt
      packetId
      packet
      readyBeforePersistence
      persistenceWithinBound
      persistenceOccurs
      ownBlockStored
      proposalCataloged
      sendGoalCreated := sendGoal
      sendGoalProtected
      persistenceBeforeSend
      sendWithinBound
      sendOccurs
      sentOwnBlockRecorded
      packetInTrace
      packetIsProtocol := packetProtocol
      packetSender
      packetReceiver
      packetPayload
      packetSentAt }⟩

/-- A legal exact latch cannot be canceled and broadcasts the exact proposal to
every other correct, available validator. -/
theorem latched_proposal_persists_and_sends_to_every_correct_peer
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
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {time validator : Nat}
    {proposal : ValidatorReadyProposal BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (latched : obligations.event time validator = .latchProposal proposal)
    (recoveryOrigin : proposal.origin = .commitProgressRecovery)
    (exactNext : proposal.block.reference.round =
      ((timed.execution.trace (time + 1)).validatorState
        validator).highestSignedRound + 1) :
    ValidatorLatchedProposalResult timed obligations (time + 1)
      validator proposal := by
  unfold ValidatorLatchedProposalResult
  have ready := latch_event_sets_ready_proposal
    obligations.transitionsFollowRules latched
  have exactTarget := latch_event_has_exact_local_target latched recoveryOrigin
    exactNext
  have legal := obligations.readyProposalIsLegal (time + 1) validator proposal
    ready
  refine ⟨exactTarget, legal.2.2.2, ?_, ?_, ?_, ?_, ?_⟩
  · intro currentTime currentReady
    exact (obligations.readyProposalIsLegal currentTime validator proposal
      currentReady).2.2.2
  · intro currentTime currentReady notPersist
    have nextReady := ready_proposal_persists_without_persist_action
      (obligations.transitionsFollowRules currentTime validator) currentReady
      notPersist
    exact ⟨nextReady,
      (obligations.readyProposalIsLegal (currentTime + 1) validator proposal
        nextReady).2.2.2⟩
  · intro currentTime currentReady cancellation
    exact cancellation_attempt_preserves_ready_proposal
      (obligations.transitionsFollowRules currentTime validator) currentReady
      cancellation
  · intro currentTime head currentReady commitOccurs
    exact commit_install_preserves_or_completes_ready_proposal source currentReady
      commitOccurs
  · intro receiver receiverInRange differentValidator
    exact ready_proposal_broadcasts_to_every_other_validator effects
      validatorInRange validatorCorrectAvailable ready receiver receiverInRange
        differentValidator

/-- Bounded execution connects `proposeNext` to the exact latch, persistence,
and addressed sends. The input is one protected local action. It is not a future
block or layer. -/
theorem enabled_propose_next_completes_exact_latched_pipeline
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
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {start validator : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (isProtected : timed.protectedAction start validator
      (.proposeNext parents)) :
    ∃ proposalActionAt proposal,
      start ≤ proposalActionAt ∧
      proposalActionAt ≤ start + timed.localActionBound ∧
      ValidatorLocalActionOccurs
          (timed.execution.events proposalActionAt) validator
          (.proposeNext parents) ∧
      proposal.latchedAt = proposalActionAt + 1 ∧
      proposal.origin = .commitProgressRecovery ∧
      proposal.block.reference.author = validator ∧
      proposal.block.parents = parents ∧
      proposal.block.reference.round =
        ((timed.execution.trace start).validatorState
          validator).highestSignedRound + 1 ∧
      ValidatorLatchedProposalResult timed obligations
        (proposalActionAt + 1) validator proposal := by
  have enabled := timed.protectedActionIsEnabled start validator
    (.proposeNext parents) isProtected
  let proposalCompletion := timed.completeProtectedAction validator
    (.proposeNext parents) start validatorInRange validatorCorrectAvailable
      isProtected
  let proposalActionAt := proposalCompletion.event.completedAt
  have proposalOccurs : ValidatorLocalActionOccurs
      (timed.execution.events proposalActionAt) validator
        (.proposeNext parents) :=
    proposalCompletion.occurs
  rcases effects.proposalEnablesPersistence proposalActionAt validator parents
      proposalOccurs with
    ⟨block, blockAuthor, blockParents, persistenceEnabled⟩
  rcases source.proposeNextResultIsLatched proposalActionAt validator parents
      block proposalOccurs blockAuthor blockParents persistenceEnabled with
    ⟨proposal, latched, latchedAt, proposalOrigin, proposalBlock⟩
  have proposalAuthor : proposal.block.reference.author = validator := by
    rw [proposalBlock]
    exact blockAuthor
  have proposalParents : proposal.block.parents = parents := by
    rw [proposalBlock]
    exact blockParents
  have startGuard := enabled.2.1
  change ∃ recovery,
      ((timed.execution.trace start).validatorState validator).recovery =
          some recovery ∧
        recovery.alignmentWitness = none ∧
        recovery.targetRound =
          ((timed.execution.trace start).validatorState
            validator).highestSignedRound + 1 ∧
        (∃ deadline,
          recovery.deadline = some deadline ∧
            deadline ≤
              ((timed.execution.trace start).validatorState validator).clock) ∧
        ValidatorParentListReady config
          ((timed.execution.trace start).validatorState validator)
          recovery.targetRound parents at startGuard
  rcases startGuard with
    ⟨recovery, _recoveryState, _noAlignment, targetIsNext,
      _deadline, parentsReadyAtStart⟩
  have parentsNonempty := proposal_latch_parent_list_nonempty parentsReadyAtStart
  rcases List.exists_mem_of_ne_nil parents parentsNonempty with
    ⟨parent, parentInParents⟩
  have ready := latch_event_sets_ready_proposal
    obligations.transitionsFollowRules latched
  have legalAtLatch := obligations.readyProposalIsLegal (proposalActionAt + 1)
    validator proposal ready
  have parentInProposal : parent ∈ proposal.block.parents := by
    rw [proposalParents]
    exact parentInParents
  have parentTargetsStart :=
    (parentsReadyAtStart.2.1 parent parentInParents).1
  have parentTargetsProposal :=
    (legalAtLatch.2.2.2.1.2.1 parent parentInProposal).1
  have proposalRoundFromStart : proposal.block.reference.round =
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound + 1 := by
    omega
  have startBeforeProposalAction : start ≤ proposalActionAt := by
    simpa [proposalActionAt, proposalCompletion.sameEnableTime] using
      proposalCompletion.enableBeforeCompletion
  have startBeforeLatch : start ≤ proposalActionAt + 1 :=
    Nat.le_trans startBeforeProposalAction (Nat.le_add_right _ _)
  have durable := timed.execution.durableStateMonotone validator start
    (proposalActionAt + 1) validatorInRange startBeforeLatch
  rcases durable with
    ⟨_commitIndex, _commitRound, _sameCommit, _installed, _installSource,
      _lastCommit, signerFloorMonotone, _ownBlock, _sentBlock, _accepted,
      _representative, _gcRound⟩
  have exactNext : proposal.block.reference.round =
      ((timed.execution.trace (proposalActionAt + 1)).validatorState
        validator).highestSignedRound + 1 := by
    have latchSignerBeforeProposal := legalAtLatch.2.1
    omega
  have result := latched_proposal_persists_and_sends_to_every_correct_peer
    source effects validatorInRange validatorCorrectAvailable
      latched proposalOrigin exactNext
  refine ⟨proposalActionAt, proposal, ?_, ?_, proposalOccurs, latchedAt, proposalOrigin,
    proposalAuthor, proposalParents, proposalRoundFromStart, result⟩
  · simpa [proposalActionAt, proposalCompletion.sameEnableTime] using
      proposalCompletion.enableBeforeCompletion
  · simpa [proposalActionAt, proposalCompletion.sameEnableTime] using
      proposalCompletion.completesWithinBound

end Mysticeti
