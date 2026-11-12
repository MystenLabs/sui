/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Lean.Elab.Tactic.Omega
import Mysticeti.ValidatorPointwiseFixedReferenceCatchUp

namespace Mysticeti

/-!
Final pointwise composition for the fixed-reference ordinary-DAG route.

The only lower result used by this file is receiver-local. For a correct
receiver whose current head is one exact prior reference, the protocol either
advances that receiver or derives its local `depth + 1` fixed-reference direct
range. This result must be proved from current and past protocol source maps.
It is not a field of `EndToEndLivenessInputs`.

One correct receiver first exposes the exact next durable prefix entry. Every
other correct receiver is then already at that entry or has the exact prior
head. In the second case, its own receiver-local progress produces a later
local Flex commit. Exact-prefix safety identifies the already known next
entry. No certificate transport, replay manifest, future range, or global
commit-silence premise occurs in this composition.
-/

/-- The internal receiver-local progress theorem needed by the final
composition.

This proposition is a derived execution result. It must not be added to the
end-to-end input structure or supplied as a future window premise. -/
def DerivedReceiverFixedReferenceProgress
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network) : Prop :=
  ∀ start receiver prior,
    network.gst ≤ start →
    (∀ time, start ≤ time →
      (inputs.timedExecution.execution.trace time).epochActive = true) →
    AllCorrectAvailableInstalledExactAt faults
      inputs.timedExecution.execution.trace start prior →
    receiver < config.authorityCount →
    faults.correctAvailable receiver = true →
    ((inputs.timedExecution.execution.trace start).validatorState
      receiver).commitHead = prior →
    ValidatorReceiverCommitAdvance inputs.timedExecution start receiver ∨
      Nonempty (ValidatorFixedReferenceFavorableDirectRange
        inputs.timedExecution start receiver
          (inputs.leaderSchedule.indirectDepth + 1) prior)

/-- A common exact installed prefix persists to a later trace time. -/
private theorem fixed_reference_installed_exact_persists
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {earlier later : Time} {reference : ValidatorCommitHead CommitId}
    (installed : AllCorrectAvailableInstalledExactAt faults execution.trace
      earlier reference)
    (ordered : earlier ≤ later) :
    AllCorrectAvailableInstalledExactAt faults execution.trace later
      reference := by
  intro validator validatorInRange validatorCorrect
  have earlierInstalled := installed validator validatorInRange validatorCorrect
  have laterInstalled := execution.installed_commit_persists validatorInRange
    ordered earlierInstalled.1
  have withinHead :=
    (execution.statesWellFormed later validator validatorInRange)
      |>.installedIndexIsNotFuture reference.index reference.id laterInstalled
  exact ⟨laterInstalled, withinHead⟩

/-- Receiver-local fixed-reference progress proves one exact common commit
step.

