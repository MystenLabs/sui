/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorCoreProposalContinuation
import Mysticeti.ValidatorExactNextRecovery

namespace Mysticeti

/-! Author-local proposal scheduling across one local commit handler.

Core runs one handler and one `try_propose` callback serially. A commit cannot
interrupt a callback after it starts. Therefore the commit-interference boundary
uses only work which is scheduled before the callback starts:

* one exact protected normal callback with no latched proposal; or
* one armed exact recovery timer with no latched proposal; or
* one occupied protected recovery timer-arm goal with no latched proposal.

At the first same-host install after this scheduled work, the callback has
already persisted its exact proposal, or the post-install state contains one
legal protected normal callback. The latter case covers normal scheduling,
recovery exit, and GC safe resume. It does not create a fresh head-keyed
recovery timer and cannot recurse through another commit race.

The execution trace starts after `recover_validator`; startup recovery is not
an event source inside this running-Core suffix. The local refinement contains
only past actions and current scheduled work. Bounded execution supplies the
later proposal and broadcast in a separate theorem.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- Current protected normal work after one local commit.

`frontierRound` is the exact immediate-parent round of the protected proposal.
The two strict bounds say that these parents are later than the committed
leader and remain above local GC. The protected action's program guard supplies
the accepted, retained, quorum parent-list facts. -/
structure ValidatorAuthorLocalNormalContinuationAt
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Time) where
  proposal : ValidatorNormalProposalAt timed time validator
  frontierRound : Nat
  frontierImmediatelyPrecedesTarget :
    frontierRound + 1 = proposal.targetRound
  committedLeaderBeforeFrontier :
    ((timed.execution.trace time).validatorState
      validator).commitHead.round < frontierRound
  frontierAboveGc :
    ((timed.execution.trace time).validatorState validator).gcRound <
      frontierRound

/-- One exact proposal callback is scheduled but has not started.

The normal branch names its protected exact action. The recovery branches name
one armed exact timer or one occupied protected timer-arm goal. In all branches,
the absence of a latched proposal excludes callbacks which already ran their
proposal action. Raw parent readiness is not a scheduled callback. -/
inductive ValidatorAuthorLocalPreAttemptScheduleAt
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (obligations : ValidatorProposalObligationExecution timed)
    (time validator : Time) : Type where
  | normal
      (proposal : ValidatorNormalProposalAt timed time validator)
      (notLatched : (obligations.trace time validator).readyProposal = none) :
      ValidatorAuthorLocalPreAttemptScheduleAt arms obligations time validator
  | recovery
      (timer : ValidatorArmedExactRecoveryTimerAt timed time validator)
      (notLatched : (obligations.trace time validator).readyProposal = none) :
      ValidatorAuthorLocalPreAttemptScheduleAt arms obligations time validator
  | recoveryArmPending
      (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId)
      (pending : (arms.trace time validator).pending = some goal)
      (notLatched : (obligations.trace time validator).readyProposal = none) :
      ValidatorAuthorLocalPreAttemptScheduleAt arms obligations time validator

/-- The exact round selected by one scheduled callback. -/
def ValidatorAuthorLocalPreAttemptScheduleAt.targetRound
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {obligations : ValidatorProposalObligationExecution timed}
    {time validator : Time}
    (schedule : ValidatorAuthorLocalPreAttemptScheduleAt arms obligations time
      validator) : Nat :=
  match schedule with
  | .normal proposal _ => proposal.targetRound
  | .recovery _ _ =>
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1
  | .recoveryArmPending goal _ _ => goal.targetRound

/-- One legal normal callback is scheduled after the install handler.

The mode guard permits normal persistence only after recovery exits or while
the signer floor is at the GC safe-resume boundary. -/
structure ValidatorAuthorLocalPostInstallNormalScheduleAt
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds)
    (scheduledTarget : Nat) (time validator : Time) where
  work : ValidatorAuthorLocalNormalContinuationAt timed time validator
  targetNotReset : scheduledTarget ≤ work.proposal.targetRound
  targetPreservedOrGcObsolete :
    work.proposal.targetRound = scheduledTarget ∨
      scheduledTarget ≤
        ((timed.execution.trace time).validatorState validator).gcRound + 1
  recoveryInactiveOrSafeResume :
    ¬ValidatorBlockProgressRecoveryModeAt mode time validator ∨
      ((timed.execution.trace time).validatorState
          validator).highestSignedRound ≤
        ((timed.execution.trace time).validatorState validator).gcRound
  notLatched : (obligations.trace time validator).readyProposal = none

