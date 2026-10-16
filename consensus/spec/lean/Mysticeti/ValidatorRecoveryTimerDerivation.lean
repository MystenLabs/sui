/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorTracePacing
import Mysticeti.ValidatorProposalObligation

namespace Mysticeti

/-! Derive one recovery proposal timer from same-validator rules.

This module does not assume a future `ValidatorRecoveryProposalReady`, a block
layer, or a successful proposal. A timer-start observation records current local
state. The source map states the local rules that keep that exact timer and its
parents usable until its deadline. The final theorem derives the protected
proposal work that `ValidatorTracePacing` consumes.

Two rules are not consequences of the current `ValidatorBoundedExecution`
interface. The local clock needs a bounded relation to trace time, and timer-arm
work needs a bounded completion rule. Both rules are explicit below. The action
enum does not yet contain a timer-arm action, so `completeProtectedAction`
cannot prove its bound.
-/

/-- One local observation made when a validator stores an exact recovery timer.

The commit head and target round are part of the durable timer key. Later round
observations can change `highestObservedRound`. They cannot change this key or
the timer start.
-/
structure ValidatorRecoveryTimerStart (BlockId CommitId : Type) where
  validator : Nat
  commitHead : ValidatorCommitHead CommitId
  targetRound : Nat
  parentReadyAt : Time
  startedAt : Time
  highestObservedRound : Nat

namespace ValidatorRecoveryTimerStart

/-- The absolute trace-time deadline stored for one timer key. -/
def deadline
    {BlockId CommitId : Type}
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)) : Time :=
  start.startedAt + waits.wait start.commitHead start.targetRound

/-- The exact local recovery record stored for one timer key. -/
def recovery
    {BlockId CommitId : Type}
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)) :
    ValidatorRecoveryState BlockId CommitId :=
  { baselineCommit := start.commitHead
    targetRound := start.targetRound
    parentsReadyAt := some start.parentReadyAt
    deadline := some (start.deadline waits)
    alignmentWitness := none }

/-- The stored timer key fixes its absolute deadline from the start and wait. -/
@[simp]
theorem deadline_eq
    {BlockId CommitId : Type}
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)) :
    start.deadline waits =
      start.startedAt + waits.wait start.commitHead start.targetRound := by
  rfl

end ValidatorRecoveryTimerStart

/-- Durable local work that arms one recovery timer. -/
structure ValidatorRecoveryTimerArmGoal (BlockId CommitId : Type) where
  validator : Nat
  commitHead : ValidatorCommitHead CommitId
  targetRound : Nat
  parentsReadyAt : Time

namespace ValidatorRecoveryTimerArmGoal

/-- Turn one completed arm goal into its exact timer-start observation. -/
def toTimerStart
    {BlockId CommitId : Type}
    (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId)
    (startedAt : Time) : ValidatorRecoveryTimerStart BlockId CommitId :=
  { validator := goal.validator
    commitHead := goal.commitHead
    targetRound := goal.targetRound
    parentReadyAt := goal.parentsReadyAt
    startedAt
    highestObservedRound := goal.targetRound }

end ValidatorRecoveryTimerArmGoal

/-- The isolated durable state of one timer-arm worker. -/
structure ValidatorRecoveryTimerArmState (BlockId CommitId : Type) where
  pending : Option (ValidatorRecoveryTimerArmGoal BlockId CommitId)

/-- One isolated timer-arm worker event. -/
inductive ValidatorRecoveryTimerArmEvent (BlockId CommitId : Type) where
  | idle
  | latch (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId)
  | complete (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId)

/-- A latched timer-arm goal persists until its matching completion. -/
inductive ValidatorRecoveryTimerArmTransition
    {BlockId CommitId : Type} :
    ValidatorRecoveryTimerArmState BlockId CommitId →
      ValidatorRecoveryTimerArmEvent BlockId CommitId →
      ValidatorRecoveryTimerArmState BlockId CommitId → Prop where
  | idle {state} : ValidatorRecoveryTimerArmTransition state .idle state
  | latch {before after goal} :
      before.pending = none →
      after.pending = some goal →
      ValidatorRecoveryTimerArmTransition before (.latch goal) after
  | wait {before after goal} :
      before.pending = some goal →
      after.pending = some goal →
      ValidatorRecoveryTimerArmTransition before .idle after
  | complete {before after goal} :
      before.pending = some goal →
      after.pending = none →
      ValidatorRecoveryTimerArmTransition before (.complete goal) after

/-- One local state has an origin-aware quorum parent witness for a recovery
target. The timer records only the time of this fact. It does not store this
list as the final proposal list. -/
def ValidatorRecoveryParentQuorumReadyAt
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (state : ValidatorLocalState BlockId CommitId)
    (targetRound : Nat) : Prop :=
  ∃ parents,
    ValidatorProposalParentListReady .commitProgressRecovery config state
      targetRound parents

/-- A proposal-time refresh selects the current retained representative for
each included author.

`ValidatorProposalParentListReady` gives exact target-minus-one rounds, quorum
stake, and at most one branch per author. The two representative fields make
the selection fresh: it uses the current accepted representative map and it
includes every current representative whose body is retained.
-/
structure ValidatorRefreshedRecoveryParentListAt
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (state : ValidatorLocalState BlockId CommitId)
    (targetRound : Nat)
    (parents : List (ValidatorBlockRef BlockId)) : Prop where
  ready :
    ValidatorProposalParentListReady .commitProgressRecovery config state
      targetRound parents
  selectedAuthorsInRange : ∀ parent,
    parent ∈ parents → parent.author < config.authorityCount
  selectedCurrentRepresentatives : ∀ parent,
    parent ∈ parents →
    state.acceptedRepresentative (targetRound - 1) parent.author = some parent
  includesRetainedCurrentRepresentatives : ∀ author parent,
    author < config.authorityCount →
    state.acceptedRepresentative (targetRound - 1) author = some parent →
    state.retained parent = true →
    parent ∈ parents

/-- Schedule-independent recovery parent selection.

The list contains one current accepted and retained representative per author.
It does not consult the leader schedule and it does not score-exclude any such
representative. Quorum readiness is a separate property. -/
structure ValidatorFullRetainedRepresentativeParentSelectionAt
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (state : ValidatorLocalState BlockId CommitId)
    (targetRound : Nat)
    (parents : List (ValidatorBlockRef BlockId)) : Prop where
  parentAuthorsNodup :
    (parents.map ValidatorBlockRef.author).Nodup
  selectedAuthorsInRange : ∀ parent,
    parent ∈ parents → parent.author < config.authorityCount
  selectedCurrentRepresentatives : ∀ parent,
    parent ∈ parents →
    state.acceptedRepresentative (targetRound - 1) parent.author = some parent
  includesEveryRetainedCurrentRepresentative : ∀ author parent,
    author < config.authorityCount →
    state.acceptedRepresentative (targetRound - 1) author = some parent →
    state.retained parent = true →
    parent ∈ parents

namespace ValidatorRefreshedRecoveryParentListAt

/-- A refreshed recovery list uses the schedule-independent full retained
representative selection rule. -/
theorem fullRetainedRepresentativeSelection
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorLocalState BlockId CommitId}
    {targetRound : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (fresh : ValidatorRefreshedRecoveryParentListAt config state targetRound
      parents) :
    ValidatorFullRetainedRepresentativeParentSelectionAt config state
      targetRound parents := by
  exact ⟨fresh.ready.1.1, fresh.selectedAuthorsInRange,
    fresh.selectedCurrentRepresentatives,
    fresh.includesRetainedCurrentRepresentatives⟩

/-- A refreshed list contains at most one branch for each author. -/
theorem oneBranchPerAuthor
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorLocalState BlockId CommitId}
    {targetRound : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (fresh : ValidatorRefreshedRecoveryParentListAt config state targetRound
      parents) :
    (parents.map ValidatorBlockRef.author).Nodup :=
  fresh.ready.1.1

/-- Every refreshed parent is from the exact immediate predecessor round. -/
theorem exactImmediateRound
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorLocalState BlockId CommitId}
    {targetRound : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (fresh : ValidatorRefreshedRecoveryParentListAt config state targetRound
      parents) :
    ∀ parent, parent ∈ parents → parent.round + 1 = targetRound := by
  intro parent included
  exact (fresh.ready.1.2.1 parent included).1

end ValidatorRefreshedRecoveryParentListAt

/-- One arm goal is eligible from current same-validator state. -/
def ValidatorRecoveryTimerArmEligibleAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time : Time) (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId) :
    Prop :=
  (timed.execution.trace time).epochActive = true ∧
    goal.validator < config.authorityCount ∧
    faults.correctAvailable goal.validator = true ∧
    ((timed.execution.trace time).validatorState
      goal.validator).recovery.isNone = true ∧
    ((timed.execution.trace time).validatorState
      goal.validator).commitHead = goal.commitHead ∧
    goal.targetRound =
      ((timed.execution.trace time).validatorState
        goal.validator).highestSignedRound + 1 ∧
    goal.parentsReadyAt = time ∧
    ValidatorRecoveryParentQuorumReadyAt config
      ((timed.execution.trace time).validatorState goal.validator)
      goal.targetRound

/-- Fundamental local input for timer-arm work at one validator.

