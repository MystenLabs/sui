/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorTimedExecutionLemmas

namespace Mysticeti

/-! Durable local obligations for proposal persistence and block send.

A ready proposal is local work that must persist one exact block. The proposal
can come from normal operation or commit progress recovery. Round observation
and commit installation do not remove it. Proposal persistence creates one send
goal for each other validator.

These fields contain pending local work. They do not state that a future action
or a protocol result exists.
-/

/-- A parent list is ready and remains usable at the local GC boundary.

`retained parent = true` means that the local parent block body is present.
-/
def UsableValidatorParentListReady
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (state : ValidatorLocalState BlockId CommitId)
    (targetRound : Nat)
    (parents : List (ValidatorBlockRef BlockId)) : Prop :=
  ValidatorParentListReady config state targetRound parents ∧
    ∀ parent, parent ∈ parents →
      state.retained parent = true ∧
        (parent.round = 0 ∨ state.gcRound < parent.round)

/-- The local source of one ready proposal. -/
inductive ValidatorProposalOrigin where
  | normal
  | commitProgressRecovery
  deriving DecidableEq, Repr

/-- A proposal uses only genesis parents or parents above the local GC round.

Recovery pins can retain older blocks as committed roots. They cannot make an
at-or-below-GC block legal as an immediate proposal parent.
-/
def ValidatorProposalParentListReady
    {BlockId CommitId : Type}
    (_origin : ValidatorProposalOrigin)
    (config : ValidatorEpochConfig CommitId)
    (state : ValidatorLocalState BlockId CommitId)
    (targetRound : Nat)
    (parents : List (ValidatorBlockRef BlockId)) : Prop :=
  ValidatorParentListReady config state targetRound parents ∧
    ∀ parent, parent ∈ parents →
      state.retained parent = true ∧
        (parent.round = 0 ∨ state.gcRound < parent.round)

/-- One exact proposal that is ready for durable persistence. -/
structure ValidatorReadyProposal (BlockId : Type) where
  origin : ValidatorProposalOrigin
  block : ValidatorBlock BlockId
  latchedAt : Time

/-- Pending local work for one validator. -/
structure ValidatorProposalObligationState (BlockId : Type) where
  readyProposal : Option (ValidatorReadyProposal BlockId)
  sendGoal : ValidatorBlockRef BlockId → Nat → Bool

/-- The host has one durable proposal or send obligation. This predicate is a
current local state fact. It does not state that the work finishes. -/
def ValidatorProposalObligationState.HasWork
    {BlockId : Type}
    (state : ValidatorProposalObligationState BlockId) : Prop :=
  (∃ proposal, state.readyProposal = some proposal) ∨
    ∃ reference receiver, state.sendGoal reference receiver = true

/-- One local change to proposal and send obligations. -/
inductive ValidatorProposalObligationEvent (BlockId CommitId : Type) where
  | idle
  | latchProposal (proposal : ValidatorReadyProposal BlockId)
  | persistProposal (block : ValidatorBlock BlockId)
  | markBlockSent (receiver : Nat) (reference : ValidatorBlockRef BlockId)
  | observeRound (round : Nat)
  | installCommit (head : ValidatorCommitHead CommitId)

/-- One ready proposal has enough local data for legal persistence. -/
def ValidatorReadyProposalLegal
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (validator : Nat)
    (state : ValidatorLocalState BlockId CommitId)
    (proposal : ValidatorReadyProposal BlockId) : Prop :=
  proposal.block.reference.author = validator ∧
    state.highestSignedRound < proposal.block.reference.round ∧
    (proposal.origin = .commitProgressRecovery →
      proposal.block.reference.round = state.highestSignedRound + 1) ∧
    ValidatorProposalParentListReady proposal.origin config state
      proposal.block.reference.round proposal.block.parents

