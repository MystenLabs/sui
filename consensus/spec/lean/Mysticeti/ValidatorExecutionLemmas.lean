/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorExecution

namespace Mysticeti

/-- A member of an execution batch has one matching atomic step. -/
theorem validator_world_step_member_has_atomic_step
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {event : ValidatorAtomicEvent BlockId CommitId PacketId}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (member : event ∈ events) :
    ∃ eventBefore eventAfter,
      ValidatorAtomicStep config faults protocolPacket program time eventBefore
        event eventAfter := by
  induction step with
  | nil => simp at member
  | cons firstStep remainingSteps ih =>
      simp only [List.mem_cons] at member
      rcases member with rfl | member
      · exact ⟨_, _, firstStep⟩
      · exact ih member

/-- A local-action atomic step has the structural effect declared for that
action. -/
theorem validator_atomic_local_action_has_structural_effect
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
    ValidatorActionStructuralEffect validator action
      (before.validatorState validator) (after.validatorState validator) := by
  cases step
  assumption

/-- A local action in one execution batch has its declared structural effect at
its atomic transition. -/
theorem validator_local_action_occurrence_has_structural_effect
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
    ∃ eventBefore eventAfter : ValidatorWorldState BlockId CommitId PacketId,
      ValidatorActionStructuralEffect validator action
        (eventBefore.validatorState validator)
        (eventAfter.validatorState validator) := by
  have member :
      ValidatorAtomicEvent.localAction validator action ∈ events := by
    rcases occurs with ⟨eventPrefix, eventSuffix, rfl⟩
    simp
  rcases validator_world_step_member_has_atomic_step step member with
    ⟨eventBefore, eventAfter, atomicStep⟩
  exact ⟨eventBefore, eventAfter,
    validator_atomic_local_action_has_structural_effect atomicStep⟩

namespace ValidatorDurableStateMonotone

variable {BlockId CommitId : Type}
variable {before after : ValidatorLocalState BlockId CommitId}

/-- An installed commit entry persists through a durable transition. -/
theorem installed_commit_persists
    (monotone : ValidatorDurableStateMonotone before after)
    {index : Nat} {commitId : CommitId}
    (installed : before.installedCommitAt index = some commitId) :
    after.installedCommitAt index = some commitId := by
  rcases monotone with
    ⟨_, _, _, installedMonotone, _, _, _, _, _, _, _, _⟩
  exact installedMonotone index commitId installed

/-- A commit-install source persists through a durable transition. -/
theorem install_source_persists
    (monotone : ValidatorDurableStateMonotone before after)
    {index : Nat} {source : CommitInstallSource}
    (recorded : before.commitInstallSourceAt index = some source) :
    after.commitInstallSourceAt index = some source := by
  rcases monotone with
    ⟨_, _, _, _, sourceMonotone, _, _, _, _, _, _, _⟩
  exact sourceMonotone index source recorded

/-- A durable own block persists through a durable transition. -/
theorem own_block_persists
    (monotone : ValidatorDurableStateMonotone before after)
    {round : Nat} {reference : ValidatorBlockRef BlockId}
    (stored : before.ownBlockAt round = some reference) :
    after.ownBlockAt round = some reference := by
  rcases monotone with
    ⟨_, _, _, _, _, _, _, ownBlockMonotone, _, _, _, _⟩
  exact ownBlockMonotone round reference stored

/-- A sent-own-block record persists through a durable transition. -/
theorem sent_own_block_persists
    (monotone : ValidatorDurableStateMonotone before after)
    {round : Nat}
    (sent : before.sentOwnBlockAt round = true) :
    after.sentOwnBlockAt round = true := by
  rcases monotone with
    ⟨_, _, _, _, _, _, _, _, sentMonotone, _, _, _⟩
  exact sentMonotone round sent

/-- An accepted block record persists through a durable transition. -/
theorem accepted_block_persists
    (monotone : ValidatorDurableStateMonotone before after)
    {reference : ValidatorBlockRef BlockId}
    (accepted : before.accepted reference = true) :
    after.accepted reference = true := by
  rcases monotone with
    ⟨_, _, _, _, _, _, _, _, _, acceptedMonotone, _, _⟩
  exact acceptedMonotone reference accepted

/-- An accepted representative persists through a durable transition. -/
theorem accepted_representative_persists
    (monotone : ValidatorDurableStateMonotone before after)
    {round author : Nat} {reference : ValidatorBlockRef BlockId}
    (accepted :
      before.acceptedRepresentative round author = some reference) :
    after.acceptedRepresentative round author = some reference := by
  rcases monotone with
    ⟨_, _, _, _, _, _, _, _, _, _, representativeMonotone, _⟩
  exact representativeMonotone round author reference accepted