When the signer floor is zero, a canonical retained genesis parent list makes
this same input target round one. Genesis does not bypass the durable worker or
the recovery wait.
-/
def ValidatorRecoveryTimerArmInputAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time : Time) (validator : Nat) : Prop :=
  (timed.execution.trace time).epochActive = true ∧
    validator < config.authorityCount ∧
    faults.correctAvailable validator = true ∧
    ((timed.execution.trace time).validatorState
      validator).recovery.isNone = true ∧
    ValidatorRecoveryParentQuorumReadyAt config
      ((timed.execution.trace time).validatorState validator)
      (((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1)

/-- The current main state still reserves one occupied timer-arm goal.

This is a same-validator state condition. It prevents a second recovery timer
from replacing the occupied goal, and it keeps the exact signer floor and
recovery-origin parent pins until the worker completes.
-/
structure ValidatorRecoveryTimerPendingReservationAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Time)
    (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId) : Prop where
  goalValidator : goal.validator = validator
  epochActive : (timed.execution.trace time).epochActive = true
  validatorInRange : validator < config.authorityCount
  validatorCorrectAvailable : faults.correctAvailable validator = true
  noStoredTimer :
    ((timed.execution.trace time).validatorState
      validator).recovery.isNone = true
  commitHeadCurrent :
    ((timed.execution.trace time).validatorState validator).commitHead =
      goal.commitHead
  targetIsExactNext :
    goal.targetRound =
      ((timed.execution.trace time).validatorState
      validator).highestSignedRound + 1
  parentsReadyAtReached : goal.parentsReadyAt ≤ time
  initialQuorumReady :
    ValidatorRecoveryParentQuorumReadyAt config
      ((timed.execution.trace time).validatorState validator)
      goal.targetRound

/-- A timer restored at trace time zero is valid local durable state.

This is the only non-completion origin for `timerStarted`. It models restart
recovery of an already stored timer. It does not assume proposal readiness.
-/
structure ValidatorRestoredRecoveryTimerValidAtZero
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (start : ValidatorRecoveryTimerStart BlockId CommitId) : Prop where
  startedAtZero : start.startedAt = 0
  parentReadyBeforeStart : start.parentReadyAt ≤ start.startedAt
  epochActive : (timed.execution.trace 0).epochActive = true
  validatorInRange : start.validator < config.authorityCount
  validatorCorrectAvailable :
    faults.correctAvailable start.validator = true
  commitHeadCurrent :
    ((timed.execution.trace 0).validatorState start.validator).commitHead =
      start.commitHead
  targetIsExactNext :
    start.targetRound =
      ((timed.execution.trace 0).validatorState
        start.validator).highestSignedRound + 1
  initialQuorumReady :
    ValidatorRecoveryParentQuorumReadyAt config
      ((timed.execution.trace 0).validatorState start.validator)
      start.targetRound
  timerStored :
    ((timed.execution.trace 0).validatorState start.validator).recovery =
      some (start.recovery waits)

/-- One epoch stays active for one closed trace-time interval. -/
def ValidatorEpochActiveBetween
    {BlockId CommitId PacketId : Type}
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (first last : Time) : Prop :=
  ∀ time, first ≤ time → time ≤ last →
    (trace time).epochActive = true

/-- One validator keeps one commit head for one closed trace-time interval. -/
def ValidatorCommitHeadStableBetween
    {BlockId CommitId PacketId : Type}
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (validator : Nat) (commitHead : ValidatorCommitHead CommitId)
    (first last : Time) : Prop :=
  ∀ time, first ≤ time → time ≤ last →
    ((trace time).validatorState validator).commitHead = commitHead

/-- Current local state contains one exact recovery timer for the current head
and signer floor. This is a state predicate, not a timer-start witness. -/
def ValidatorArmedExactRecoveryTimerAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Time) : Prop :=
  ∃ recovery,
    ((timed.execution.trace time).validatorState validator).recovery =
        some recovery ∧
      recovery.baselineCommit =
        ((timed.execution.trace time).validatorState validator).commitHead ∧
      recovery.targetRound =
        ((timed.execution.trace time).validatorState
          validator).highestSignedRound + 1 ∧
      recovery.parentsReadyAt.isSome = true ∧
      recovery.deadline.isSome = true ∧
      recovery.alignmentWitness = none

/-- A current accepted-layer observation can start a timer or reuse the exact
timer that this host already armed. -/
def ValidatorRecoveryTimerCurrentInputAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Time) : Prop :=
  ValidatorRecoveryTimerArmInputAt timed time validator ∨
    ((timed.execution.trace time).epochActive = true ∧
      validator < config.authorityCount ∧
      faults.correctAvailable validator = true ∧
      ValidatorArmedExactRecoveryTimerAt timed time validator)

/-- Same-validator source rules for exact recovery timers.

`timerStarted` identifies real local timer-start observations. The other fields
map one such observation to current local state and to simple persistence,
clock, and protected-work rules. They do not name a future block or successful
proposal.
-/
structure ValidatorRecoveryTimerSourceMap
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (program : ValidatorExecutionProgram BlockId CommitId)
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)) where
  timerStarted : ValidatorRecoveryTimerStart BlockId CommitId → Prop
  validatorInRange : ∀ start,
    timerStarted start → start.validator < config.authorityCount
  validatorCorrectAvailable : ∀ start,
    timerStarted start → faults.correctAvailable start.validator = true
  startsAfterParentsReady : ∀ start,
    timerStarted start → start.parentReadyAt ≤ start.startedAt
  /-- This is the explicit timer-arm source mapping. A future concrete
  `armRecoveryTimer` action can replace this field. -/
  startsWithinLocalBound : ∀ start,
    timerStarted start →
    start.startedAt ≤
      start.parentReadyAt + timed.localActionBound + 2
  commitHeadAtStart : ∀ start,
    timerStarted start →
    ((timed.execution.trace start.startedAt).validatorState
      start.validator).commitHead = start.commitHead
  targetIsExactNextAtStart : ∀ start,
    timerStarted start →
    start.targetRound =
      ((timed.execution.trace start.startedAt).validatorState
        start.validator).highestSignedRound + 1
  /-- This proof-only witness is the quorum that caused the timer to arm. The
  timer key does not store it and the final proposal does not reuse it. -/
  initialQuorumParents : ValidatorRecoveryTimerStart BlockId CommitId →
    List (ValidatorBlockRef BlockId)
  initialQuorumReadyAtStart : ∀ start,
    timerStarted start →
    ValidatorProposalParentListReady .commitProgressRecovery config
      ((timed.execution.trace start.startedAt).validatorState start.validator)
      start.targetRound (initialQuorumParents start)
  /-- Timer-arm selection records one current representative for each author
  in its proof-only start quorum. This permits other accepted equivocations to
  exist; it does not map every accepted branch to the functional slot. -/
  initialQuorumParentsAreRepresentativesAtStart : ∀ start,
    timerStarted start →
    ∀ parent, parent ∈ initialQuorumParents start →
      ((timed.execution.trace start.startedAt).validatorState
        start.validator).acceptedRepresentative
          (start.targetRound - 1) parent.author = some parent
  timerStoredAtStart : ∀ start,
    timerStarted start →
    ((timed.execution.trace start.startedAt).validatorState
      start.validator).recovery = some (start.recovery waits)
  /-- The timer key and absolute deadline stay stored while their commit head
  is current. -/
  timerPersistsUntilDeadline : ∀ start time,
    timerStarted start →
    start.startedAt ≤ time →
    time ≤ start.deadline waits →
    ((timed.execution.trace time).validatorState
      start.validator).commitHead = start.commitHead →
    ((timed.execution.trace time).validatorState
      start.validator).recovery = some (start.recovery waits)
  /-- The durable exact-next timer reserves its signer floor until its
  deadline. A higher-round observation does not move this floor. -/
  exactNextFloorPersistsUntilDeadline : ∀ start time,
    timerStarted start →
    start.startedAt ≤ time →
    time ≤ start.deadline waits →
    ((timed.execution.trace time).validatorState
      start.validator).commitHead = start.commitHead →
    start.targetRound =
      ((timed.execution.trace time).validatorState
        start.validator).highestSignedRound + 1
  /-- The timer keeps GC below its immediate-parent round. Genesis is the only
  exception. An at-or-below-GC block is a committed root, not a proposal
  parent. -/
  gcFencedUntilDeadline : ∀ start time,
    timerStarted start →
    start.startedAt ≤ time →
    time ≤ start.deadline waits →
    ((timed.execution.trace time).validatorState
      start.validator).commitHead = start.commitHead →
    start.targetRound = 1 ∨
      ((timed.execution.trace time).validatorState
        start.validator).gcRound + 1 < start.targetRound
  /-- Bodies in the start-quorum witness remain pinned for this timer target.
  This is a local retention rule, not a future quorum-ready result. -/
  initialQuorumParentsStayRetainedUntilDeadline : ∀ start time,
    timerStarted start →
    start.startedAt ≤ time →
    time ≤ start.deadline waits →
    ((timed.execution.trace time).validatorState
      start.validator).commitHead = start.commitHead →
    ∀ parent, parent ∈ initialQuorumParents start →
      ((timed.execution.trace time).validatorState
        start.validator).retained parent = true
  /-- Proposal-time selection reads the current accepted representative map.
  It does not copy the proof-only start list. Durable selected start parents
  supply quorum, and newly accepted retained representatives can join it. -/
  refreshedParents : ValidatorRecoveryTimerStart BlockId CommitId →
    Time → List (ValidatorBlockRef BlockId)
  refreshedParentsMembershipIff : ∀ start time parent,
    parent ∈ refreshedParents start time ↔
      parent.author < config.authorityCount ∧
        ((timed.execution.trace time).validatorState
          start.validator).acceptedRepresentative
            (start.targetRound - 1) parent.author = some parent ∧
        ((timed.execution.trace time).validatorState
          start.validator).retained parent = true
  refreshedParentAuthorsNodup : ∀ start time,
    ((refreshedParents start time).map ValidatorBlockRef.author).Nodup
  /-- A correct validator's local clock advances by at least the elapsed trace
  time while the epoch stays active. -/
  localClockKeepsPace : ∀ start time,
    timerStarted start →
    start.startedAt ≤ time →
    time ≤ start.deadline waits →
    ValidatorEpochActiveBetween timed.execution.trace start.startedAt time →
    ((timed.execution.trace start.startedAt).validatorState
        start.validator).clock + (time - start.startedAt) ≤
      ((timed.execution.trace time).validatorState start.validator).clock
  localClockCoversStart : ∀ start,
    timerStarted start →
    start.startedAt ≤
      ((timed.execution.trace start.startedAt).validatorState
        start.validator).clock
  /-- An expired, current, exact-next timer creates durable proposal work with
  the current refreshed representative list. -/
  expiredTimerProtectsRefreshedProposal : ∀ start time,
    timerStarted start →
    (timed.execution.trace time).epochActive = true →
    ((timed.execution.trace time).validatorState
      start.validator).commitHead = start.commitHead →
    ((timed.execution.trace time).validatorState
      start.validator).recovery = some (start.recovery waits) →
    start.targetRound =
      ((timed.execution.trace time).validatorState
        start.validator).highestSignedRound + 1 →
    start.deadline waits ≤
      ((timed.execution.trace time).validatorState start.validator).clock →
    ValidatorRefreshedRecoveryParentListAt config
      ((timed.execution.trace time).validatorState start.validator)
      start.targetRound (refreshedParents start time) →
    timed.protectedAction time start.validator
      (.proposeNext (refreshedParents start time))
  /-- An active stored recovery record at a correct host is the exact timer for
  that host's current commit head and next signer round. Stale or partially
  restored recovery records are not permitted to remain in active state. -/
  activeStoredRecoveryIsArmedExact : ∀ time validator recovery,
    (timed.execution.trace time).epochActive = true →
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((timed.execution.trace time).validatorState validator).recovery =
      some recovery →
    ValidatorArmedExactRecoveryTimerAt timed time validator
  /-- A current exact stored timer has one real local timer-start source. The
  returned active interval supplies the trace history that a current-state
  theorem needs when the timer started before its observation time. -/
  storedExactTimerHasSource : ∀ time validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorArmedExactRecoveryTimerAt timed time validator →
    ∃ start,
      timerStarted start ∧
        start.validator = validator ∧
        start.commitHead =
          ((timed.execution.trace time).validatorState validator).commitHead ∧
        start.targetRound =
          ((timed.execution.trace time).validatorState
            validator).highestSignedRound + 1 ∧
        ((timed.execution.trace time).validatorState validator).recovery =
          some (start.recovery waits) ∧
        start.startedAt ≤ time ∧
        ValidatorEpochActiveBetween timed.execution.trace start.startedAt time