/-- Local transition rules for ready proposals and send goals. -/
inductive ValidatorProposalObligationTransition
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (validator : Nat) :
    ValidatorProposalObligationState BlockId →
      ValidatorProposalObligationEvent BlockId CommitId →
      ValidatorProposalObligationState BlockId → Prop where
  | idle {state} :
      ValidatorProposalObligationTransition config validator state .idle state
  | latchProposal {before after proposal} :
      before.readyProposal = none →
      after.readyProposal = some proposal →
      after.sendGoal = before.sendGoal →
      ValidatorProposalObligationTransition config validator before
        (.latchProposal proposal) after
  | persistProposal {before after proposal block} :
      before.readyProposal = some proposal →
      proposal.block = block →
      after.readyProposal = none →
      (∀ reference receiver,
        after.sendGoal reference receiver = true ↔
          before.sendGoal reference receiver = true ∨
            (reference = block.reference ∧
              receiver < config.authorityCount ∧ receiver ≠ validator)) →
      ValidatorProposalObligationTransition config validator before
        (.persistProposal block) after
  | markBlockSent {before after receiver reference} :
      before.sendGoal reference receiver = true →
      after.readyProposal = before.readyProposal →
      after.sendGoal reference receiver = false →
      (∀ otherReference otherReceiver,
        otherReference ≠ reference ∨ otherReceiver ≠ receiver →
          after.sendGoal otherReference otherReceiver =
            before.sendGoal otherReference otherReceiver) →
      ValidatorProposalObligationTransition config validator before
        (.markBlockSent receiver reference) after
  | observeRound {state round} :
      ValidatorProposalObligationTransition config validator state
        (.observeRound round) state
  | installCommit {state head} :
      ValidatorProposalObligationTransition config validator state
        (.installCommit head) state

/-- A legal ready proposal satisfies the base persistence guard. -/
theorem legal_ready_proposal_has_basic_persist_guard
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {validator : Nat}
    {state : ValidatorLocalState BlockId CommitId}
    {proposal : ValidatorReadyProposal BlockId}
    (legal : ValidatorReadyProposalLegal config validator state proposal) :
    BasicValidatorActionGuard config validator
      (.persistProposal proposal.block) state := by
  rcases legal with ⟨author, laterRound, _, ready, _⟩
  rcases ready with ⟨parentAuthorsNodup, parentsReady, quorum⟩
  refine ⟨author, laterRound, ?_, ?_⟩
  · exact ⟨parentAuthorsNodup,
      fun parent member => (parentsReady parent member).1, quorum⟩
  · intro parent member
    exact (parentsReady parent member).2

/-- A legal recovery proposal is exactly one round above the signer floor. -/
theorem legal_recovery_proposal_is_exact_next
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {validator : Nat}
    {state : ValidatorLocalState BlockId CommitId}
    {proposal : ValidatorReadyProposal BlockId}
    (legal : ValidatorReadyProposalLegal config validator state proposal)
    (recovery : proposal.origin = .commitProgressRecovery) :
    proposal.block.reference.round = state.highestSignedRound + 1 :=
  legal.2.2.1 recovery

