/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.EndToEndLiveness

namespace Mysticeti

/-! Align one derived favorable leader window with actual fresh productions.

The probability layer derives a favorable first-slot path after every future
round. The deterministic recovery layer can extend one current recovery source
by any finite number of rounds. This module selects the favorable base only
after it has derived the current common recovery source. It then extends that
source to exactly two rounds before the favorable base.

No future proposal, block, layer, carrier, FlexCommitter run, install, or
commit is an input to this construction.
-/

/-- One fresh recovery window whose first actual production round is also the
first round of one favorable first-slot window.

The raw fresh-window base is two rounds before the first leader round. The two
extra fresh rounds after the leader range provide the vote-to-carrier suffix
used by `ValidatorCollectiveRecoveryCarrier`. -/
structure ValidatorAlignedFavorableFreshWindow
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
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (prior : ValidatorCommitHead CommitId)
    (observation windowBase leaderCount : Nat) where
  window : ValidatorFreshTimerPacedExactRoundWindow timed obligations waits
    observation windowBase (leaderCount + 2)
  leaderAuthorAt : Nat → Nat
  leaderInRange : ∀ offset,
    offset < leaderCount →
      leaderAuthorAt offset < config.authorityCount
  leaderCorrect : ∀ offset,
    offset < leaderCount →
      faults.correctAvailable (leaderAuthorAt offset) = true
  firstSelected : ∀ offset,
    offset < leaderCount →
      (config.selectedLeaderOrder prior.id
        (windowBase + 2 + offset)).head? = some (leaderAuthorAt offset)

/-- Current recovery inputs and one already-derived favorable path produce an
actual fresh window at exactly the favorable rounds.