end ValidatorDurableStateMonotone

namespace ValidatorExecution

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- Durable local fields persist between any two ordered execution times. -/
theorem durable_fields_persist
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {validator earlier later : Nat}
    (validatorInRange : validator < config.authorityCount)
    (ordered : earlier ≤ later) :
    ValidatorDurableStateMonotone
      ((execution.trace earlier).validatorState validator)
      ((execution.trace later).validatorState validator) :=
  execution.durableStateMonotone validator earlier later validatorInRange ordered

/-- One installed commit remains installed at every later time. -/
theorem installed_commit_persists
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {validator earlier later index : Nat} {commitId : CommitId}
    (validatorInRange : validator < config.authorityCount)
    (ordered : earlier ≤ later)
    (installed :
      ((execution.trace earlier).validatorState validator).installedCommitAt
        index = some commitId) :
    ((execution.trace later).validatorState validator).installedCommitAt index =
      some commitId := by
  exact (execution.durable_fields_persist validatorInRange ordered)
    |>.installed_commit_persists installed

/-- One accepted block remains accepted at every later time. -/
theorem accepted_block_persists
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {validator earlier later : Nat}
    {reference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (ordered : earlier ≤ later)
    (accepted :
      ((execution.trace earlier).validatorState validator).accepted reference =
        true) :
    ((execution.trace later).validatorState validator).accepted reference =
      true := by
  exact (execution.durable_fields_persist validatorInRange ordered)
    |>.accepted_block_persists accepted

end ValidatorExecution

/-- A new durable own-block entry requires proposal persistence. -/
theorem new_own_block_requires_persist_proposal
    {BlockId CommitId : Type}
    {validator round : Nat}
    {reference : ValidatorBlockRef BlockId}
    {action : ValidatorLocalAction BlockId CommitId}
    {before after : ValidatorLocalState BlockId CommitId}
    (effect : ValidatorActionStructuralEffect validator action before after)
    (absent : before.ownBlockAt round = none)
    (added : after.ownBlockAt round = some reference) :
    ∃ block,
      action = .persistProposal block ∧
        block.reference.round = round ∧
        block.reference = reference := by
  rcases effect with ⟨_, _, ownEffect, _, _, _⟩
  cases action with
  | persistProposal block =>
      refine ⟨block, rfl, ?_⟩
      have update := ownEffect.2.1
      by_cases sameRound : round = block.reference.round
      · subst round
        have sameReference : reference = block.reference := by
          have equality : some reference = some block.reference := by
            rw [← added]
            exact update.1
          exact Option.some.inj equality
        exact ⟨rfl, sameReference.symm⟩
      · have unchanged := update.2 round sameRound
        rw [unchanged, absent] at added
        contradiction
  | sendReplayManifest _ _ =>
      simp only [OwnBlockActionEffect] at ownEffect
      rw [ownEffect.1, absent] at added
      contradiction
  | enterRecovery | requestBlock | serveBlock | acceptBlock | sendBlock |
      proposeNormal | proposeNext | alignProposal | runCommitter |
      runReplayCommitter | recordCommit | applySyncedCommit =>
      simp only [OwnBlockActionEffect] at ownEffect
      rw [ownEffect.1, absent] at added
      contradiction

/-- A new sent-own-block record requires a block-send action. -/
theorem new_sent_own_block_requires_send
    {BlockId CommitId : Type}
    {validator round : Nat}
    {action : ValidatorLocalAction BlockId CommitId}
    {before after : ValidatorLocalState BlockId CommitId}
    (effect : ValidatorActionStructuralEffect validator action before after)
    (notSent : before.sentOwnBlockAt round = false)
    (sent : after.sentOwnBlockAt round = true) :
    ∃ receiver reference,
      action = .sendBlock receiver reference ∧ reference.round = round := by
  rcases effect with ⟨_, _, _, sentEffect, _, _⟩
  cases action with
  | sendBlock receiver reference =>
      refine ⟨receiver, reference, rfl, ?_⟩
      by_cases sameRound : round = reference.round
      · exact sameRound.symm
      · have unchanged := sentEffect.2 round sameRound
        rw [unchanged, notSent] at sent
        contradiction
  | sendReplayManifest _ _ =>
      simp only [SentOwnBlockActionEffect] at sentEffect
      rw [sentEffect, notSent] at sent
      contradiction
  | enterRecovery | requestBlock | serveBlock | acceptBlock | persistProposal |
      proposeNormal | proposeNext | alignProposal | runCommitter |
      runReplayCommitter | recordCommit | applySyncedCommit =>
      simp only [SentOwnBlockActionEffect] at sentEffect
      rw [sentEffect, notSent] at sent
      contradiction

/-- A new installed-commit entry requires local commit execution or verified
commit synchronization. -/
theorem new_installed_commit_requires_commit_action
    {BlockId CommitId : Type}
    {validator index : Nat} {commitId : CommitId}
    {action : ValidatorLocalAction BlockId CommitId}
    {before after : ValidatorLocalState BlockId CommitId}
    (effect : ValidatorActionStructuralEffect validator action before after)
    (absent : before.installedCommitAt index = none)
    (added : after.installedCommitAt index = some commitId) :
    (∃ head,
      action = .recordCommit head ∧ head.index = index ∧ head.id = commitId) ∨
    (∃ head,
      action = .applySyncedCommit head ∧ head.index = index ∧
        head.id = commitId) := by
  rcases effect with ⟨_, _, _, _, _, installEffect⟩
  cases action with
  | recordCommit head =>
      left
      refine ⟨head, rfl, ?_⟩
      have update := installEffect.2.2.1
      by_cases sameIndex : index = head.index
      · subst index
        have sameCommit : commitId = head.id := by
          have equality : some commitId = some head.id := by
            rw [← added]
            exact update.1
          exact Option.some.inj equality
        exact ⟨rfl, sameCommit.symm⟩
      · have unchanged := update.2 index sameIndex
        rw [unchanged, absent] at added
        contradiction
  | applySyncedCommit head =>
      right
      refine ⟨head, rfl, ?_⟩
      have update := installEffect.2.2.1
      by_cases sameIndex : index = head.index
      · subst index
        have sameCommit : commitId = head.id := by
          have equality : some commitId = some head.id := by
            rw [← added]
            exact update.1
          exact Option.some.inj equality
        exact ⟨rfl, sameCommit.symm⟩
      · have unchanged := update.2 index sameIndex
        rw [unchanged, absent] at added
        contradiction
  | sendReplayManifest _ _ =>
      simp only [CommitInstallActionEffect] at installEffect
      rw [installEffect.2.1, absent] at added
      contradiction
  | enterRecovery | requestBlock | serveBlock | acceptBlock | persistProposal |
      sendBlock | proposeNormal | proposeNext | alignProposal |
      runCommitter | runReplayCommitter =>
      simp only [CommitInstallActionEffect] at installEffect
      rw [installEffect.2.1, absent] at added
      contradiction

/-- A new commit-install source requires the matching local or synchronized
commit action. -/
theorem new_install_source_requires_commit_action
    {BlockId CommitId : Type}
    {validator index : Nat} {source : CommitInstallSource}
    {action : ValidatorLocalAction BlockId CommitId}
    {before after : ValidatorLocalState BlockId CommitId}
    (effect : ValidatorActionStructuralEffect validator action before after)
    (absent : before.commitInstallSourceAt index = none)
    (added : after.commitInstallSourceAt index = some source) :
    (∃ head,
      action = .recordCommit head ∧ head.index = index ∧
        source = .localExecution) ∨
    (∃ head,
      action = .applySyncedCommit head ∧ head.index = index ∧
        source = .verifiedCommitSync) := by
  rcases effect with ⟨_, _, _, _, _, installEffect⟩
  cases action with
  | recordCommit head =>
      left
      refine ⟨head, rfl, ?_⟩
      have update := installEffect.2.2.2
      by_cases sameIndex : index = head.index
      · subst index
        have sameSource : source = .localExecution := by
          have equality : some source = some CommitInstallSource.localExecution := by
            rw [← added]
            exact update.1
          exact Option.some.inj equality
        exact ⟨rfl, sameSource⟩
      · have unchanged := update.2 index sameIndex
        rw [unchanged, absent] at added
        contradiction
  | applySyncedCommit head =>
      right
      refine ⟨head, rfl, ?_⟩
      have update := installEffect.2.2.2
      by_cases sameIndex : index = head.index
      · subst index
        have sameSource : source = .verifiedCommitSync := by
          have equality :
              some source = some CommitInstallSource.verifiedCommitSync := by
            rw [← added]
            exact update.1
          exact Option.some.inj equality
        exact ⟨rfl, sameSource⟩
      · have unchanged := update.2 index sameIndex
        rw [unchanged, absent] at added
        contradiction
  | sendReplayManifest _ _ =>
      simp only [CommitInstallActionEffect] at installEffect
      rw [installEffect.2.2.1, absent] at added
      contradiction
  | enterRecovery | requestBlock | serveBlock | acceptBlock | persistProposal |
      sendBlock | proposeNormal | proposeNext | alignProposal |
      runCommitter | runReplayCommitter =>
      simp only [CommitInstallActionEffect] at installEffect
      rw [installEffect.2.2.1, absent] at added
      contradiction

/-- A local GC advance implies a local commit-head index advance in the same
batch. -/
theorem gc_round_advance_implies_commit_index_advance
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {time validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (gcAdvanced :
      ((execution.trace time).validatorState validator).gcRound <
        ((execution.trace (time + 1)).validatorState validator).gcRound) :
    ((execution.trace time).validatorState validator).commitHead.index <
      ((execution.trace (time + 1)).validatorState validator).commitHead.index :=
    by
  have monotone := execution.durableStateMonotone validator time (time + 1)
    validatorInRange (Nat.le_add_right time 1)
  have headLe := monotone.1
  by_cases indexEqual :
      ((execution.trace time).validatorState validator).commitHead.index =
        ((execution.trace (time + 1)).validatorState validator).commitHead.index
  · have headEqual :
        ((execution.trace time).validatorState validator).commitHead =
          ((execution.trace (time + 1)).validatorState validator).commitHead :=
      monotone.2.2.1 indexEqual
    have gcAtStart := execution.correctGcRoundMatchesCommitHead time validator
      validatorInRange validatorCorrectAvailable
    have gcAtEnd := execution.correctGcRoundMatchesCommitHead (time + 1)
      validator validatorInRange validatorCorrectAvailable
    have gcEqual :
        ((execution.trace time).validatorState validator).gcRound =
          ((execution.trace (time + 1)).validatorState validator).gcRound := by
      rw [gcAtStart, gcAtEnd, headEqual]
    omega
  · omega

/-- Correct hosts at the same exact commit head use the same GC round. -/
theorem correct_validators_same_head_have_same_gc_round
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {leftTime rightTime leftValidator rightValidator : Time}
    (leftInRange : leftValidator < config.authorityCount)
    (leftCorrectAvailable : faults.correctAvailable leftValidator = true)
    (rightInRange : rightValidator < config.authorityCount)
    (rightCorrectAvailable : faults.correctAvailable rightValidator = true)
    (sameHead :
      ((execution.trace leftTime).validatorState leftValidator).commitHead =
        ((execution.trace rightTime).validatorState rightValidator).commitHead) :
    ((execution.trace leftTime).validatorState leftValidator).gcRound =
      ((execution.trace rightTime).validatorState rightValidator).gcRound := by
  rw [execution.correctGcRoundMatchesCommitHead leftTime leftValidator
      leftInRange leftCorrectAvailable,
    execution.correctGcRoundMatchesCommitHead rightTime rightValidator
      rightInRange rightCorrectAvailable,
    sameHead]

/-- A correct host cannot collect blocks past its installed commit round. -/
theorem correct_validator_gc_round_at_most_commit_round
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {time validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true) :
    ((execution.trace time).validatorState validator).gcRound ≤
      ((execution.trace time).validatorState validator).commitHead.round := by
  rw [execution.correctGcRoundMatchesCommitHead time validator
    validatorInRange validatorCorrectAvailable]
  exact execution.gcRoundForCommitHeadAtMostRound _

/-- Local selection of one accepted representative for stake accounting.

A Byzantine author can have more than one accepted branch. The selected map
keeps one accepted branch. A correct author has only one branch, so its exact
accepted reference is selected.
-/
structure ValidatorAcceptedRepresentativeRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  representativeIsSound : ∀ time observer round author reference,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    ((execution.trace time).validatorState observer).acceptedRepresentative
        round author = some reference →
    reference.author = author ∧
      reference.round = round ∧
      ((execution.trace time).validatorState observer).accepted reference = true
  acceptedCorrectReferenceIsRecorded : ∀ time observer reference,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    reference.author < config.authorityCount →
    faults.byzantine reference.author = false →
    ((execution.trace time).validatorState observer).accepted reference = true →
    ((execution.trace time).validatorState observer).acceptedRepresentative
      reference.round reference.author = some reference
  acceptedReferenceHasRepresentative : ∀ time observer reference,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    reference.author < config.authorityCount →
    ((execution.trace time).validatorState observer).accepted reference = true →
    ∃ selected,
      ((execution.trace time).validatorState observer).acceptedRepresentative
          reference.round reference.author = some selected ∧
        selected.author = reference.author ∧
        selected.round = reference.round ∧
        ((execution.trace time).validatorState observer).accepted selected = true

end Mysticeti