/-- Normal proposal parent readiness is the standard GC-usable readiness. -/
theorem normal_proposal_parent_list_ready_is_usable
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorLocalState BlockId CommitId}
    {targetRound : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (ready : ValidatorProposalParentListReady .normal config state targetRound
      parents) :
    UsableValidatorParentListReady config state targetRound parents := by
  exact ready

/-- Round observation cannot cancel a ready proposal. -/
theorem observe_round_preserves_ready_proposal
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {validator round : Nat}
    {before after : ValidatorProposalObligationState BlockId}
    (step : ValidatorProposalObligationTransition (CommitId := CommitId) config
      validator before (.observeRound round) after) :
    after.readyProposal = before.readyProposal := by
  cases step
  rfl

/-- Commit installation cannot cancel a ready proposal. -/
theorem install_commit_preserves_ready_proposal
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {validator : Nat} {head : ValidatorCommitHead CommitId}
    {before after : ValidatorProposalObligationState BlockId}
    (step : ValidatorProposalObligationTransition config validator before
      (.installCommit head) after) :
    after.readyProposal = before.readyProposal := by
  cases step
  rfl

/-- A ready proposal remains latched until its matching persistence action. -/
theorem ready_proposal_persists_without_persist_action
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {validator : Nat}
    {before after : ValidatorProposalObligationState BlockId}
    {event : ValidatorProposalObligationEvent BlockId CommitId}
    {proposal : ValidatorReadyProposal BlockId}
    (step : ValidatorProposalObligationTransition config validator before event
      after)
    (ready : before.readyProposal = some proposal)
    (notPersist : ∀ block, event ≠ .persistProposal block) :
    after.readyProposal = some proposal := by
  cases step with
  | idle => exact ready
  | latchProposal empty => rw [ready] at empty; contradiction
  | persistProposal => exact False.elim (notPersist _ rfl)
  | markBlockSent _ unchanged => rw [unchanged, ready]
  | observeRound => exact ready
  | installCommit => exact ready

/-- While a proposal is ready, a persistence transition must use that block. -/
theorem ready_proposal_forces_matching_persistence
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {validator : Nat}
    {before after : ValidatorProposalObligationState BlockId}
    {proposal : ValidatorReadyProposal BlockId}
    {block : ValidatorBlock BlockId}
    (step : ValidatorProposalObligationTransition config validator before
      (.persistProposal block) after)
    (ready : before.readyProposal = some proposal) :
    block = proposal.block := by
  cases step with
  | persistProposal transitionReady matchingBlock =>
      have sameProposal : _ = _ := Option.some.inj (transitionReady.symm.trans ready)
      subst proposal
      exact matchingBlock.symm

/-- Proposal persistence creates a send goal for every other validator. -/
theorem persist_proposal_creates_send_goals
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {validator receiver : Nat}
    {before after : ValidatorProposalObligationState BlockId}
    {block : ValidatorBlock BlockId}
    (step : ValidatorProposalObligationTransition config validator before
      (.persistProposal block) after)
    (receiverInRange : receiver < config.authorityCount)
    (differentValidator : receiver ≠ validator) :
    after.sendGoal block.reference receiver = true := by
  cases step with
  | persistProposal _ _ _ sendGoals =>
      exact (sendGoals block.reference receiver).2
        (Or.inr ⟨rfl, receiverInRange, differentValidator⟩)

/-- A send goal remains set until its matching send completion. -/
theorem send_goal_persists_without_matching_send
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {validator receiver : Nat}
    {reference : ValidatorBlockRef BlockId}
    {before after : ValidatorProposalObligationState BlockId}
    {event : ValidatorProposalObligationEvent BlockId CommitId}
    (step : ValidatorProposalObligationTransition config validator before event
      after)
    (sendGoal : before.sendGoal reference receiver = true)
    (notMatchingSend : event ≠ .markBlockSent receiver reference) :
    after.sendGoal reference receiver = true := by
  cases step
  case idle => exact sendGoal
  case latchProposal =>
    rename_i _ _ _ unchanged
    rw [unchanged]
    exact sendGoal
  case persistProposal =>
    rename_i _ _ _ _ _ sendGoals
    exact (sendGoals reference receiver).2 (Or.inl sendGoal)
  case markBlockSent =>
    rename_i sentReceiver sentReference _ _ _ otherGoals
    by_cases sameReference : reference = sentReference
    · by_cases sameReceiver : receiver = sentReceiver
      · subst sentReference
        subst sentReceiver
        exact False.elim (notMatchingSend rfl)
      · rw [otherGoals reference receiver (Or.inr sameReceiver)]
        exact sendGoal
    · rw [otherGoals reference receiver (Or.inl sameReference)]
      exact sendGoal
  case observeRound => exact sendGoal
  case installCommit => exact sendGoal

/-- A matching send completion clears only that local send goal. -/
theorem matching_send_clears_goal
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {validator receiver : Nat}
    {reference : ValidatorBlockRef BlockId}
    {before after : ValidatorProposalObligationState BlockId}
    (step : ValidatorProposalObligationTransition config validator before
      (.markBlockSent receiver reference) after) :
    after.sendGoal reference receiver = false := by
  cases step
  assumption

/-- A ready proposal after one transition was either latched by that transition
or was already ready before it. -/
theorem ready_proposal_after_transition_has_source
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {validator : Nat}
    {before after : ValidatorProposalObligationState BlockId}
    {event : ValidatorProposalObligationEvent BlockId CommitId}
    {proposal : ValidatorReadyProposal BlockId}
    (step : ValidatorProposalObligationTransition config validator before event
      after)
    (ready : after.readyProposal = some proposal) :
    event = .latchProposal proposal ∨
      before.readyProposal = some proposal := by
  cases step with
  | idle => exact Or.inr ready
  | latchProposal _ stored _ =>
      have sameProposal := Option.some.inj (stored.symm.trans ready)
      subst proposal
      exact Or.inl rfl
  | persistProposal _ _ cleared _ =>
      rw [ready] at cleared
      contradiction
  | markBlockSent _ unchanged _ _ =>
      exact Or.inr (unchanged.symm.trans ready)
  | observeRound => exact Or.inr ready
  | installCommit => exact Or.inr ready

/-- A local execution with durable proposal and send obligations. -/
structure ValidatorProposalObligationExecution
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  trace : Time → Nat → ValidatorProposalObligationState BlockId
  event : Time → Nat → ValidatorProposalObligationEvent BlockId CommitId
  transitionsFollowRules : ∀ time validator,
    ValidatorProposalObligationTransition config validator
      (trace time validator) (event time validator) (trace (time + 1) validator)
  readyProposalIsLegal : ∀ time validator proposal,
    (trace time validator).readyProposal = some proposal →
    ValidatorReadyProposalLegal config validator
      ((timed.execution.trace time).validatorState validator) proposal
  readyProposalIsProtected : ∀ time validator proposal,
    (trace time validator).readyProposal = some proposal →
    timed.protectedAction time validator (.persistProposal proposal.block)
  sendGoalIsProtected : ∀ time validator receiver reference,
    (trace time validator).sendGoal reference receiver = true →
    timed.protectedAction time validator (.sendBlock receiver reference)
  /-- An outstanding send goal keeps its block as the latest signed block. The
  matching send step cannot also persist a later block. -/
  sendGoalSerializesProposalPersistence : ∀ time validator receiver reference,
    (trace time validator).sendGoal reference receiver = true →
    ((timed.execution.trace time).validatorState validator).ownBlockAt
        reference.round = some reference →
    ((timed.execution.trace time).validatorState
        validator).highestSignedRound = reference.round ∧
      (ValidatorLocalActionOccurs (timed.execution.events time) validator
          (.sendBlock receiver reference) →
        ((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound = reference.round)
  persistActionIsReflected : ∀ time validator block,
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.persistProposal block) →
    event time validator = .persistProposal block
  persistEventHasActionOrigin : ∀ time validator block,
    event time validator = .persistProposal block →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.persistProposal block)
  sendActionIsReflected : ∀ time validator receiver reference,
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.sendBlock receiver reference) →
    event time validator = .markBlockSent receiver reference
  sendEventHasActionOrigin : ∀ time validator receiver reference,
    event time validator = .markBlockSent receiver reference →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.sendBlock receiver reference)
  initialStateHasNoWork : ∀ validator,
    (trace 0 validator).readyProposal = none ∧
      ∀ reference receiver,
        (trace 0 validator).sendGoal reference receiver = false