/-- A trace-pacing timer together with the stronger recovery-origin parent
rule that its proposal latch needs. -/
structure ValidatorOriginAwareRecoveryProposalReady
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (commitHead : ValidatorCommitHead CommitId)
    (targetRound validator : Nat) where
  ready : ValidatorRecoveryProposalReady faults timed waits commitHead
    targetRound validator
  startedWithinLocalBound :
    ready.startedAt ≤ ready.parentReadyAt + timed.localActionBound + 2
  refreshedRecoveryParents :
    ValidatorRefreshedRecoveryParentListAt config
      ((timed.execution.trace
        (ready.startedAt + waits.wait commitHead targetRound)).validatorState
          validator)
      targetRound ready.parents
  recoveryParentsReady :
    ValidatorProposalParentListReady .commitProgressRecovery config
      ((timed.execution.trace
        (ready.startedAt + waits.wait commitHead targetRound)).validatorState
          validator)
      targetRound ready.parents

namespace ValidatorRecoveryTimerSourceMap

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}

/-- Current parent readiness and the actual optional recovery field give the
canonical timer input. An empty field starts arm work. A nonempty field must be
the exact current timer. -/
theorem active_parent_quorum_state_gives_current_timer_input
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (time validator : Time)
    (active : (timed.execution.trace time).epochActive = true)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (parentsReady : ValidatorRecoveryParentQuorumReadyAt config
      ((timed.execution.trace time).validatorState validator)
      (((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1)) :
    ValidatorRecoveryTimerCurrentInputAt timed time validator := by
  cases recoveryValue :
      ((timed.execution.trace time).validatorState validator).recovery with
  | none =>
      left
      exact ⟨active, validatorInRange, validatorCorrectAvailable,
        by simp [recoveryValue], parentsReady⟩
  | some recovery =>
      right
      exact ⟨active, validatorInRange, validatorCorrectAvailable,
        source.activeStoredRecoveryIsArmedExact time validator recovery active
          validatorInRange validatorCorrectAvailable recoveryValue⟩

/-- One protected timer-arm task has not completed in a half-open interval. -/
def TimerArmCompletionAbsentBefore
    (events : Time → Nat → ValidatorRecoveryTimerArmEvent BlockId CommitId)
    (start finish : Time)
    (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId) : Prop :=
  ∀ time, start ≤ time → time < finish →
    events time goal.validator ≠ .complete goal

/-- Isolated bounded execution for durable timer-arm work.

This interface is separate because `ValidatorLocalAction` does not yet have an
`armRecoveryTimer` action. Its protected-work rules have the same strict shape
as `ValidatorBoundedExecution`: the goal persists until it runs, and only a
protected goal gets the finite bound.
-/
structure ValidatorRecoveryTimerArmExecution
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits) where
  trace : Time → Nat → ValidatorRecoveryTimerArmState BlockId CommitId
  events : Time → Nat → ValidatorRecoveryTimerArmEvent BlockId CommitId
  transitionsFollowRules : ∀ time validator,
    ValidatorRecoveryTimerArmTransition (trace time validator)
      (events time validator) (trace (time + 1) validator)
  selectedGoal :
    Time → Nat → Option (ValidatorRecoveryTimerArmGoal BlockId CommitId)
  readyStateSelectsGoal : ∀ time validator,
    ValidatorRecoveryTimerArmInputAt timed time validator →
    (trace time validator).pending = none →
    ∃ goal,
      selectedGoal time validator = some goal ∧
        goal.validator = validator ∧
        goal.commitHead =
          ((timed.execution.trace time).validatorState validator).commitHead ∧
        goal.targetRound =
          ((timed.execution.trace time).validatorState
            validator).highestSignedRound + 1 ∧
        goal.parentsReadyAt = time ∧
        ValidatorRecoveryTimerArmEligibleAt timed time goal
  selectedGoalIsEligible : ∀ time goal,
    selectedGoal time goal.validator = some goal →
    ValidatorRecoveryTimerArmEligibleAt timed time goal
  protectedArmWork :
    Time → ValidatorRecoveryTimerArmGoal BlockId CommitId → Prop
  /-- A goal restored after a host restart is the initial occupied goal. -/
  restoredArmGoal : ValidatorRecoveryTimerArmGoal BlockId CommitId → Prop
  restoredArmGoalIsPendingAtZero : ∀ goal,
    restoredArmGoal goal →
    (trace 0 goal.validator).pending = some goal
  /-- Every occupied goal has either a real latch event or a validated restart
  origin. This includes an occupied worker at trace time zero. -/
  pendingGoalHasOrigin : ∀ time validator goal,
    (trace time validator).pending = some goal →
    (∃ latchedAt,
      latchedAt < time ∧
        selectedGoal latchedAt goal.validator = some goal ∧
        events latchedAt goal.validator = .latch goal) ∨
      restoredArmGoal goal
  /-- Occupancy is itself protected durable work. -/
  pendingGoalIsProtected : ∀ time validator goal,
    (trace time validator).pending = some goal →
    protectedArmWork time goal
  /-- An occupied worker reserves its exact local head, signer floor, and
  retained recovery parents until completion. -/
  pendingGoalKeepsReservation : ∀ time validator goal,
    (trace time validator).pending = some goal →
    ValidatorRecoveryTimerPendingReservationAt timed time validator goal
  selectedGoalLatches : ∀ time goal,
    selectedGoal time goal.validator = some goal →
    events time goal.validator = .latch goal
  latchedGoalIsProtected : ∀ time goal,
    events time goal.validator = .latch goal →
    protectedArmWork (time + 1) goal
  protectedArmWorkPersistsUntilRun : ∀ start finish goal,
    protectedArmWork start goal →
    start ≤ finish →
    TimerArmCompletionAbsentBefore events start finish goal →
    protectedArmWork finish goal
  completeProtectedArmWork : ∀ latchedAt goal,
    protectedArmWork latchedAt goal →
    ∃ completedAt,
      latchedAt ≤ completedAt ∧
        completedAt ≤ latchedAt + timed.localActionBound ∧
        events completedAt goal.validator = .complete goal
  /-- Timer-arm completion is serialized with the local signer floor. The
  completion stores the exact timer, or a commit-head installation wins the
  same step. A signer-floor change without commit progress cannot silently
  replace this recovery goal; the host must defer that signing work or retry it
  after this arm step. -/
  completedArmStartsExactTimerOrCommitAdvances : ∀ time goal,
    events time goal.validator = .complete goal →
    source.timerStarted (goal.toTimerStart (time + 1)) ∨
      goal.commitHead.index <
        ((timed.execution.trace (time + 1)).validatorState
          goal.validator).commitHead.index
  /-- A stored timer comes from this worker's exact completion event or from a
  validated timer restored at trace time zero. -/
  restoredTimer : ValidatorRecoveryTimerStart BlockId CommitId → Prop
  restoredTimerIsValid : ∀ start,
    restoredTimer start →
    ValidatorRestoredRecoveryTimerValidAtZero faults timed waits start
  timerStartedHasOrigin : ∀ start,
    source.timerStarted start →
    (∃ completedAt goal,
      events completedAt goal.validator = .complete goal ∧
        start = goal.toTimerStart (completedAt + 1)) ∨
      restoredTimer start

/-- A change from one armed commit head has advanced the local commit index. -/
def ValidatorRecoveryTimerHeadRace
    (_source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (start : ValidatorRecoveryTimerStart BlockId CommitId) : Prop :=
  ∃ changedAt,
    start.startedAt ≤ changedAt ∧
      changedAt ≤ start.deadline waits ∧
      start.commitHead.index <
        ((timed.execution.trace changedAt).validatorState
          start.validator).commitHead.index

/-- Every in-range author in the start quorum remains represented in the
proposal-time refreshed list. -/
theorem initial_quorum_parent_is_in_refreshed_deadline
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (started : source.timerStarted start)
    (headAtDeadline :
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator).commitHead = start.commitHead)
    (parent : ValidatorBlockRef BlockId)
    (included : parent ∈ source.initialQuorumParents start)
    (authorInRange : parent.author < config.authorityCount) :
    parent ∈ source.refreshedParents start (start.deadline waits) := by
  have representativeAtStart :=
    source.initialQuorumParentsAreRepresentativesAtStart start started parent
      included
  have representative :
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator).acceptedRepresentative
          (start.targetRound - 1) parent.author = some parent := by
    exact
      (timed.execution.durable_fields_persist
        (source.validatorInRange start started)
        (by simp [ValidatorRecoveryTimerStart.deadline]))
        |>.accepted_representative_persists representativeAtStart
  have retained := source.initialQuorumParentsStayRetainedUntilDeadline start
    (start.deadline waits) started
    (by simp [ValidatorRecoveryTimerStart.deadline]) (Nat.le_refl _)
    headAtDeadline parent included
  exact (source.refreshedParentsMembershipIff start (start.deadline waits)
    parent).2 ⟨authorInRange, representative, retained⟩

/-- The deadline selector derives a fresh ready list from current accepted and
retained representatives. No source field supplies this future ready result. -/
theorem refreshed_parents_at_deadline
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (started : source.timerStarted start)
    (headAtDeadline :
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator).commitHead = start.commitHead) :
    ValidatorRefreshedRecoveryParentListAt config
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator)
      start.targetRound
      (source.refreshedParents start (start.deadline waits)) := by
  have startBeforeDeadline : start.startedAt ≤ start.deadline waits := by
    simp [ValidatorRecoveryTimerStart.deadline]
  have exactNext := source.exactNextFloorPersistsUntilDeadline start
    (start.deadline waits) started startBeforeDeadline (Nat.le_refl _)
    headAtDeadline
  have targetPositive : 0 < start.targetRound := by omega
  have worldWellFormed := timed.execution.statesWellFormed
    (start.deadline waits)
  have localWellFormed := worldWellFormed start.validator
    (source.validatorInRange start started)
  refine
    { ready := ?_
      selectedAuthorsInRange := ?_
      selectedCurrentRepresentatives := ?_
      includesRetainedCurrentRepresentatives := ?_ }
  · refine ⟨?_, ?_⟩
    · refine ⟨source.refreshedParentAuthorsNodup start
          (start.deadline waits), ?_, ?_⟩
      · intro parent included
        have selected := (source.refreshedParentsMembershipIff start
          (start.deadline waits) parent).1 included
        have sound := localWellFormed.acceptedRepresentativeIsSound
          (start.targetRound - 1) parent.author parent selected.2.1
        refine ⟨?_, sound.2.2.1⟩
        rw [sound.2.1]
        omega
      · have initialReady := source.initialQuorumReadyAtStart start started
        have subset : VoterSet.SubsetAt config.authorityCount
            (validatorParentAuthors (source.initialQuorumParents start))
            (validatorParentAuthors
              (source.refreshedParents start (start.deadline waits))) := by
          intro author authorInRange initialAuthor
          simp [validatorParentAuthors] at initialAuthor ⊢
          rcases initialAuthor with ⟨parent, included, parentAuthor⟩
          have parentAuthorInRange : parent.author < config.authorityCount := by
            simpa [parentAuthor] using authorInRange
          have refreshed :=
            source.initial_quorum_parent_is_in_refreshed_deadline start started
              headAtDeadline parent included parentAuthorInRange
          exact ⟨parent, refreshed, parentAuthor⟩
        exact Nat.le_trans initialReady.1.2.2
          (weight_mono config.stake subset)
    · intro parent included
      have selected := (source.refreshedParentsMembershipIff start
        (start.deadline waits) parent).1 included
      have sound := localWellFormed.acceptedRepresentativeIsSound
        (start.targetRound - 1) parent.author parent selected.2.1
      have gcFence := source.gcFencedUntilDeadline start
        (start.deadline waits) started
        (by simp [ValidatorRecoveryTimerStart.deadline]) (Nat.le_refl _)
        headAtDeadline
      refine ⟨selected.2.2, ?_⟩
      rcases gcFence with targetOne | gcBelowTarget
      · exact Or.inl (by omega)
      · exact Or.inr (by omega)
  · intro parent included
    exact ((source.refreshedParentsMembershipIff start
      (start.deadline waits) parent).1 included).1
  · intro parent included
    exact ((source.refreshedParentsMembershipIff start
      (start.deadline waits) parent).1 included).2.1
  · intro author parent authorInRange representative retained
    have parentAuthor :=
      (localWellFormed.acceptedRepresentativeIsSound
        (start.targetRound - 1) author parent representative).1
    subst author
    exact (source.refreshedParentsMembershipIff start
      (start.deadline waits) parent).2
        ⟨authorInRange, representative, retained⟩