/-- Past-or-current disposition of a scheduled callback at the first
same-host install after `start`.

The persistence branch names the exact scheduled round and an action which
already occurred before or in the install batch. The continuation branch names
only a legal normal callback which is scheduled after that atomic handler. -/
inductive ValidatorAuthorLocalFirstInstallScheduleDispositionAt
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {obligations : ValidatorProposalObligationExecution timed}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds)
    {start validator : Time}
    (schedule : ValidatorAuthorLocalPreAttemptScheduleAt arms obligations start
      validator)
    (installTime : Time) : Prop where
  | persisted (persistTime : Time) (block : ValidatorBlock BlockId) :
      start ≤ persistTime →
      persistTime ≤ installTime →
      block.reference.author = validator →
      block.reference.round = schedule.targetRound →
      ValidatorLocalActionOccurs (timed.execution.events persistTime) validator
        (.persistProposal block) →
      ValidatorAuthorLocalFirstInstallScheduleDispositionAt mode schedule
        installTime
  | continues
      (continuation : ValidatorAuthorLocalPostInstallNormalScheduleAt
        (obligations := obligations) mode schedule.targetRound
          (installTime + 1) validator) :
      ValidatorAuthorLocalFirstInstallScheduleDispositionAt mode schedule
        installTime

/-- One-host commit-to-proposal non-interference.

The premise is the first same-host commit-install batch after one exact
pre-attempt schedule. The install can be local execution or verified commit
synchronization, but it is interference only. Core serialization has no
in-progress callback state: the exact scheduled proposal already persisted, or
the post-install state schedules one legal normal callback. An install by
another validator is outside this rule. -/
structure ValidatorAuthorLocalCommitContinuationRules
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    {obligations : ValidatorProposalObligationExecution timed}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds) : Prop
    where
  firstInstallDisposesScheduledAttempt : ∀ start installTime validator head,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    start ≤ installTime →
    (timed.execution.trace (installTime + 1)).epochActive = true →
    (∀ earlier candidate,
      start ≤ earlier → earlier < installTime →
      ¬ValidatorCommitInstallOccurs (timed.execution.events earlier) validator
        candidate) →
    ValidatorCommitInstallOccurs (timed.execution.events installTime) validator
      head →
    ∀ schedule : ValidatorAuthorLocalPreAttemptScheduleAt arms obligations start
      validator,
      ValidatorAuthorLocalFirstInstallScheduleDispositionAt mode schedule
        installTime

/-- The local-record specialization of the caller-independent first-install
rule. A no-sync interval uses this theorem after selecting its first exact
`recordCommit` action. -/
theorem ValidatorAuthorLocalCommitContinuationRules.firstRecordDisposesScheduledAttempt
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {obligations : ValidatorProposalObligationExecution timed}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (rules : ValidatorAuthorLocalCommitContinuationRules
      (obligations := obligations) arms mode)
    {start recordTime validator : Time}
    {head : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ordered : start ≤ recordTime)
    (activeAfter :
      (timed.execution.trace (recordTime + 1)).epochActive = true)
    (noEarlierInstall : ∀ earlier candidate,
      start ≤ earlier → earlier < recordTime →
      ¬ValidatorCommitInstallOccurs (timed.execution.events earlier) validator
        candidate)
    (recorded : ValidatorLocalActionOccurs (timed.execution.events recordTime)
      validator (.recordCommit head))
    (schedule : ValidatorAuthorLocalPreAttemptScheduleAt arms obligations start
      validator) :
    ValidatorAuthorLocalFirstInstallScheduleDispositionAt mode schedule
      recordTime := by
  exact rules.firstInstallDisposesScheduledAttempt start recordTime validator head
    validatorInRange validatorCorrectAvailable ordered activeAfter
      noEarlierInstall (Or.inl recorded) schedule

/-- The protected action keeps the concrete accepted and retained quorum
frontier named by the normal continuation. -/
theorem author_local_normal_continuation_has_ready_frontier
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {time validator : Time}
    (work : ValidatorAuthorLocalNormalContinuationAt timed time validator) :
    ValidatorProposalParentListReady .normal config
      ((timed.execution.trace time).validatorState validator)
      work.proposal.targetRound work.proposal.parents := by
  have enabled := timed.protectedActionIsEnabled time validator
    (.proposeNormal work.proposal.targetRound work.proposal.parents)
      work.proposal.actionProtected
  simpa [ValidatorProposalParentListReady, BasicValidatorActionGuard] using
    enabled.2.1.2