/-- Every current ready proposal has one earlier concrete latch event. The
empty initial state and exact transitions derive this origin. -/
theorem ready_proposal_has_latch_event
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
    {time validator : Time} {proposal : ValidatorReadyProposal BlockId}
    (ready : (obligations.trace time validator).readyProposal = some proposal) :
    ∃ latchTime,
      latchTime < time ∧
        obligations.event latchTime validator = .latchProposal proposal := by
  induction time with
  | zero =>
      rw [obligations.initialStateHasNoWork validator |>.1] at ready
      contradiction
  | succ previous inductionHypothesis =>
      have step := obligations.transitionsFollowRules previous validator
      rcases ready_proposal_after_transition_has_source step ready with
        latched | readyBefore
      · exact ⟨previous, Nat.lt_succ_self _, latched⟩
      · rcases inductionHypothesis readyBefore with
          ⟨latchTime, latchBefore, latched⟩
        exact ⟨latchTime,
          Nat.lt_trans latchBefore (Nat.lt_succ_self previous), latched⟩

/-- A send goal after one transition was either created by proposal persistence
or was already present before the transition. -/
theorem send_goal_after_transition_has_source
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {validator receiver : Nat}
    {reference : ValidatorBlockRef BlockId}
    {before after : ValidatorProposalObligationState BlockId}
    {event : ValidatorProposalObligationEvent BlockId CommitId}
    (step : ValidatorProposalObligationTransition config validator before event
      after)
    (goal : after.sendGoal reference receiver = true) :
    (∃ block,
      event = .persistProposal block ∧
        reference = block.reference ∧
        receiver < config.authorityCount ∧ receiver ≠ validator) ∨
      before.sendGoal reference receiver = true := by
  cases step
  case idle => exact Or.inr goal
  case latchProposal =>
    rename_i _ _ unchanged
    rw [unchanged] at goal
    exact Or.inr goal
  case persistProposal =>
    rename_i _ _ _ sendGoals
    rcases (sendGoals reference receiver).1 goal with old | created
    · exact Or.inr old
    · exact Or.inl ⟨_, rfl, created⟩
  case markBlockSent =>
    rename_i sentReceiver sentReference _ _ cleared otherGoals
    by_cases sameReference : reference = sentReference
    · by_cases sameReceiver : receiver = sentReceiver
      · subst sentReference
        subst sentReceiver
        rw [cleared] at goal
        contradiction
      · have same := otherGoals reference receiver (Or.inr sameReceiver)
        rw [same] at goal
        exact Or.inr goal
    · have same := otherGoals reference receiver (Or.inl sameReference)
      rw [same] at goal
      exact Or.inr goal
  case observeRound => exact Or.inr goal
  case installCommit => exact Or.inr goal