/-- The refreshed proposal list is a basic legal parent list. -/
theorem parents_ready_at_deadline
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (started : source.timerStarted start)
    (headAtDeadline :
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator).commitHead = start.commitHead) :
    ValidatorParentListReady config
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator)
      start.targetRound
      (source.refreshedParents start (start.deadline waits)) := by
  exact (source.refreshed_parents_at_deadline start started
    headAtDeadline).ready.1

/-- The timer keeps the recovery-origin parent rule at its deadline. Every
positive immediate parent stays strictly above the local GC round. -/
theorem recovery_parents_ready_at_deadline
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (started : source.timerStarted start)
    (headAtDeadline :
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator).commitHead = start.commitHead) :
    ValidatorProposalParentListReady .commitProgressRecovery config
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator)
      start.targetRound
      (source.refreshedParents start (start.deadline waits)) := by
  exact (source.refreshed_parents_at_deadline start started
    headAtDeadline).ready

/-- With monotone commit installation, the same head at the deadline means
that this validator kept the head for the full timer interval. -/
theorem same_head_at_deadline_implies_stable_interval
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (started : source.timerStarted start)
    (headAtDeadline :
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator).commitHead = start.commitHead) :
    ValidatorCommitHeadStableBetween timed.execution.trace start.validator
      start.commitHead start.startedAt (start.deadline waits) := by
  intro time afterStart beforeDeadline
  have range := source.validatorInRange start started
  have startToTime := timed.execution.durable_fields_persist range afterStart
  have timeToDeadline := timed.execution.durable_fields_persist range
    beforeDeadline
  have headAtStart := source.commitHeadAtStart start started
  have startIndexLeTime : start.commitHead.index ≤
      ((timed.execution.trace time).validatorState
        start.validator).commitHead.index := by
    simpa [headAtStart] using startToTime.1
  have timeIndexLeStart :
      ((timed.execution.trace time).validatorState
          start.validator).commitHead.index ≤ start.commitHead.index := by
    have bound := timeToDeadline.1
    rw [headAtDeadline] at bound
    exact bound
  have sameIndex :
      ((timed.execution.trace start.startedAt).validatorState
          start.validator).commitHead.index =
        ((timed.execution.trace time).validatorState
          start.validator).commitHead.index := by
    rw [headAtStart]
    omega
  have sameHead := startToTime.2.2.1 sameIndex
  rw [headAtStart] at sameHead
  exact sameHead.symm

/-- If the exact timer is still current after its deadline, monotone commit
installation shows that the same head was current at the deadline. -/
theorem current_head_after_deadline_implies_head_at_deadline
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (started : source.timerStarted start)
    (time : Time)
    (deadlineBeforeTime : start.deadline waits ≤ time)
    (headAtTime :
      ((timed.execution.trace time).validatorState
        start.validator).commitHead = start.commitHead) :
    ((timed.execution.trace (start.deadline waits)).validatorState
      start.validator).commitHead = start.commitHead := by
  have range := source.validatorInRange start started
  have startBeforeDeadline : start.startedAt ≤ start.deadline waits := by
    simp [ValidatorRecoveryTimerStart.deadline]
  have startToDeadline := timed.execution.durable_fields_persist range
    startBeforeDeadline
  have deadlineToTime := timed.execution.durable_fields_persist range
    deadlineBeforeTime
  have headAtStart := source.commitHeadAtStart start started
  have startIndexLeDeadline : start.commitHead.index ≤
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator).commitHead.index := by
    simpa [headAtStart] using startToDeadline.1
  have deadlineIndexLeStart :
      ((timed.execution.trace (start.deadline waits)).validatorState
          start.validator).commitHead.index ≤ start.commitHead.index := by
    have bound := deadlineToTime.1
    rw [headAtTime] at bound
    exact bound
  have sameIndex :
      ((timed.execution.trace start.startedAt).validatorState
          start.validator).commitHead.index =
        ((timed.execution.trace (start.deadline waits)).validatorState
          start.validator).commitHead.index := by
    rw [headAtStart]
    omega
  have sameHead := startToDeadline.2.2.1 sameIndex
  rw [headAtStart] at sameHead
  exact sameHead.symm

/-- The bounded local clock rule makes the stored deadline expire at its trace
time. Mere unbounded clock progress is not sufficient for this result. -/
theorem clock_reaches_deadline
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (started : source.timerStarted start)
    (active : ValidatorEpochActiveBetween timed.execution.trace start.startedAt
      (start.deadline waits)) :
    start.deadline waits ≤
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator).clock := by
  have advances := source.localClockKeepsPace start (start.deadline waits)
    started (by simp [ValidatorRecoveryTimerStart.deadline]) (Nat.le_refl _)
    active
  have coversStart := source.localClockCoversStart start started
  have waitShape :
      start.deadline waits - start.startedAt =
        waits.wait start.commitHead start.targetRound := by
    simp [ValidatorRecoveryTimerStart.deadline]
  rw [waitShape] at advances
  unfold ValidatorRecoveryTimerStart.deadline
  exact Nat.le_trans
    (Nat.add_le_add_right coversStart
      (waits.wait start.commitHead start.targetRound)) advances

/-- One real timer start and same-validator rules derive the exact protected
proposal input used by the trace pacing proof. -/
def recoveryProposalReadyOfTimerStart
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (started : source.timerStarted start)
    (headAtDeadline :
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator).commitHead = start.commitHead)
    (active : ValidatorEpochActiveBetween timed.execution.trace start.startedAt
      (start.deadline waits)) :
    ValidatorRecoveryProposalReady faults timed waits start.commitHead
      start.targetRound start.validator := by
  have startBeforeDeadline : start.startedAt ≤ start.deadline waits := by
    simp [ValidatorRecoveryTimerStart.deadline]
  have activeAtDeadline :
      (timed.execution.trace (start.deadline waits)).epochActive = true :=
    active (start.deadline waits) startBeforeDeadline (Nat.le_refl _)
  have recoveryAtDeadline := source.timerPersistsUntilDeadline start
    (start.deadline waits) started startBeforeDeadline (Nat.le_refl _)
      headAtDeadline
  have exactNextAtDeadline := source.exactNextFloorPersistsUntilDeadline start
    (start.deadline waits) started startBeforeDeadline (Nat.le_refl _)
      headAtDeadline
  have refreshedParents := source.refreshed_parents_at_deadline start started
    headAtDeadline
  have clockAtDeadline := source.clock_reaches_deadline start started active
  have proposalProtected := source.expiredTimerProtectsRefreshedProposal start
    (start.deadline waits) started activeAtDeadline headAtDeadline
      recoveryAtDeadline exactNextAtDeadline clockAtDeadline refreshedParents
  refine
    { validatorInRange := source.validatorInRange start started
      validatorCorrectAvailable :=
        source.validatorCorrectAvailable start started
      parentReadyAt := start.parentReadyAt
      startedAt := start.startedAt
      highestObservedRound := start.highestObservedRound
      startsAfterParentReady := source.startsAfterParentsReady start started
      parents := source.refreshedParents start (start.deadline waits)
      recovery := start.recovery waits
      recoveryAtDeadline := ?_
      recoveryBaseline := rfl
      recoveryTarget := rfl
      recoveryParentsReady := rfl
      recoveryDeadline := rfl
      recoveryHasNoAlignment := rfl
      proposalProtected := ?_ }
  · simpa [ValidatorRecoveryTimerStart.deadline] using recoveryAtDeadline
  · simpa [ValidatorRecoveryTimerStart.deadline] using proposalProtected

/-- The proposition form of `recoveryProposalReadyOfTimerStart`. -/
theorem timer_start_derives_recovery_proposal_ready
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (started : source.timerStarted start)
    (headAtDeadline :
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator).commitHead = start.commitHead)
    (active : ValidatorEpochActiveBetween timed.execution.trace start.startedAt
      (start.deadline waits)) :
  Nonempty (ValidatorRecoveryProposalReady faults timed waits start.commitHead
      start.targetRound start.validator) :=
  ⟨source.recoveryProposalReadyOfTimerStart start started headAtDeadline active⟩

/-- One timer start derives the trace-pacing input and keeps the
recovery-origin retention fact needed by the proposal latch. -/
def originAwareReadyOfTimerStart
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (started : source.timerStarted start)
    (headAtDeadline :
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator).commitHead = start.commitHead)
    (active : ValidatorEpochActiveBetween timed.execution.trace start.startedAt
      (start.deadline waits)) :
    ValidatorOriginAwareRecoveryProposalReady faults timed waits
      start.commitHead start.targetRound start.validator := by
  let ready := source.recoveryProposalReadyOfTimerStart start started
    headAtDeadline active
  refine
    { ready := ready,
      startedWithinLocalBound := ?_,
      refreshedRecoveryParents := ?_,
      recoveryParentsReady := ?_ }
  · change start.startedAt ≤
      start.parentReadyAt + timed.localActionBound + 2
    exact source.startsWithinLocalBound start started
  · simpa [ready, recoveryProposalReadyOfTimerStart,
      ValidatorRecoveryTimerStart.deadline] using
        source.refreshed_parents_at_deadline start started headAtDeadline
  · simpa [ready, recoveryProposalReadyOfTimerStart,
      ValidatorRecoveryTimerStart.deadline] using
        source.recovery_parents_ready_at_deadline start started headAtDeadline

/-- A derived ready value keeps the observed timer start and uses the fresh
deadline parent list. -/
theorem timer_start_derives_ready_with_observed_fields
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (started : source.timerStarted start)
    (headAtDeadline :
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator).commitHead = start.commitHead)
    (active : ValidatorEpochActiveBetween timed.execution.trace start.startedAt
      (start.deadline waits)) :
    ∃ ready : ValidatorRecoveryProposalReady faults timed waits
        start.commitHead start.targetRound start.validator,
      ready.startedAt = start.startedAt ∧
        ready.parentReadyAt = start.parentReadyAt ∧
        ready.parents =
          source.refreshedParents start (start.deadline waits) := by
  let ready := source.recoveryProposalReadyOfTimerStart start started
    headAtDeadline active
  exact ⟨ready, rfl, rfl, rfl⟩