/-- The protected normal target is above the signer floor, the commit voting
frontier, and the new GC safe-resume boundary. -/
theorem author_local_normal_continuation_has_safe_target
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {time validator : Time}
    (work : ValidatorAuthorLocalNormalContinuationAt timed time validator) :
    Nat.max
        (((timed.execution.trace time).validatorState
          validator).highestSignedRound + 1)
        (Nat.max
          (((timed.execution.trace time).validatorState
            validator).commitHead.round + 2)
          (((timed.execution.trace time).validatorState
            validator).gcRound + 2)) ≤
      work.proposal.targetRound := by
  have enabled := timed.protectedActionIsEnabled time validator
    (.proposeNormal work.proposal.targetRound work.proposal.parents)
      work.proposal.actionProtected
  have floorBelow :
      ((timed.execution.trace time).validatorState
          validator).highestSignedRound < work.proposal.targetRound := by
    simpa [BasicValidatorActionGuard] using enabled.2.1.1
  have leaderBound :
      ((timed.execution.trace time).validatorState
          validator).commitHead.round + 2 ≤ work.proposal.targetRound := by
    have leaderBefore := work.committedLeaderBeforeFrontier
    rw [← work.frontierImmediatelyPrecedesTarget]
    omega
  have gcBound :
      ((timed.execution.trace time).validatorState validator).gcRound + 2 ≤
        work.proposal.targetRound := by
    have gcBefore := work.frontierAboveGc
    rw [← work.frontierImmediatelyPrecedesTarget]
    omega
  exact Nat.max_le.2 ⟨Nat.succ_le_iff.mpr floorBelow,
    Nat.max_le.2 ⟨leaderBound, gcBound⟩⟩

/-- A local action by another validator does not mutate this validator's local
state in that atomic step. In particular, another validator's commit record
cannot end this validator's proposal-continuation branch. -/
theorem other_validator_local_action_preserves_validator_state
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {actor validator : Nat}
    {action : ValidatorLocalAction BlockId CommitId}
    (step : ValidatorAtomicStep config faults protocolPacket program time before
      (.localAction actor action) after)
    (different : validator ≠ actor) :
    after.validatorState validator = before.validatorState validator := by
  cases step with
  | localAction _ _ _ _ _ otherUnchanged _ _ =>
      exact otherUnchanged validator different

/-- A receiver-local commit-index increase has a first same-author
commit-install batch. No earlier local commit install occurs after `start`.