/-- Every current send goal comes from one earlier concrete persistence event.
The empty initial state and exact transitions derive this origin. -/
theorem send_goal_has_persist_event
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
    {time validator receiver : Time}
    {reference : ValidatorBlockRef BlockId}
    (goal : (obligations.trace time validator).sendGoal reference receiver =
      true) :
    ∃ persistTime block,
      persistTime < time ∧
        ValidatorLocalActionOccurs (timed.execution.events persistTime)
          validator (.persistProposal block) ∧
        reference = block.reference ∧
        receiver < config.authorityCount ∧ receiver ≠ validator := by
  induction time with
  | zero =>
      rw [obligations.initialStateHasNoWork validator |>.2 reference receiver]
        at goal
      contradiction
  | succ previous inductionHypothesis =>
      have step := obligations.transitionsFollowRules previous validator
      rcases send_goal_after_transition_has_source step goal with
        ⟨block, reflected, sameReference, receiverInRange, receiverIsOther⟩ |
          goalBefore
      · exact ⟨previous, block, Nat.lt_succ_self _,
          obligations.persistEventHasActionOrigin previous validator block
            reflected,
          sameReference, receiverInRange, receiverIsOther⟩
      · rcases inductionHypothesis goalBefore with
          ⟨persistTime, block, persistedBefore, occurs, sameReference,
            receiverInRange, receiverIsOther⟩
        exact ⟨persistTime, block,
          Nat.lt_trans persistedBefore (Nat.lt_succ_self previous), occurs,
          sameReference, receiverInRange, receiverIsOther⟩

/-- Every latched parent remains retained and stays above GC or at genesis. -/
theorem latched_proposal_parent_is_retained_and_permitted
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
    {time : Time} {validator : Nat}
    {proposal : ValidatorReadyProposal BlockId}
    {parent : ValidatorBlockRef BlockId}
    (latched : (obligations.trace time validator).readyProposal =
      some proposal)
    (parentMember : parent ∈ proposal.block.parents) :
    ((timed.execution.trace time).validatorState validator).retained parent =
        true ∧
      (parent.round = 0 ∨
        ((timed.execution.trace time).validatorState validator).gcRound <
          parent.round) := by
  have legal := obligations.readyProposalIsLegal time validator proposal latched
  exact legal.2.2.2.2 parent parentMember