/-- A current exact stored timer derives its real start internally. It then
uses the same refreshed deadline selection as a newly armed timer. -/
theorem armed_exact_timer_and_active_suffix_derives_ready_or_commit_advance
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time validator : Time)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (armed : ValidatorArmedExactRecoveryTimerAt timed time validator)
    (activeSuffix : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true) :
    Nonempty (ValidatorOriginAwareRecoveryProposalReady faults timed waits
      ((timed.execution.trace time).validatorState validator).commitHead
      (((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1)
      validator) ∨
      (∃ changedAt,
        time ≤ changedAt ∧
          ((timed.execution.trace time).validatorState
              validator).commitHead.index <
            ((timed.execution.trace changedAt).validatorState
              validator).commitHead.index) := by
  rcases source.storedExactTimerHasSource time validator validatorInRange
      validatorCorrectAvailable armed with
    ⟨start, started, startValidator, startHead, startTarget,
      _stored, startBeforeTime, activeBefore⟩
  have _origin := arms.timerStartedHasOrigin start started
  have activeInterval : ValidatorEpochActiveBetween timed.execution.trace
      start.startedAt (start.deadline waits) := by
    intro later afterStart beforeDeadline
    by_cases beforeTime : later ≤ time
    · exact activeBefore later afterStart beforeTime
    · exact activeSuffix later (Nat.le_of_not_ge beforeTime)
  have startBeforeDeadline : start.startedAt ≤ start.deadline waits := by
    simp [ValidatorRecoveryTimerStart.deadline]
  by_cases deadlineBeforeTime : start.deadline waits ≤ time
  · have headAtTime :
        ((timed.execution.trace time).validatorState
          start.validator).commitHead = start.commitHead := by
      simp [startValidator, startHead]
    have headAtDeadline :=
      source.current_head_after_deadline_implies_head_at_deadline start started
        time deadlineBeforeTime headAtTime
    left
    let ready := source.originAwareReadyOfTimerStart start started
      headAtDeadline activeInterval
    simpa [startValidator, startHead, startTarget] using (show Nonempty _ from
      ⟨ready⟩)
  · by_cases headAtDeadline :
        ((timed.execution.trace (start.deadline waits)).validatorState
          start.validator).commitHead = start.commitHead
    · left
      let ready := source.originAwareReadyOfTimerStart start started
        headAtDeadline activeInterval
      simpa [startValidator, startHead, startTarget] using (show Nonempty _ from
        ⟨ready⟩)
    · right
      have timeBeforeDeadline : time ≤ start.deadline waits :=
        Nat.le_of_not_ge deadlineBeforeTime
      have monotone := timed.execution.durable_fields_persist
        (source.validatorInRange start started) startBeforeDeadline
      have headAtStart := source.commitHeadAtStart start started
      have indexLe : start.commitHead.index ≤
          ((timed.execution.trace (start.deadline waits)).validatorState
            start.validator).commitHead.index := by
        simpa [headAtStart] using monotone.1
      have indexNe : start.commitHead.index ≠
          ((timed.execution.trace (start.deadline waits)).validatorState
            start.validator).commitHead.index := by
        intro sameIndex
        have sameHead := monotone.2.2.1 (by
          simpa [headAtStart] using sameIndex)
        rw [headAtStart] at sameHead
        exact headAtDeadline sameHead.symm
      refine ⟨start.deadline waits, timeBeforeDeadline, ?_⟩
      simpa [startValidator, startHead] using
        Nat.lt_of_le_of_ne indexLe indexNe

/-- A selected goal can latch only when that validator's arm worker is empty. -/
theorem selected_goal_requires_empty_worker
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time : Time) (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId)
    (selected : arms.selectedGoal time goal.validator = some goal) :
    (arms.trace time goal.validator).pending = none := by
  have latched := arms.selectedGoalLatches time goal selected
  have step := arms.transitionsFollowRules time goal.validator
  rw [latched] at step
  cases step
  assumption

/-- Current exact-next recovery parents create durable arm work. Its bounded
completion either creates the exact timer or loses to local commit progress. -/
theorem selected_goal_completes_timer_or_commit_race
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time : Time) (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId)
    (selected : arms.selectedGoal time goal.validator = some goal) :
    ∃ completedAt,
      time + 1 ≤ completedAt ∧
        completedAt ≤ time + 1 + timed.localActionBound ∧
        arms.events completedAt goal.validator = .complete goal ∧
        (source.timerStarted (goal.toTimerStart (completedAt + 1)) ∨
          goal.commitHead.index <
            ((timed.execution.trace (completedAt + 1)).validatorState
              goal.validator).commitHead.index) := by
  have latched := arms.selectedGoalLatches time goal selected
  have protectedWork := arms.latchedGoalIsProtected time goal latched
  rcases arms.completeProtectedArmWork (time + 1) goal protectedWork with
    ⟨completedAt, startsAfterLatch, completionBound, completed⟩
  exact ⟨completedAt, startsAfterLatch, completionBound, completed,
    arms.completedArmStartsExactTimerOrCommitAdvances completedAt goal
      completed⟩

/-- One occupied goal has a valid latch or restored-state origin. -/
theorem pending_goal_has_valid_origin
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time validator : Time)
    (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId)
    (pending : (arms.trace time validator).pending = some goal) :
    (∃ latchedAt,
      latchedAt < time ∧
        arms.selectedGoal latchedAt goal.validator = some goal ∧
        arms.events latchedAt goal.validator = .latch goal ∧
        ValidatorRecoveryTimerArmEligibleAt timed latchedAt goal) ∨
      (arms.restoredArmGoal goal ∧
        ValidatorRecoveryTimerPendingReservationAt timed 0
          goal.validator goal) := by
  rcases arms.pendingGoalHasOrigin time validator goal pending with
      latched | restored
  · left
    rcases latched with ⟨latchedAt, beforeTime, selected, event⟩
    exact ⟨latchedAt, beforeTime, selected, event,
      arms.selectedGoalIsEligible latchedAt goal selected⟩
  · right
    refine ⟨restored, ?_⟩
    have pendingAtZero := arms.restoredArmGoalIsPendingAtZero goal restored
    exact arms.pendingGoalKeepsReservation 0 goal.validator goal pendingAtZero

/-- Occupied protected timer-arm work completes within the local bound. Its
completion creates the exact timer or loses to a local commit advance. -/
theorem pending_goal_completes_timer_or_commit_race
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time validator : Time)
    (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId)
    (pending : (arms.trace time validator).pending = some goal) :
    ∃ completedAt,
      time ≤ completedAt ∧
        completedAt ≤ time + timed.localActionBound ∧
        arms.events completedAt goal.validator = .complete goal ∧
        (source.timerStarted (goal.toTimerStart (completedAt + 1)) ∨
          goal.commitHead.index <
            ((timed.execution.trace (completedAt + 1)).validatorState
              goal.validator).commitHead.index) := by
  have protectedWork := arms.pendingGoalIsProtected time validator goal pending
  rcases arms.completeProtectedArmWork time goal protectedWork with
    ⟨completedAt, afterStart, completionBound, completed⟩
  exact ⟨completedAt, afterStart, completionBound, completed,
    arms.completedArmStartsExactTimerOrCommitAdvances completedAt goal
      completed⟩

/-- Every timer-start observation has one checked local source. -/
theorem timer_start_has_completion_or_valid_restore
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (start : ValidatorRecoveryTimerStart BlockId CommitId)
    (started : source.timerStarted start) :
    (∃ completedAt goal,
      arms.events completedAt goal.validator = .complete goal ∧
        start = goal.toTimerStart (completedAt + 1)) ∨
      ValidatorRestoredRecoveryTimerValidAtZero faults timed waits start := by
  rcases arms.timerStartedHasOrigin start started with completed | restored
  · exact Or.inl completed
  · exact Or.inr (arms.restoredTimerIsValid start restored)

/-- One exact completed timer, an active epoch suffix, and no assumed future
ready value derive the proposal branch or the installed-head race. -/
theorem completed_timer_and_active_suffix_derives_bounded_ready_or_commit_advance
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (time completedAt : Time)
    (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId)
    (timerStarted :
      source.timerStarted (goal.toTimerStart (completedAt + 1)))
    (timeBeforeStart : time ≤ completedAt + 1)
    (startBound :
      completedAt + 1 ≤ time + timed.localActionBound + 2)
    (activeSuffix : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true) :
    (∃ ready : ValidatorOriginAwareRecoveryProposalReady faults timed waits
        goal.commitHead goal.targetRound goal.validator,
      time ≤ ready.ready.startedAt ∧
        ready.ready.startedAt ≤ time + timed.localActionBound + 2 ∧
        ready.ready.parentReadyAt = goal.parentsReadyAt) ∨
      (∃ changedAt,
        time ≤ changedAt ∧
          goal.commitHead.index <
            ((timed.execution.trace changedAt).validatorState
              goal.validator).commitHead.index) := by
  let start := goal.toTimerStart (completedAt + 1)
  have timeBeforeStoredStart : time ≤ start.startedAt := by
    simpa [start, ValidatorRecoveryTimerArmGoal.toTimerStart] using
      timeBeforeStart
  have storedStartBound :
      start.startedAt ≤ time + timed.localActionBound + 2 := by
    simpa [start, ValidatorRecoveryTimerArmGoal.toTimerStart] using startBound
  have startBeforeDeadline : start.startedAt ≤ start.deadline waits := by
    simp [ValidatorRecoveryTimerStart.deadline]
  have activeInterval : ValidatorEpochActiveBetween timed.execution.trace
      start.startedAt (start.deadline waits) := by
    intro later afterStart _beforeDeadline
    exact activeSuffix later (Nat.le_trans timeBeforeStoredStart afterStart)
  by_cases headAtDeadline :
      ((timed.execution.trace (start.deadline waits)).validatorState
        start.validator).commitHead = start.commitHead
  · left
    let ready := source.originAwareReadyOfTimerStart start timerStarted
      headAtDeadline activeInterval
    refine ⟨?_, ?_, ?_, ?_⟩
    · simpa [ready, start, ValidatorRecoveryTimerArmGoal.toTimerStart] using
        ready
    · change time ≤ start.startedAt
      exact timeBeforeStoredStart
    · change start.startedAt ≤ time + timed.localActionBound + 2
      exact storedStartBound
    · rfl
  · right
    let changedAt := start.deadline waits
    have afterStart : start.startedAt ≤ changedAt := by
      simp [changedAt]
    have monotone := timed.execution.durable_fields_persist
      (source.validatorInRange start timerStarted) afterStart
    have headAtStart := source.commitHeadAtStart start timerStarted
    have indexLe : start.commitHead.index ≤
        ((timed.execution.trace changedAt).validatorState
          start.validator).commitHead.index := by
      simpa [headAtStart] using monotone.1
    have indexNe : start.commitHead.index ≠
        ((timed.execution.trace changedAt).validatorState
          start.validator).commitHead.index := by
      intro sameIndex
      have sameHeadAtTimes := monotone.2.2.1 (by
        simpa [headAtStart] using sameIndex)
      have changedHeadIsStart :
          ((timed.execution.trace changedAt).validatorState
            start.validator).commitHead = start.commitHead := by
        rw [headAtStart] at sameHeadAtTimes
        exact sameHeadAtTimes.symm
      exact headAtDeadline changedHeadIsStart
    refine ⟨changedAt,
      Nat.le_trans timeBeforeStoredStart afterStart, ?_⟩
    simpa [start, ValidatorRecoveryTimerArmGoal.toTimerStart] using
      Nat.lt_of_le_of_ne indexLe indexNe