This theorem only selects an action from the finite past interval. It does not
use either install kind as liveness progress. -/
theorem commit_index_advance_has_first_install_before
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start finish validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (ordered : start ≤ finish)
    (advanced :
      ((timed.execution.trace start).validatorState
          validator).commitHead.index <
        ((timed.execution.trace finish).validatorState
          validator).commitHead.index) :
    ∃ installTime head,
      start ≤ installTime ∧ installTime < finish ∧
        ValidatorCommitInstallOccurs (timed.execution.events installTime)
          validator head ∧
        ∀ earlier candidate,
          start ≤ earlier → earlier < installTime →
          ¬ValidatorCommitInstallOccurs (timed.execution.events earlier)
            validator candidate := by
  classical
  let installAt : Time → Prop := fun time =>
    ∃ head,
      start ≤ time ∧ time < finish ∧
        ValidatorCommitInstallOccurs (timed.execution.events time) validator
          head
  have boundedMinimum : ∀ bound,
      (∃ time, time ≤ bound ∧ installAt time) →
        ∃ first,
          first ≤ bound ∧ installAt first ∧
            ∀ earlier, earlier < first → ¬installAt earlier := by
    intro bound
    induction bound with
    | zero =>
        intro existsAtOrBefore
        rcases existsAtOrBefore with ⟨time, timeAtMostZero, atTime⟩
        have timeIsZero : time = 0 := Nat.eq_zero_of_le_zero timeAtMostZero
        subst time
        exact ⟨0, Nat.le_refl _, atTime, by
          intro earlier earlierBeforeZero
          exact False.elim (Nat.not_lt_zero earlier earlierBeforeZero)⟩
    | succ previous inductionHypothesis =>
        intro existsAtOrBefore
        by_cases existsBefore : ∃ time, time ≤ previous ∧ installAt time
        · rcases inductionHypothesis existsBefore with
            ⟨first, firstAtMostPrevious, atFirst, firstIsMinimum⟩
          exact ⟨first, Nat.le_trans firstAtMostPrevious (Nat.le_succ _),
            atFirst, firstIsMinimum⟩
        · rcases existsAtOrBefore with ⟨time, timeAtMostNext, atTime⟩
          have notTimeAtMostPrevious : ¬time ≤ previous := by
            intro timeAtMostPrevious
            exact existsBefore ⟨time, timeAtMostPrevious, atTime⟩
          have previousBeforeTime : previous < time :=
            Nat.lt_of_not_ge notTimeAtMostPrevious
          have timeIsNext : time = previous + 1 :=
            Nat.le_antisymm
              (by simpa [Nat.succ_eq_add_one] using timeAtMostNext)
              (Nat.succ_le_iff.mpr previousBeforeTime)
          subst time
          refine ⟨previous + 1, by simp, atTime, ?_⟩
          intro earlier earlierBeforeNext atEarlier
          apply existsBefore
          have earlierAtMostPrevious : earlier ≤ previous :=
            Nat.lt_succ_iff.mp (by
              simpa [Nat.succ_eq_add_one] using earlierBeforeNext)
          exact ⟨earlier, earlierAtMostPrevious, atEarlier⟩
  rcases ValidatorCoreProposalAttemptContinuationRules.commit_index_advance_has_install_before
      validatorInRange ordered advanced with
    ⟨someTime, someHead, startBeforeSome, someBeforeFinish, someInstalled⟩
  have someInstall : installAt someTime :=
    ⟨someHead, startBeforeSome, someBeforeFinish, someInstalled⟩
  rcases boundedMinimum someTime ⟨someTime, Nat.le_refl _, someInstall⟩ with
    ⟨installTime, _installAtMostSome, firstInstall, noEarlierInstall⟩
  rcases firstInstall with
    ⟨head, startBeforeInstall, installBeforeFinish, installed⟩
  refine ⟨installTime, head, startBeforeInstall, installBeforeFinish, installed,
    ?_⟩
  intro earlier candidate startBeforeEarlier earlierBeforeInstall installed
  have earlierIsInstall : installAt earlier :=
    ⟨candidate, startBeforeEarlier,
      Nat.lt_trans earlierBeforeInstall installBeforeFinish, installed⟩
  exact noEarlierInstall earlier earlierBeforeInstall earlierIsInstall

/-- Without synchronized commits, the first install selected above is an exact
same-author `recordCommit`. -/
theorem commit_index_advance_without_sync_has_first_record_commit_before
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start finish validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (ordered : start ≤ finish)
    (advanced :
      ((timed.execution.trace start).validatorState
          validator).commitHead.index <
        ((timed.execution.trace finish).validatorState
          validator).commitHead.index)
    (noSyncedCommit : ∀ time head,
      start ≤ time → time < finish →
      ¬ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.applySyncedCommit head)) :
    ∃ recordTime head,
      start ≤ recordTime ∧ recordTime < finish ∧
        ValidatorLocalActionOccurs (timed.execution.events recordTime)
          validator (.recordCommit head) ∧
        ∀ earlier candidate,
          start ≤ earlier → earlier < recordTime →
          ¬ValidatorCommitInstallOccurs (timed.execution.events earlier)
            validator candidate := by
  rcases commit_index_advance_has_first_install_before validatorInRange ordered
      advanced with
    ⟨installTime, head, startBeforeInstall, installBeforeFinish,
      recorded | synchronized, noEarlierInstall⟩
  · exact ⟨installTime, head, startBeforeInstall, installBeforeFinish, recorded,
      noEarlierInstall⟩
  · exact False.elim
      (noSyncedCommit installTime head startBeforeInstall installBeforeFinish
        synchronized)

/-- Every scheduled callback selects a round above its current signer floor. -/
theorem ValidatorAuthorLocalPreAttemptScheduleAt.target_above_signer_floor
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {obligations : ValidatorProposalObligationExecution timed}
    {time validator : Time}
    (schedule : ValidatorAuthorLocalPreAttemptScheduleAt arms obligations time
      validator) :
    ((timed.execution.trace time).validatorState
        validator).highestSignedRound < schedule.targetRound := by
  cases schedule with
  | normal proposal _ =>
      have enabled := timed.protectedActionIsEnabled time validator
        (.proposeNormal proposal.targetRound proposal.parents)
          proposal.actionProtected
      simpa [ValidatorAuthorLocalPreAttemptScheduleAt.targetRound,
        BasicValidatorActionGuard] using enabled.2.1.1
  | recovery _ _ =>
      simp [ValidatorAuthorLocalPreAttemptScheduleAt.targetRound]
  | recoveryArmPending goal pending _ =>
      have reservation := arms.pendingGoalKeepsReservation time validator goal
        pending
      rw [ValidatorAuthorLocalPreAttemptScheduleAt.targetRound,
        reservation.targetIsExactNext]
      exact Nat.lt_succ_self _