The first correct receiver that moves exposes the exact next entry. At that
later time, each correct receiver is either already at the next index or has
the exact prior head. Only the latter receiver calls the local progress
theorem again. Other validators' installs do not interrupt this call. -/
theorem derived_receiver_fixed_reference_progress_proves_common_commit_step
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network)
    (progress : DerivedReceiverFixedReferenceProgress inputs) :
    DerivedPointwiseCommonCommitStep faults network
      inputs.timedExecution.execution.trace := by
  intro start prior afterGst active priorInstalled
  have liveWeightPositive : 0 < weight config.authorityCount config.stake
      faults.correctAvailable :=
    Nat.lt_of_lt_of_le config.thresholds.quorum_positive
      faults.correct_available_stake_is_quorum
  rcases positive_weight_has_member liveWeightPositive with
    ⟨seed, seedInRange, seedCorrect, _seedStake⟩
  have firstAhead : ∃ witnessAt,
      start ≤ witnessAt ∧
        Nonempty (ValidatorAheadExactNext inputs.exactCommitPrefix witnessAt
          prior) := by
    by_cases aheadAtStart : ∃ validator,
        validator < config.authorityCount ∧
          faults.correctAvailable validator = true ∧
          prior.index + 1 ≤
            (inputs.timedExecution.execution.trace start).localCommitIndex
              validator
    · exact ⟨start, Nat.le_refl _,
        correct_ahead_index_gives_exact_next inputs.exactCommitPrefix
          inputs.exactCommitInstallProvenance aheadAtStart⟩
    · rcases correct_available_installed_prior_is_ahead_or_exact_head
          inputs.commitPrefix priorInstalled seedInRange seedCorrect with
        seedAhead | seedHead
      · exact False.elim
          (aheadAtStart ⟨seed, seedInRange, seedCorrect, seedAhead⟩)
      · exact receiver_progress_eventually_gives_exact_next inputs seedInRange
          seedCorrect seedHead
            (progress start seed prior afterGst active priorInstalled
              seedInRange seedCorrect seedHead)
  rcases firstAhead with ⟨witnessAt, startBeforeWitness, aheadRaw⟩
  let ahead := Classical.choice aheadRaw
  have priorInstalledAtWitness : AllCorrectAvailableInstalledExactAt faults
      inputs.timedExecution.execution.trace witnessAt prior :=
    fixed_reference_installed_exact_persists priorInstalled startBeforeWitness
  have afterGstAtWitness : network.gst ≤ witnessAt :=
    Nat.le_trans afterGst startBeforeWitness
  have activeAtWitness : ∀ time, witnessAt ≤ time →
      (inputs.timedExecution.execution.trace time).epochActive = true :=
    active_suffix_of_later_start startBeforeWitness active
  have completesAtWitness : EveryCorrectAvailableValidatorCompletesReference
      faults inputs.timedExecution.execution.trace witnessAt ahead.next := by
    intro receiver receiverInRange receiverCorrect
    rcases correct_available_installed_prior_is_ahead_or_exact_head
        inputs.commitPrefix priorInstalledAtWitness receiverInRange
          receiverCorrect with receiverAhead | receiverHead
    · have nextAtOrBelow : ahead.next.index ≤
          (inputs.timedExecution.execution.trace witnessAt).localCommitIndex
            receiver := by
        rw [ahead.nextIndex]
        exact receiverAhead
      exact exact_prefix_entry_at_or_below_head_gives_completion
        inputs.exactCommitPrefix inputs.authenticatedFlexVotes
          inputs.exactCommitInstallProvenance (Nat.le_refl witnessAt)
            ahead.holderInRange ahead.holderCorrect ahead.installed
              receiverInRange receiverCorrect (by
                rw [ahead.nextIndex]
                omega) nextAtOrBelow
    · exact ahead_exact_next_and_receiver_progress_give_completion inputs ahead
        receiverInRange receiverCorrect receiverHead
          (priorInstalledAtWitness receiver receiverInRange receiverCorrect).1
          (progress witnessAt receiver prior afterGstAtWitness activeAtWitness
            priorInstalledAtWitness receiverInRange receiverCorrect
              receiverHead)
  have completesFromStart := per_validator_completions_rebase_start
    startBeforeWitness completesAtWitness
  exact per_validator_completions_give_pointwise_result ahead.nextIndex
    completesFromStart

/-- The receiver-local progress theorem and finite exact-prefix induction give
unbounded network commit progress. -/
theorem derived_receiver_fixed_reference_progress_proves_network_commit_progress
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network)
    (progress : DerivedReceiverFixedReferenceProgress inputs) :
    NetworkCommitProgressLiveness config faults network
      inputs.timedExecution.execution.trace := by
  exact derived_common_commit_step_proves_network_commit_progress
    inputs.timedExecution.execution
      (derived_receiver_fixed_reference_progress_proves_common_commit_step
        inputs progress)
      inputs.genesis inputs.initial.genesisIndex
        inputs.initial.installedAtCorrectValidator

/-- The same finite induction gives pointwise exact-prefix catch-up. -/
theorem derived_receiver_fixed_reference_progress_proves_pointwise_catch_up
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network)
    (progress : DerivedReceiverFixedReferenceProgress inputs) :
    PointwiseCommitCatchUpLiveness config faults network
      inputs.timedExecution.execution.trace := by
  exact derived_common_commit_step_proves_pointwise_commit_catch_up
    inputs.timedExecution.execution
      (derived_receiver_fixed_reference_progress_proves_common_commit_step
        inputs progress)
      inputs.genesis inputs.initial.genesisIndex
        inputs.initial.installedAtCorrectValidator

/-- Deterministic end-to-end composition after the receiver-local progress
theorem is derived from the ordinary-DAG and pacing rules. -/
theorem derived_receiver_fixed_reference_progress_proves_end_to_end_goal
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network)
    (progress : DerivedReceiverFixedReferenceProgress inputs) :
    EndToEndLivenessGoal inputs := by
  exact network_dag_progress_and_derived_common_commit_step_prove_end_to_end_goal
    inputs (EndToEndLivenessInputs.network_dag_progress inputs)
      (derived_receiver_fixed_reference_progress_proves_common_commit_step
        inputs progress)

end Mysticeti