/-- A newly selected empty-worker goal gives a bounded exact timer proposal or
a strict commit advance. -/
theorem selected_goal_and_active_suffix_derives_bounded_ready_or_commit_advance
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time : Time) (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId)
    (selected : arms.selectedGoal time goal.validator = some goal)
    (activeSuffix : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true) :
    (∃ ready : ValidatorOriginAwareRecoveryProposalReady faults timed waits
        goal.commitHead goal.targetRound goal.validator,
      time ≤ ready.ready.startedAt ∧
        ready.ready.startedAt ≤ time + timed.localActionBound + 2 ∧
        ready.ready.parentReadyAt = goal.parentsReadyAt) ∨
      (∃ changedAt,
        time ≤ changedAt ∧
          goal.commitHead.index <
            ((timed.execution.trace changedAt).validatorState
              goal.validator).commitHead.index) := by
  rcases source.selected_goal_completes_timer_or_commit_race arms time goal
      selected with
    ⟨completedAt, afterLatch, completionBound, _completed,
      timerStarted | commitAdvanced⟩
  · have timeBeforeStart : time ≤ completedAt + 1 := by
      exact Nat.le_trans (Nat.le_add_right _ 1)
        (Nat.le_trans afterLatch (Nat.le_add_right _ 1))
    have startBound :
        completedAt + 1 ≤ time + timed.localActionBound + 2 := by
      have withVisibility := Nat.add_le_add_right completionBound 1
      exact Nat.le_trans withVisibility (by omega)
    exact source.completed_timer_and_active_suffix_derives_bounded_ready_or_commit_advance
      time completedAt goal timerStarted timeBeforeStart startBound activeSuffix
  · right
    refine ⟨completedAt + 1, ?_, commitAdvanced⟩
    exact Nat.le_trans (Nat.le_add_right _ 1)
      (Nat.le_trans afterLatch (Nat.le_add_right _ 1))

/-- An already occupied worker gives the same bounded timer result. The
occupied state supplies protected work, its valid origin, and its exact
head/signer-floor reservation. -/
theorem pending_goal_and_active_suffix_derives_bounded_ready_or_commit_advance
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time validator : Time)
    (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId)
    (pending : (arms.trace time validator).pending = some goal)
    (activeSuffix : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true) :
    (∃ ready : ValidatorOriginAwareRecoveryProposalReady faults timed waits
        goal.commitHead goal.targetRound goal.validator,
      time ≤ ready.ready.startedAt ∧
        ready.ready.startedAt ≤ time + timed.localActionBound + 2 ∧
        ready.ready.parentReadyAt = goal.parentsReadyAt) ∨
      (∃ changedAt,
        time ≤ changedAt ∧
          goal.commitHead.index <
            ((timed.execution.trace changedAt).validatorState
              goal.validator).commitHead.index) := by
  have _origin := source.pending_goal_has_valid_origin arms time validator goal
    pending
  rcases source.pending_goal_completes_timer_or_commit_race arms time validator
      goal pending with
    ⟨completedAt, afterStart, completionBound, _completed,
      timerStarted | commitAdvanced⟩
  · have timeBeforeStart : time ≤ completedAt + 1 :=
      Nat.le_trans afterStart (Nat.le_add_right _ 1)
    have startBound :
        completedAt + 1 ≤ time + timed.localActionBound + 2 := by
      have withVisibility := Nat.add_le_add_right completionBound 1
      exact Nat.le_trans withVisibility (by omega)
    exact source.completed_timer_and_active_suffix_derives_bounded_ready_or_commit_advance
      time completedAt goal timerStarted timeBeforeStart startBound activeSuffix
  · right
    refine ⟨completedAt + 1,
      Nat.le_trans afterStart (Nat.le_add_right _ 1), commitAdvanced⟩

/-- Canonical timer bridge for an available or occupied worker.

Current recovery parents and an active epoch suffix derive the exact next
timer-paced ready value or a strict local commit advance. The caller does not
need an empty-worker, selected-goal, timer-start, or ready-value witness.
-/
theorem recovery_state_and_active_suffix_derives_exact_ready_or_commit_advance
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time validator : Time)
    (input : ValidatorRecoveryTimerArmInputAt timed time validator)
    (activeSuffix : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true) :
    (∃ ready : ValidatorOriginAwareRecoveryProposalReady faults timed waits
        ((timed.execution.trace time).validatorState validator).commitHead
        (((timed.execution.trace time).validatorState
          validator).highestSignedRound + 1)
        validator,
      time ≤ ready.ready.startedAt ∧
        ready.ready.startedAt ≤ time + timed.localActionBound + 2) ∨
      (∃ changedAt,
        time ≤ changedAt ∧
          ((timed.execution.trace time).validatorState
              validator).commitHead.index <
            ((timed.execution.trace changedAt).validatorState
              validator).commitHead.index) := by
  cases pendingState : (arms.trace time validator).pending with
  | none =>
      rcases arms.readyStateSelectsGoal time validator input pendingState with
        ⟨goal, selected, goalValidator, goalHead, goalTarget, _readyAt,
          _eligible⟩
      subst validator
      rw [← goalHead, ← goalTarget]
      rcases source.selected_goal_and_active_suffix_derives_bounded_ready_or_commit_advance
          arms time goal selected activeSuffix with ready | commitAdvance
      · left
        rcases ready with ⟨originReady, afterTime, startBound, _parentTime⟩
        exact ⟨originReady, afterTime, startBound⟩
      · exact Or.inr commitAdvance
  | some goal =>
      have reservation := arms.pendingGoalKeepsReservation time validator goal
        pendingState
      have goalValidator := reservation.goalValidator
      subst validator
      rw [reservation.commitHeadCurrent, ← reservation.targetIsExactNext]
      rcases source.pending_goal_and_active_suffix_derives_bounded_ready_or_commit_advance
          arms time goal.validator goal pendingState activeSuffix with
        ready | commitAdvance
      · left
        rcases ready with ⟨originReady, afterTime, startBound, _parentTime⟩
        exact ⟨originReady, afterTime, startBound⟩
      · exact Or.inr commitAdvance

/-- Canonical current-state bridge for a free arm worker or an exact timer that
is already stored. No timer-start or ready-value witness is an input. -/
theorem current_timer_input_and_active_suffix_derives_ready_or_commit_advance
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time validator : Time)
    (input : ValidatorRecoveryTimerCurrentInputAt timed time validator)
    (activeSuffix : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true) :
    Nonempty (ValidatorOriginAwareRecoveryProposalReady faults timed waits
      ((timed.execution.trace time).validatorState validator).commitHead
      (((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1)
      validator) ∨
      (∃ changedAt,
        time ≤ changedAt ∧
          ((timed.execution.trace time).validatorState
              validator).commitHead.index <
            ((timed.execution.trace changedAt).validatorState
              validator).commitHead.index) := by
  rcases input with armInput |
      ⟨_active, validatorInRange, validatorCorrectAvailable, armed⟩
  · rcases source.recovery_state_and_active_suffix_derives_exact_ready_or_commit_advance
        arms time validator armInput activeSuffix with ready | commitAdvance
    · rcases ready with ⟨originReady, _afterTime, _startBound⟩
      exact Or.inl ⟨originReady⟩
    · exact Or.inr commitAdvance
  · exact source.armed_exact_timer_and_active_suffix_derives_ready_or_commit_advance
      arms time validator validatorInRange validatorCorrectAvailable armed
      activeSuffix

/-- The canonical no-occupancy bridge with the correct parent-ready time
relation. A newly selected goal has `parentReadyAt = time`. An occupied goal
can predate `time`, but its parent-ready time is still within one local arm
envelope behind `time`. -/
theorem recovery_state_and_active_suffix_derives_ready_with_parent_time_or_commit_advance
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time validator : Time)
    (input : ValidatorRecoveryTimerArmInputAt timed time validator)
    (activeSuffix : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true) :
    (∃ ready : ValidatorOriginAwareRecoveryProposalReady faults timed waits
        ((timed.execution.trace time).validatorState validator).commitHead
        (((timed.execution.trace time).validatorState
          validator).highestSignedRound + 1)
        validator,
      ready.ready.parentReadyAt ≤ time ∧
        time ≤ ready.ready.startedAt ∧
        ready.ready.startedAt ≤ time + timed.localActionBound + 2 ∧
        time ≤ ready.ready.parentReadyAt + timed.localActionBound + 2) ∨
      (∃ changedAt,
        time ≤ changedAt ∧
          ((timed.execution.trace time).validatorState
              validator).commitHead.index <
            ((timed.execution.trace changedAt).validatorState
              validator).commitHead.index) := by
  cases pendingState : (arms.trace time validator).pending with
  | none =>
      rcases arms.readyStateSelectsGoal time validator input pendingState with
        ⟨goal, selected, goalValidator, goalHead, goalTarget, goalReadyAt,
          _eligible⟩
      subst validator
      rw [← goalHead, ← goalTarget]
      rcases source.selected_goal_and_active_suffix_derives_bounded_ready_or_commit_advance
          arms time goal selected activeSuffix with ready | commitAdvance
      · left
        rcases ready with
          ⟨originReady, afterTime, startBound, parentReadyAt⟩
        have parentBefore : originReady.ready.parentReadyAt ≤ time := by
          rw [parentReadyAt, goalReadyAt]
          exact Nat.le_refl _
        have parentWindow :
            time ≤ originReady.ready.parentReadyAt +
              timed.localActionBound + 2 :=
          Nat.le_trans afterTime originReady.startedWithinLocalBound
        exact ⟨originReady, parentBefore, afterTime, startBound, parentWindow⟩
      · exact Or.inr commitAdvance
  | some goal =>
      have reservation := arms.pendingGoalKeepsReservation time validator goal
        pendingState
      have goalValidator := reservation.goalValidator
      subst validator
      rw [reservation.commitHeadCurrent, ← reservation.targetIsExactNext]
      rcases source.pending_goal_and_active_suffix_derives_bounded_ready_or_commit_advance
          arms time goal.validator goal pendingState activeSuffix with
        ready | commitAdvance
      · left
        rcases ready with
          ⟨originReady, afterTime, startBound, parentReadyAt⟩
        have parentBefore :
            originReady.ready.parentReadyAt ≤ time := by
          rw [parentReadyAt]
          exact reservation.parentsReadyAtReached
        have parentWindow :
            time ≤ originReady.ready.parentReadyAt +
              timed.localActionBound + 2 :=
          Nat.le_trans afterTime originReady.startedWithinLocalBound
        exact ⟨originReady, parentBefore, afterTime, startBound, parentWindow⟩
      · exact Or.inr commitAdvance

/-- In the stalled-head branch, the available-or-occupied worker bridge gives
the exact next timer-paced proposal. -/
theorem recovery_state_and_stable_head_derives_exact_next_ready
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time validator : Time)
    (input : ValidatorRecoveryTimerArmInputAt timed time validator)
    (activeSuffix : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true)
    (headStableSuffix : ∀ later, time ≤ later →
      ((timed.execution.trace later).validatorState validator).commitHead =
        ((timed.execution.trace time).validatorState validator).commitHead) :
    ∃ ready : ValidatorOriginAwareRecoveryProposalReady faults timed waits
        ((timed.execution.trace time).validatorState validator).commitHead
        (((timed.execution.trace time).validatorState
          validator).highestSignedRound + 1)
        validator,
      time ≤ ready.ready.startedAt ∧
        ready.ready.startedAt ≤ time + timed.localActionBound + 2 := by
  rcases source.recovery_state_and_active_suffix_derives_exact_ready_or_commit_advance
      arms time validator input activeSuffix with ready | commitAdvance
  · exact ready
  · rcases commitAdvance with ⟨changedAt, afterTime, advanced⟩
    have stable := headStableSuffix changedAt afterTime
    rw [stable] at advanced
    omega