/-- Every normal-proposal parent remains retained and usable after GC. -/
theorem latched_normal_proposal_parent_is_usable
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
    {time : Time} {validator : Nat}
    {proposal : ValidatorReadyProposal BlockId}
    {parent : ValidatorBlockRef BlockId}
    (latched : (obligations.trace time validator).readyProposal =
      some proposal)
    (_normal : proposal.origin = .normal)
    (parentMember : parent ∈ proposal.block.parents) :
    ((timed.execution.trace time).validatorState validator).retained parent =
        true ∧
      (parent.round = 0 ∨
        ((timed.execution.trace time).validatorState validator).gcRound <
          parent.round) := by
  have permitted := latched_proposal_parent_is_retained_and_permitted
    obligations latched parentMember
  exact permitted

/-- A latched normal or recovery proposal runs within the local action bound. -/
theorem latched_proposal_runs_within_bound
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
    {time : Time} {validator : Nat}
    {proposal : ValidatorReadyProposal BlockId}
    (validatorInRange : validator < config.authorityCount)
    (correctAvailable : faults.correctAvailable validator = true)
    (latched : (obligations.trace time validator).readyProposal =
      some proposal) :
    ∃ completion : ValidatorActionCompletion timed.execution
        timed.localActionBound validator (.persistProposal proposal.block) time,
      time ≤ completion.event.completedAt ∧
      completion.event.completedAt ≤ time + timed.localActionBound ∧
      ValidatorLocalActionOccurs
        (timed.execution.events completion.event.completedAt) validator
        (.persistProposal proposal.block) := by
  exact protected_validator_action_completes_within_bound timed validatorInRange
    correctAvailable
    (obligations.readyProposalIsProtected time validator proposal latched)