/-- Exact broadcast evidence after one serialized install-handler race. -/
inductive ValidatorAuthorLocalScheduledBroadcastAt
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (obligations : ValidatorProposalObligationExecution timed)
    (start validator : Time) (scheduledTarget : Nat) : Prop where
  | persisted
      (production : Nonempty { result :
          ValidatorPersistedProposalBroadcastProduction timed obligations start
            validator //
        result.proposal.block.reference.round = scheduledTarget }) :
      ValidatorAuthorLocalScheduledBroadcastAt obligations start validator
        scheduledTarget
  | normal
      (scheduledAt targetRound : Time)
      (parents : List (ValidatorBlockRef BlockId))
      (startBeforeScheduled : start ≤ scheduledAt)
      (targetNotReset : scheduledTarget ≤ targetRound)
      (targetPreservedOrGcObsolete : targetRound = scheduledTarget ∨
        scheduledTarget ≤
          ((timed.execution.trace scheduledAt).validatorState
            validator).gcRound + 1)
      (production : Nonempty { result :
          ValidatorNormalProposalBroadcastProduction timed obligations
            scheduledAt validator targetRound //
        result.parents = parents }) :
      ValidatorAuthorLocalScheduledBroadcastAt obligations start validator
        scheduledTarget

/-- The first install after one queued callback cannot create a recursive
commit escape.

Core serialization gives an exact past persistence or one legal protected
normal callback after the handler. Bounded execution turns either case into
an exact addressed broadcast. The install itself is not a result. -/
theorem first_install_scheduled_attempt_eventually_produces_broadcast
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {obligations : ValidatorProposalObligationExecution timed}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (rules : ValidatorAuthorLocalCommitContinuationRules
      (obligations := obligations) arms mode)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start installTime validator : Time}
    {head : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ordered : start ≤ installTime)
    (activeAfter :
      (timed.execution.trace (installTime + 1)).epochActive = true)
    (noEarlierInstall : ∀ earlier candidate,
      start ≤ earlier → earlier < installTime →
      ¬ValidatorCommitInstallOccurs (timed.execution.events earlier) validator
        candidate)
    (installed : ValidatorCommitInstallOccurs
      (timed.execution.events installTime) validator head)
    (schedule : ValidatorAuthorLocalPreAttemptScheduleAt arms obligations start
      validator) :
    ValidatorAuthorLocalScheduledBroadcastAt obligations start validator
      schedule.targetRound := by
  have disposition := rules.firstInstallDisposesScheduledAttempt start
    installTime validator head validatorInRange validatorCorrectAvailable
      ordered activeAfter noEarlierInstall installed schedule
  cases disposition with
  | persisted persistTime block startBeforePersist _persistBeforeInstall
      _blockAuthor blockRound persisted =>
      have blockAboveStart :
          ((timed.execution.trace start).validatorState
              validator).highestSignedRound < block.reference.round := by
        rw [blockRound]
        exact schedule.target_above_signer_floor
      rcases persist_proposal_occurrence_eventually_produces_exact_broadcast
          latchSource effects authorityCountAtLeastTwo validatorInRange
            validatorCorrectAvailable startBeforePersist blockAboveStart
              persisted with
        ⟨⟨production, _persistedAt, proposalBlock⟩⟩
      refine .persisted ⟨⟨production, ?_⟩⟩
      rw [proposalBlock, blockRound]
  | continues continuation =>
      exact .normal (installTime + 1) continuation.work.proposal.targetRound
        continuation.work.proposal.parents
        (Nat.le_trans ordered (Nat.le_succ installTime))
        continuation.targetNotReset continuation.targetPreservedOrGcObsolete
        (protected_normal_proposal_eventually_produces_broadcast_with_exact_parents
          latchSource effects authorityCountAtLeastTwo validatorInRange
            validatorCorrectAvailable continuation.work.proposal)