/-- Current same-validator state selects one canonical retained parent list and
starts its exact timer. No timer-start witness is an input. -/
theorem ready_state_selects_and_completes_timer_or_commit_race
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time : Time) (validator : Nat)
    (input : ValidatorRecoveryTimerArmInputAt timed time validator)
    (armEmpty : (arms.trace time validator).pending = none) :
    ∃ goal completedAt,
      arms.selectedGoal time validator = some goal ∧
        goal.validator = validator ∧
        ValidatorRecoveryTimerArmEligibleAt timed time goal ∧
        time + 1 ≤ completedAt ∧
        completedAt ≤ time + 1 + timed.localActionBound ∧
        arms.events completedAt validator = .complete goal ∧
        (source.timerStarted (goal.toTimerStart (completedAt + 1)) ∨
          goal.commitHead.index <
            ((timed.execution.trace (completedAt + 1)).validatorState
              validator).commitHead.index) := by
  rcases arms.readyStateSelectsGoal time validator input armEmpty with
    ⟨goal, selected, goalValidator, _goalHead, _goalTarget,
      _goalReadyAt, eligible⟩
  have selectedAtGoal :
      arms.selectedGoal time goal.validator = some goal := by
    simpa [goalValidator] using selected
  rcases source.selected_goal_completes_timer_or_commit_race arms time goal
      selectedAtGoal with
    ⟨completedAt, afterLatch, completionBound, completed, result⟩
  refine ⟨goal, completedAt, selected, goalValidator, eligible, afterLatch,
    completionBound, ?_, ?_⟩
  · simpa [goalValidator] using completed
  · rcases result with timerStarted | commitAdvanced
    · exact Or.inl timerStarted
    · exact Or.inr (by simpa [goalValidator] using commitAdvanced)

/-- Eligible recovery parents derive either one origin-aware proposal timer or
a local commit-index advance that won the race before its deadline. The epoch
activity premise is applied only after the timer start is derived. -/
theorem selected_goal_derives_ready_or_commit_race
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time : Time) (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId)
    (selected : arms.selectedGoal time goal.validator = some goal) :
    ∃ completedAt,
      time + 1 ≤ completedAt ∧
        completedAt ≤ time + 1 + timed.localActionBound ∧
        arms.events completedAt goal.validator = .complete goal ∧
        (goal.commitHead.index <
            ((timed.execution.trace (completedAt + 1)).validatorState
              goal.validator).commitHead.index ∨
          let start := goal.toTimerStart (completedAt + 1)
          source.timerStarted start ∧
            (ValidatorEpochActiveBetween timed.execution.trace start.startedAt
                (start.deadline waits) →
              Nonempty (ValidatorOriginAwareRecoveryProposalReady faults timed
                  waits start.commitHead start.targetRound start.validator) ∨
                ValidatorRecoveryTimerHeadRace source start)) := by
  rcases source.selected_goal_completes_timer_or_commit_race arms time goal
      selected with
    ⟨completedAt, afterLatch, completionBound, completed, completionResult⟩
  refine ⟨completedAt, afterLatch, completionBound, completed, ?_⟩
  rcases completionResult with commitTimerStarted | commitAdvanced
  · right
    refine ⟨commitTimerStarted, ?_⟩
    intro active
    let start := goal.toTimerStart (completedAt + 1)
    by_cases headAtDeadline :
        ((timed.execution.trace (start.deadline waits)).validatorState
          start.validator).commitHead = start.commitHead
    · exact Or.inl ⟨source.originAwareReadyOfTimerStart start
        commitTimerStarted headAtDeadline active⟩
    · right
      let changedAt := start.deadline waits
      have afterStart : start.startedAt ≤ changedAt := by
        simp [changedAt]
      have beforeDeadline : changedAt ≤ start.deadline waits := by
        exact Nat.le_refl _
      have monotone := timed.execution.durable_fields_persist
        (source.validatorInRange start commitTimerStarted) afterStart
      have headAtStart := source.commitHeadAtStart start commitTimerStarted
      have indexLe : start.commitHead.index ≤
          ((timed.execution.trace changedAt).validatorState
            start.validator).commitHead.index := by
        simpa [headAtStart] using monotone.1
      have indexNe : start.commitHead.index ≠
          ((timed.execution.trace changedAt).validatorState
            start.validator).commitHead.index := by
        intro sameIndex
        have sameHeadAtTimes := monotone.2.2.1 (by
          simpa [headAtStart] using sameIndex)
        have changedHeadIsStart :
            ((timed.execution.trace changedAt).validatorState
              start.validator).commitHead = start.commitHead := by
          rw [headAtStart] at sameHeadAtTimes
          exact sameHeadAtTimes.symm
        exact headAtDeadline changedHeadIsStart
      exact ⟨changedAt, afterStart, beforeDeadline,
        Nat.lt_of_le_of_ne indexLe indexNe⟩
  · exact Or.inl commitAdvanced

/-- Fundamental current local state derives the exact timer branch. The result
contains the canonical selected parents, the bounded timer start, and the
stable-head proposal or advanced-head race. -/
theorem ready_state_derives_ready_or_commit_race
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time : Time) (validator : Nat)
    (input : ValidatorRecoveryTimerArmInputAt timed time validator)
    (armEmpty : (arms.trace time validator).pending = none) :
    ∃ goal completedAt,
      arms.selectedGoal time validator = some goal ∧
        goal.validator = validator ∧
        ValidatorRecoveryTimerArmEligibleAt timed time goal ∧
        time + 1 ≤ completedAt ∧
        completedAt ≤ time + 1 + timed.localActionBound ∧
        arms.events completedAt validator = .complete goal ∧
        (goal.commitHead.index <
            ((timed.execution.trace (completedAt + 1)).validatorState
              validator).commitHead.index ∨
          let start := goal.toTimerStart (completedAt + 1)
          source.timerStarted start ∧
            (ValidatorEpochActiveBetween timed.execution.trace start.startedAt
                (start.deadline waits) →
              Nonempty (ValidatorOriginAwareRecoveryProposalReady faults timed
                  waits start.commitHead start.targetRound start.validator) ∨
                ValidatorRecoveryTimerHeadRace source start)) := by
  rcases arms.readyStateSelectsGoal time validator input armEmpty with
    ⟨goal, selected, goalValidator, _goalHead, _goalTarget,
      _goalReadyAt, eligible⟩
  have selectedAtGoal :
      arms.selectedGoal time goal.validator = some goal := by
    simpa [goalValidator] using selected
  rcases source.selected_goal_derives_ready_or_commit_race arms time goal
      selectedAtGoal with
    ⟨completedAt', afterLatch', completionBound', completed',
      result⟩
  exact ⟨goal, completedAt', selected, goalValidator, eligible, afterLatch',
    completionBound', by simpa [goalValidator] using completed',
    by rcases result with commitAdvanced | timerResult
       · exact Or.inl (by simpa [goalValidator] using commitAdvanced)
       · exact Or.inr timerResult⟩

/-- With an active epoch suffix, current local parent readiness gives an
origin-aware trace timer or a strict local commit-index advance. This public
bridge has no ready-value or timer-start input. -/
theorem ready_state_and_active_suffix_derives_ready_or_commit_advance
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time : Time) (validator : Nat)
    (input : ValidatorRecoveryTimerArmInputAt timed time validator)
    (armEmpty : (arms.trace time validator).pending = none)
    (activeSuffix : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true) :
    (∃ commitHead targetRound,
      Nonempty (ValidatorOriginAwareRecoveryProposalReady faults timed waits
        commitHead targetRound validator)) ∨
    (∃ changedAt,
      time ≤ changedAt ∧
        ((timed.execution.trace time).validatorState
            validator).commitHead.index <
          ((timed.execution.trace changedAt).validatorState
            validator).commitHead.index) := by
  rcases source.ready_state_derives_ready_or_commit_race arms time validator
      input armEmpty with
    ⟨goal, completedAt, _selected, goalValidator, eligible, afterLatch,
      _completionBound, _completed, result⟩
  have goalHeadAtTime :
      ((timed.execution.trace time).validatorState
        validator).commitHead = goal.commitHead := by
    simpa [goalValidator] using eligible.2.2.2.2.1
  rcases result with armCommitAdvanced | timerResult
  · right
    refine ⟨completedAt + 1, ?_, ?_⟩
    · exact Nat.le_trans (Nat.le_add_right _ 1)
        (Nat.le_trans afterLatch (Nat.le_add_right _ 1))
    · rw [goalHeadAtTime]
      exact armCommitAdvanced
  · rcases timerResult with ⟨timerStarted, continuation⟩
    let start := goal.toTimerStart (completedAt + 1)
    have readyTimeBeforeStart : time ≤ start.startedAt := by
      dsimp [start, ValidatorRecoveryTimerArmGoal.toTimerStart]
      exact Nat.le_trans (Nat.le_add_right _ 1)
        (Nat.le_trans afterLatch (Nat.le_add_right _ 1))
    have activeInterval : ValidatorEpochActiveBetween timed.execution.trace
        start.startedAt (start.deadline waits) := by
      intro later afterStart _beforeDeadline
      exact activeSuffix later (Nat.le_trans readyTimeBeforeStart afterStart)
    rcases continuation activeInterval with ready | deadlineRace
    · left
      refine ⟨start.commitHead, start.targetRound, ?_⟩
      simpa [start, goalValidator,
        ValidatorRecoveryTimerArmGoal.toTimerStart] using ready
    · right
      rcases deadlineRace with
        ⟨changedAt, afterStart, _beforeDeadline, commitAdvanced⟩
      refine ⟨changedAt,
        Nat.le_trans readyTimeBeforeStart afterStart, ?_⟩
      rw [goalHeadAtTime]
      simpa [start, goalValidator,
        ValidatorRecoveryTimerArmGoal.toTimerStart] using
        commitAdvanced