/-- One durable send goal runs within the local action bound. -/
theorem latched_send_goal_runs_within_bound
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
    {time : Time} {validator receiver : Nat}
    {reference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (correctAvailable : faults.correctAvailable validator = true)
    (latched :
      (obligations.trace time validator).sendGoal reference receiver = true) :
    ∃ completion : ValidatorActionCompletion timed.execution
        timed.localActionBound validator (.sendBlock receiver reference) time,
      time ≤ completion.event.completedAt ∧
      completion.event.completedAt ≤ time + timed.localActionBound ∧
      ValidatorLocalActionOccurs
        (timed.execution.events completion.event.completedAt) validator
        (.sendBlock receiver reference) := by
  exact protected_validator_action_completes_within_bound timed validatorInRange
    correctAvailable
    (obligations.sendGoalIsProtected time validator receiver reference latched)

/-- Running a latched proposal creates its durable send goal for one peer. -/
theorem latched_proposal_run_creates_send_goal
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
    {time : Time} {validator receiver : Nat}
    {proposal : ValidatorReadyProposal BlockId}
    (validatorInRange : validator < config.authorityCount)
    (correctAvailable : faults.correctAvailable validator = true)
    (receiverInRange : receiver < config.authorityCount)
    (differentValidator : receiver ≠ validator)
    (latched : (obligations.trace time validator).readyProposal =
      some proposal) :
    ∃ completion : ValidatorActionCompletion timed.execution
        timed.localActionBound validator (.persistProposal proposal.block) time,
      time ≤ completion.event.completedAt ∧
      completion.event.completedAt ≤ time + timed.localActionBound ∧
      (obligations.trace (completion.event.completedAt + 1) validator).sendGoal
        proposal.block.reference receiver = true := by
  rcases latched_proposal_runs_within_bound obligations validatorInRange
      correctAvailable latched with
    ⟨completion, afterStart, withinBound, occurs⟩
  have reflected := obligations.persistActionIsReflected
    completion.event.completedAt validator proposal.block occurs
  have step := obligations.transitionsFollowRules
    completion.event.completedAt validator
  rw [reflected] at step
  have sendGoal := persist_proposal_creates_send_goals step receiverInRange
    differentValidator
  exact ⟨completion, afterStart, withinBound, sendGoal⟩

/-- Running a latched proposal records its exact durable own block. -/
theorem latched_proposal_run_records_own_block
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
    {time : Time} {validator : Nat}
    {proposal : ValidatorReadyProposal BlockId}
    (validatorInRange : validator < config.authorityCount)
    (correctAvailable : faults.correctAvailable validator = true)
    (latched : (obligations.trace time validator).readyProposal =
      some proposal) :
    ∃ completion : ValidatorActionCompletion timed.execution
        timed.localActionBound validator (.persistProposal proposal.block) time,
      time ≤ completion.event.completedAt ∧
      completion.event.completedAt ≤ time + timed.localActionBound ∧
      ((timed.execution.trace
        (completion.event.completedAt + 1)).validatorState validator).ownBlockAt
          proposal.block.reference.round = some proposal.block.reference := by
  rcases latched_proposal_runs_within_bound obligations validatorInRange
      correctAvailable latched with
    ⟨completion, afterStart, withinBound, occurs⟩
  have stored := persist_proposal_occurrence_stores_own_block timed.execution
    occurs
  exact ⟨completion, afterStart, withinBound, stored⟩

/-- Running one latched send action clears its matching local send goal. -/
theorem latched_send_goal_run_clears_goal
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
    {time : Time} {validator receiver : Nat}
    {reference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (correctAvailable : faults.correctAvailable validator = true)
    (latched :
      (obligations.trace time validator).sendGoal reference receiver = true) :
    ∃ completion : ValidatorActionCompletion timed.execution
        timed.localActionBound validator (.sendBlock receiver reference) time,
      time ≤ completion.event.completedAt ∧
      completion.event.completedAt ≤ time + timed.localActionBound ∧
      (obligations.trace (completion.event.completedAt + 1) validator).sendGoal
        reference receiver = false := by
  rcases latched_send_goal_runs_within_bound obligations validatorInRange
      correctAvailable latched with
    ⟨completion, afterStart, withinBound, occurs⟩
  have reflected := obligations.sendActionIsReflected
    completion.event.completedAt validator receiver reference occurs
  have step := obligations.transitionsFollowRules
    completion.event.completedAt validator
  rw [reflected] at step
  have cleared := matching_send_clears_goal step
  exact ⟨completion, afterStart, withinBound, cleared⟩

/-- Running one protected send records the sent round and clears its goal. -/
theorem latched_send_goal_run_records_sent
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
    {time : Time} {validator receiver : Nat}
    {reference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (correctAvailable : faults.correctAvailable validator = true)
    (latched :
      (obligations.trace time validator).sendGoal reference receiver = true) :
    ∃ completion : ValidatorActionCompletion timed.execution
        timed.localActionBound validator (.sendBlock receiver reference) time,
      time ≤ completion.event.completedAt ∧
      completion.event.completedAt ≤ time + timed.localActionBound ∧
      ((timed.execution.trace
        (completion.event.completedAt + 1)).validatorState validator).sentOwnBlockAt
          reference.round = true ∧
      (obligations.trace (completion.event.completedAt + 1) validator).sendGoal
        reference receiver = false := by
  rcases latched_send_goal_runs_within_bound obligations validatorInRange
      correctAvailable latched with
    ⟨completion, afterStart, withinBound, occurs⟩
  have sent := send_block_occurrence_records_sent_own_block timed.execution
    occurs
  have reflected := obligations.sendActionIsReflected
    completion.event.completedAt validator receiver reference occurs
  have step := obligations.transitionsFollowRules
    completion.event.completedAt validator
  rw [reflected] at step
  have cleared := matching_send_clears_goal step
  exact ⟨completion, afterStart, withinBound, sent, cleared⟩

end Mysticeti