`lateFirstRound` can be the threshold from the same-head adjacent timing
theorem. The result starts its favorable leader range at or after that
threshold. `path` is an internal consequence of the sampled ranking event; it
is not an end-to-end input field. -/
theorem recovery_inputs_and_favorable_path_give_aligned_fresh_window
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network)
    {start firstFutureRound lateFirstRound : Nat}
    {prior : ValidatorCommitHead CommitId}
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (inputs.timedExecution.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance
      inputs.timedExecution start)
    (path : CommitHeadFirstSlotLeaderPathCoverageAfter config faults
      inputs.leaderSchedule.indirectDepth prior.id firstFutureRound) :
    ∃ observation windowBase,
      start ≤ observation ∧
        lateFirstRound ≤ windowBase + 2 ∧
        Nonempty (ValidatorAlignedFavorableFreshWindow
          inputs.timedExecution inputs.proposalObligations inputs.recoveryWait
            prior observation windowBase
              (inputs.leaderSchedule.indirectDepth + 1)) := by
  let timed := inputs.timedExecution
  rcases no_commit_advance_gives_active_recovery_snapshot
      inputs.recoveryMode afterGst active noAdvance with
    ⟨snapshot, startBeforeSnapshot, recovery⟩
  have activeFromSnapshot : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true := by
    intro time snapshotBeforeTime
    exact active time (Nat.le_trans startBeforeSnapshot snapshotBeforeTime)
  have noAdvanceAtSnapshot :
      ¬SomeCorrectAvailableCommitAdvance timed snapshot :=
    no_commit_advance_persists_to_later_start startBeforeSnapshot noAdvance
  have blockRecovery : ValidatorActiveBlockProgressRecoverySnapshot
      inputs.blockProgressRecoveryMode (snapshot + 1) :=
    active_stall_snapshot_latches_block_progress_recovery inputs.recoveryMode
      inputs.blockProgressRecoveryMode
        inputs.blockProgressRecoveryWaitMatches recovery activeFromSnapshot
          noAdvanceAtSnapshot
  have recoveryAtNext : ValidatorActiveRecoverySnapshot timed
      inputs.recoveryMode.recoveryWait (snapshot + 1) :=
    { afterGst := Nat.le_trans recovery.afterGst (Nat.le_succ snapshot)
      recovering := by
        intro validator validatorInRange validatorCorrectAvailable
        exact active_recovery_snapshot_persists_without_commit_advance
          inputs.recoveryMode recovery validatorInRange
            validatorCorrectAvailable (Nat.le_succ snapshot)
              activeFromSnapshot noAdvanceAtSnapshot }
  have activeFromNext : ∀ time, snapshot + 1 ≤ time →
      (timed.execution.trace time).epochActive = true := by
    intro time nextBeforeTime
    exact activeFromSnapshot time
      (Nat.le_trans (Nat.le_succ snapshot) nextBeforeTime)
  have noAdvanceAtNext :
      ¬SomeCorrectAvailableCommitAdvance timed (snapshot + 1) :=
    no_commit_advance_persists_to_later_start (Nat.le_succ snapshot)
      noAdvanceAtSnapshot
  let localSources :=
    active_block_progress_recovery_gives_local_base_source_family
      inputs.recoveryMode inputs.recoveryProposalRounds
        inputs.recoveryTimerSource inputs.recoveryProposalPacing
          inputs.recoveryTimerArms inputs.proposalLatch
            inputs.executionEffects.effects inputs.recoverySourcePins
              inputs.genesisParents inputs.installedHeadBootstrap
                inputs.recoveryParentNeeds inputs.acceptedRepresentatives
                  inputs.blockProgressRecoveryNeedRules
                    inputs.authorityCountAtLeastTwo recoveryAtNext blockRecovery
                      activeFromNext noAdvanceAtNext
  let initialBase := correctValidatorRecoveryBaseMaximumUpTo faults
    (timed.execution.trace (snapshot + 1)) config.authorityCount
  have commonSources :
      EveryCorrectAvailableValidatorExactRecoveryRoundSource
        (obligations := inputs.proposalObligations)
          inputs.recoverySourcePins inputs.recoveryMode.recoveryWait
            (snapshot + 1) initialBase := by
    simpa [initialBase] using
      (local_base_source_family_gives_common_base_source_family
        inputs.recoveryMode inputs.recoveryProposalRounds
          inputs.recoveryTimerSource inputs.recoveryProposalPacing
            inputs.recoveryTimerArms inputs.proposalLatch
              inputs.executionEffects.effects inputs.recoverySourcePins
                inputs.recoveryTipRebroadcast inputs.recoveryCapsuleSync
                  inputs.recoveryParentAcceptance
                    inputs.authorityCountAtLeastTwo recoveryAtNext localSources
                      activeFromNext noAdvanceAtNext)
  let requestedStart :=
    max firstFutureRound (max lateFirstRound (initialBase + 3))
  have firstFutureBeforeRequested : firstFutureRound ≤ requestedStart := by
    exact Nat.le_max_left _ _
  rcases path requestedStart firstFutureBeforeRequested with
    ⟨favorableBase, requestedBeforeFavorable, favorable⟩
  have lateBeforeFavorable : lateFirstRound ≤ favorableBase := by
    exact Nat.le_trans
      (Nat.le_trans (Nat.le_max_left _ _) (Nat.le_max_right _ _))
      requestedBeforeFavorable
  have initialBeforeFavorable : initialBase + 3 ≤ favorableBase := by
    exact Nat.le_trans
      (Nat.le_trans (Nat.le_max_right _ _) (Nat.le_max_right _ _))
      requestedBeforeFavorable
  let prefixCount := favorableBase - (initialBase + 3)
  have alignedBase : initialBase + prefixCount + 1 + 2 = favorableBase := by
    dsimp [prefixCount]
    omega
  let sourceWindow := Classical.choice
    (exact_recovery_round_source_family_gives_finite_window
      inputs.recoveryMode inputs.recoveryProposalRounds
        inputs.recoveryTimerSource inputs.recoveryProposalPacing
          inputs.recoveryTimerArms inputs.proposalLatch
            inputs.executionEffects.effects inputs.recoverySourcePins
              inputs.recoveryTipRebroadcast inputs.recoveryCapsuleSync
                inputs.recoveryParentAcceptance inputs.acceptedRepresentatives
                  inputs.authorityCountAtLeastTwo (count := prefixCount)
                    recoveryAtNext (Nat.le_refl initialBase) commonSources
                      activeFromNext noAdvanceAtNext)
  let rich := Classical.choice
    (exact_recovery_source_window_gives_fresh_timer_paced_suffix
      inputs.recoveryMode inputs.blockProgressRecoveryWaitMatches
        inputs.recoveryProposalRounds inputs.blockProgressProposalOrigin
          inputs.recoveryProposalActionTiming inputs.recoveryProposalPacing
            inputs.recoveryTimerArms inputs.proposalLatch
              inputs.executionEffects.effects inputs.recoverySourcePins
                inputs.recoveryCapsuleSync inputs.recoveryParentAcceptance
                  inputs.acceptedRepresentatives
                    inputs.authorityCountAtLeastTwo recoveryAtNext blockRecovery
                      (Nat.le_refl initialBase) sourceWindow activeFromNext
                        noAdvanceAtNext
                          (freshCount :=
                            inputs.leaderSchedule.indirectDepth + 3))
  let windowBase := initialBase + prefixCount + 1
  let leaderAuthorAt := fun offset =>
    if offsetInRange :
        offset < inputs.leaderSchedule.indirectDepth + 1 then
      Classical.choose (favorable offset offsetInRange)
    else
      0
  have leaderEvidence : ∀ offset,
      offset < inputs.leaderSchedule.indirectDepth + 1 →
        leaderAuthorAt offset < config.authorityCount ∧
          (config.selectedLeaderOrder prior.id
            (windowBase + 2 + offset)).head? =
              some (leaderAuthorAt offset) ∧
          faults.correctAvailable (leaderAuthorAt offset) = true := by
    intro offset offsetInRange
    have evidence := Classical.choose_spec (favorable offset offsetInRange)
    simpa only [leaderAuthorAt, dif_pos offsetInRange, windowBase,
      alignedBase] using evidence
  refine ⟨snapshot + 1, windowBase,
    Nat.le_trans startBeforeSnapshot (Nat.le_succ snapshot), ?_, ⟨?_⟩⟩
  · simpa only [windowBase, alignedBase] using lateBeforeFavorable
  · refine {
      window := ?_
      leaderAuthorAt := leaderAuthorAt
      leaderInRange := ?_
      leaderCorrect := ?_
      firstSelected := ?_ }
    · simpa [windowBase, Nat.add_assoc] using rich
    · intro offset offsetInRange
      exact (leaderEvidence offset offsetInRange).1
    · intro offset offsetInRange
      exact (leaderEvidence offset offsetInRange).2.2
    · intro offset offsetInRange
      exact (leaderEvidence offset offsetInRange).2.1

end Mysticeti