/-- In the stalled local-head branch, current frontier parents derive one
timer-paced origin-aware proposal for the exact next signer round. -/
theorem ready_state_and_stable_head_derives_exact_next_ready
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (time : Time) (validator : Nat)
    (input : ValidatorRecoveryTimerArmInputAt timed time validator)
    (armEmpty : (arms.trace time validator).pending = none)
    (activeSuffix : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true)
    (headStableSuffix : ∀ later, time ≤ later →
      ((timed.execution.trace later).validatorState validator).commitHead =
        ((timed.execution.trace time).validatorState validator).commitHead) :
    ∃ ready : ValidatorOriginAwareRecoveryProposalReady faults timed waits
        ((timed.execution.trace time).validatorState validator).commitHead
        (((timed.execution.trace time).validatorState
          validator).highestSignedRound + 1)
        validator,
      time ≤ ready.ready.startedAt ∧
        ready.ready.startedAt ≤ time + timed.localActionBound + 2 := by
  rcases arms.readyStateSelectsGoal time validator input armEmpty with
    ⟨goal, selected, goalValidator, goalHead, goalTarget, _goalReadyAt,
      _eligible⟩
  subst validator
  rw [← goalHead, ← goalTarget]
  have selectedAtGoal :
      arms.selectedGoal time goal.validator = some goal := by
    exact selected
  rcases source.selected_goal_completes_timer_or_commit_race arms time goal
      selectedAtGoal with
    ⟨completedAt, afterLatch, completionBound, _completed,
      timerStarted | commitAdvanced⟩
  · let start := goal.toTimerStart (completedAt + 1)
    have readyTimeBeforeStart : time ≤ start.startedAt := by
      dsimp [start, ValidatorRecoveryTimerArmGoal.toTimerStart]
      exact Nat.le_trans (Nat.le_add_right _ 1)
        (Nat.le_trans afterLatch (Nat.le_add_right _ 1))
    have startBound :
        start.startedAt ≤ time + timed.localActionBound + 2 := by
      dsimp [start, ValidatorRecoveryTimerArmGoal.toTimerStart]
      have withVisibility := Nat.add_le_add_right completionBound 1
      exact Nat.le_trans withVisibility (by omega)
    have startBeforeDeadline : start.startedAt ≤ start.deadline waits := by
      simp [ValidatorRecoveryTimerStart.deadline]
    have timeBeforeDeadline : time ≤ start.deadline waits :=
      Nat.le_trans readyTimeBeforeStart startBeforeDeadline
    have headAtDeadline :
        ((timed.execution.trace (start.deadline waits)).validatorState
          start.validator).commitHead = start.commitHead := by
      have stable := headStableSuffix (start.deadline waits) timeBeforeDeadline
      simpa [start, goalHead,
        ValidatorRecoveryTimerArmGoal.toTimerStart] using stable
    have activeInterval : ValidatorEpochActiveBetween timed.execution.trace
        start.startedAt (start.deadline waits) := by
      intro later afterStart _beforeDeadline
      exact activeSuffix later (Nat.le_trans readyTimeBeforeStart afterStart)
    let originReady := source.originAwareReadyOfTimerStart start timerStarted
      headAtDeadline activeInterval
    have readyStart : originReady.ready.startedAt = start.startedAt := by
      rfl
    refine ⟨originReady, ?_, ?_⟩
    · rw [readyStart]
      exact readyTimeBeforeStart
    · rw [readyStart]
      exact startBound
  · have finishAfterTime : time ≤ completedAt + 1 := by
      exact Nat.le_trans (Nat.le_add_right _ 1)
        (Nat.le_trans afterLatch (Nat.le_add_right _ 1))
    have stable := headStableSuffix (completedAt + 1) finishAfterTime
    have sameHeadAtFinish :
        ((timed.execution.trace (completedAt + 1)).validatorState
          goal.validator).commitHead = goal.commitHead := by
      simpa [goalHead] using stable
    rw [sameHeadAtFinish] at commitAdvanced
    omega

/-- One common parent-ready time gives every member a finite ready-start
envelope, unless that member installs a newer commit head. Worker occupancy is
not an input. -/
theorem common_recovery_state_gives_ready_envelope_or_commit_advance
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    {Member : Nat → Prop}
    (readyAt : Time)
    (input : ∀ validator, Member validator →
      ValidatorRecoveryTimerArmInputAt timed readyAt validator)
    (activeSuffix : ∀ later, readyAt ≤ later →
      (timed.execution.trace later).epochActive = true) :
    ∀ validator, Member validator →
      (∃ ready : ValidatorOriginAwareRecoveryProposalReady faults timed waits
          ((timed.execution.trace readyAt).validatorState validator).commitHead
          (((timed.execution.trace readyAt).validatorState
            validator).highestSignedRound + 1)
          validator,
        readyAt ≤ ready.ready.startedAt ∧
          ready.ready.startedAt ≤
            readyAt + timed.localActionBound + 2) ∨
        (∃ changedAt,
          readyAt ≤ changedAt ∧
            ((timed.execution.trace readyAt).validatorState
                validator).commitHead.index <
              ((timed.execution.trace changedAt).validatorState
                validator).commitHead.index) := by
  intro validator member
  exact source.recovery_state_and_active_suffix_derives_exact_ready_or_commit_advance
    arms readyAt validator (input validator member) activeSuffix

/-- Any two ready starts derived from the same common state differ by no more
than the local timer-arm envelope. -/
theorem common_ready_start_pair_difference_is_bounded
    (_source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    {Member : Nat → Prop}
    (readyAt first second : Time)
    (startFor : Nat → Time)
    (lower : ∀ validator, Member validator →
      readyAt ≤ startFor validator)
    (upper : ∀ validator, Member validator →
      startFor validator ≤ readyAt + timed.localActionBound + 2)
    (firstMember : Member first) (secondMember : Member second) :
    startFor first ≤ startFor second + timed.localActionBound + 2 := by
  exact Nat.le_trans (upper first firstMember) (by
    simpa [Nat.add_assoc] using
      Nat.add_le_add_right (lower second secondMember)
        (timed.localActionBound + 2))

/-- One common accepted-parent state gives all correct members a finite timer
start envelope. This theorem takes no timer, selected-goal, or parent-spread
witness. -/
theorem common_ready_state_gives_timer_start_envelope
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    {Member : Nat → Prop}
    (readyAt : Time)
    (input : ∀ validator, Member validator →
      ValidatorRecoveryTimerArmInputAt timed readyAt validator)
    (armEmpty : ∀ validator, Member validator →
      (arms.trace readyAt validator).pending = none) :
    ∀ validator, Member validator →
      (∃ (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId)
          (completedAt : Time),
        completedAt ≤ readyAt + 1 + timed.localActionBound ∧
          goal.validator = validator ∧
          goal.commitHead.index <
            ((timed.execution.trace (completedAt + 1)).validatorState
              validator).commitHead.index) ∨
      (∃ start : ValidatorRecoveryTimerStart BlockId CommitId,
        source.timerStarted start ∧
          start.validator = validator ∧
          readyAt ≤ start.startedAt ∧
          start.startedAt ≤ readyAt + timed.localActionBound + 2) := by
  intro validator member
  rcases source.ready_state_selects_and_completes_timer_or_commit_race arms
      readyAt validator (input validator member) (armEmpty validator member) with
    ⟨goal, completedAt, _selected, goalValidator, _eligible, afterLatch,
      completionBound, _completed, result⟩
  rcases result with timerStarted | commitAdvanced
  · right
    let start := goal.toTimerStart (completedAt + 1)
    refine ⟨start, timerStarted, goalValidator, ?_, ?_⟩
    · dsimp [start, ValidatorRecoveryTimerArmGoal.toTimerStart]
      exact Nat.le_trans (Nat.le_add_right _ 1)
        (Nat.le_trans afterLatch (Nat.le_add_right _ 1))
    · dsimp [start, ValidatorRecoveryTimerArmGoal.toTimerStart]
      have withVisibility := Nat.add_le_add_right completionBound 1
      exact Nat.le_trans withVisibility (by omega)
  · left
    exact ⟨goal, completedAt, completionBound, goalValidator,
      commitAdvanced⟩

/-- A common parent-ready envelope and bounded local timer-arm work give a
finite common envelope for all matching timer starts. -/
theorem timer_starts_have_finite_envelope
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    {Member : Nat → Prop}
    (startFor : Nat → ValidatorRecoveryTimerStart BlockId CommitId)
    (earliestParentReady parentReadySpread : Nat)
    (started : ∀ validator, Member validator →
      source.timerStarted (startFor validator))
    (parentReadyLower : ∀ validator, Member validator →
      earliestParentReady ≤ (startFor validator).parentReadyAt)
    (parentReadyUpper : ∀ validator, Member validator →
      (startFor validator).parentReadyAt ≤
        earliestParentReady + parentReadySpread) :
    ∀ validator, Member validator →
      earliestParentReady ≤ (startFor validator).startedAt ∧
        (startFor validator).startedAt ≤
          earliestParentReady +
            (parentReadySpread + timed.localActionBound + 2) := by
  intro validator member
  have timerStarted := started validator member
  constructor
  · exact Nat.le_trans (parentReadyLower validator member)
      (source.startsAfterParentsReady _ timerStarted)
  · have startBound := source.startsWithinLocalBound _ timerStarted
    have readyBound := parentReadyUpper validator member
    exact Nat.le_trans startBound (by
      simpa [Nat.add_assoc] using
        Nat.add_le_add_right readyBound (timed.localActionBound + 2))

/-- Any two starts in the common envelope differ by no more than its finite
spread. -/
theorem timer_start_pair_difference_is_bounded
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    {Member : Nat → Prop}
    (startFor : Nat → ValidatorRecoveryTimerStart BlockId CommitId)
    (earliestParentReady parentReadySpread first second : Nat)
    (started : ∀ validator, Member validator →
      source.timerStarted (startFor validator))
    (parentReadyLower : ∀ validator, Member validator →
      earliestParentReady ≤ (startFor validator).parentReadyAt)
    (parentReadyUpper : ∀ validator, Member validator →
      (startFor validator).parentReadyAt ≤
        earliestParentReady + parentReadySpread)
    (firstMember : Member first) (secondMember : Member second) :
    (startFor first).startedAt ≤
      (startFor second).startedAt +
        (parentReadySpread + timed.localActionBound + 2) := by
  have envelope := source.timer_starts_have_finite_envelope startFor
    earliestParentReady parentReadySpread started parentReadyLower
      parentReadyUpper
  have firstUpper := (envelope first firstMember).2
  have secondLower := (envelope second secondMember).1
  exact Nat.le_trans firstUpper (by
    simpa [Nat.add_assoc] using Nat.add_le_add_right secondLower
      (parentReadySpread + timed.localActionBound + 2))

end ValidatorRecoveryTimerSourceMap

/-- Top-level name for the isolated timer-arm execution interface. -/
abbrev ValidatorRecoveryTimerArmExecution
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
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits) :=
  ValidatorRecoveryTimerSourceMap.ValidatorRecoveryTimerArmExecution source

end Mysticeti