/-- The record-only specialization of the serialized first-install theorem. -/
theorem first_record_scheduled_attempt_eventually_produces_broadcast
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {obligations : ValidatorProposalObligationExecution timed}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (rules : ValidatorAuthorLocalCommitContinuationRules
      (obligations := obligations) arms mode)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start recordTime validator : Time}
    {head : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ordered : start ≤ recordTime)
    (activeAfter :
      (timed.execution.trace (recordTime + 1)).epochActive = true)
    (noEarlierInstall : ∀ earlier candidate,
      start ≤ earlier → earlier < recordTime →
      ¬ValidatorCommitInstallOccurs (timed.execution.events earlier) validator
        candidate)
    (recorded : ValidatorLocalActionOccurs (timed.execution.events recordTime)
      validator (.recordCommit head))
    (schedule : ValidatorAuthorLocalPreAttemptScheduleAt arms obligations start
      validator) :
    ValidatorAuthorLocalScheduledBroadcastAt obligations start validator
      schedule.targetRound := by
  exact first_install_scheduled_attempt_eventually_produces_broadcast rules
    latchSource effects authorityCountAtLeastTwo validatorInRange
      validatorCorrectAvailable ordered activeAfter noEarlierInstall
        (Or.inl recorded) schedule

/-- A same-author commit-index race is only a finite scheduler interruption.

The first actual install in the finite interval is selected from the past
trace. The serialized first-install rule then returns an exact proposal
broadcast, never the install and never another commit race. -/
theorem commit_advance_disposes_scheduled_attempt_with_broadcast
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {obligations : ValidatorProposalObligationExecution timed}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (rules : ValidatorAuthorLocalCommitContinuationRules
      (obligations := obligations) arms mode)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start finish validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (schedule : ValidatorAuthorLocalPreAttemptScheduleAt arms obligations start
      validator)
    (ordered : start ≤ finish)
    (advanced :
      ((timed.execution.trace start).validatorState
          validator).commitHead.index <
        ((timed.execution.trace finish).validatorState
          validator).commitHead.index)
    (active : ∀ later, start ≤ later →
      (timed.execution.trace later).epochActive = true) :
    ∃ installTime,
      start ≤ installTime ∧ installTime < finish ∧
        ValidatorAuthorLocalScheduledBroadcastAt obligations start validator
          schedule.targetRound := by
  rcases commit_index_advance_has_first_install_before validatorInRange ordered
      advanced with
    ⟨installTime, head, startBeforeInstall, installBeforeFinish, installed,
      noEarlierInstall⟩
  have activeAfter :
      (timed.execution.trace (installTime + 1)).epochActive = true := by
    apply active
    exact Nat.le_trans startBeforeInstall (Nat.le_succ installTime)
  exact ⟨installTime, startBeforeInstall, installBeforeFinish,
    first_install_scheduled_attempt_eventually_produces_broadcast rules
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable startBeforeInstall activeAfter
          noEarlierInstall installed schedule⟩

/-- No-sync specialization which selects the first exact local record. -/
theorem commit_advance_without_sync_disposes_scheduled_attempt_with_broadcast
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {obligations : ValidatorProposalObligationExecution timed}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (rules : ValidatorAuthorLocalCommitContinuationRules
      (obligations := obligations) arms mode)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start finish validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (schedule : ValidatorAuthorLocalPreAttemptScheduleAt arms obligations start
      validator)
    (ordered : start ≤ finish)
    (advanced :
      ((timed.execution.trace start).validatorState
          validator).commitHead.index <
        ((timed.execution.trace finish).validatorState
          validator).commitHead.index)
    (noSyncedCommit : ∀ time head,
      start ≤ time → time < finish →
      ¬ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.applySyncedCommit head))
    (active : ∀ later, start ≤ later →
      (timed.execution.trace later).epochActive = true) :
    ∃ recordTime,
      start ≤ recordTime ∧ recordTime < finish ∧
        ValidatorAuthorLocalScheduledBroadcastAt obligations start validator
          schedule.targetRound := by
  rcases commit_index_advance_without_sync_has_first_record_commit_before
      validatorInRange ordered advanced noSyncedCommit with
    ⟨recordTime, head, startBeforeRecord, recordBeforeFinish, recorded,
      noEarlierInstall⟩
  have activeAfter :
      (timed.execution.trace (recordTime + 1)).epochActive = true := by
    apply active
    exact Nat.le_trans startBeforeRecord (Nat.le_succ recordTime)
  exact ⟨recordTime, startBeforeRecord, recordBeforeFinish,
    first_record_scheduled_attempt_eventually_produces_broadcast rules
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable startBeforeRecord activeAfter
          noEarlierInstall recorded schedule⟩

end Mysticeti
