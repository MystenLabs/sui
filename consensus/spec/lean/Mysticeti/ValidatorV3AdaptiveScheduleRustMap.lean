/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.CommittedFlushCausalReadQuality
import Mysticeti.ValidatorV3AdaptiveScheduleSafety

namespace Mysticeti

/-! The Rust mapping for adaptive schedule replay.

`ValidatorV3AdaptiveScheduleSafety` proves that two correct installed heads at
one commit index replay to the same abstract schedule state. This module builds
the concrete replay that `LeaderScheduleV3` runs, so that result speaks about
the vectors the product reads.

The mapping stays at the level the result needs. The scoring calculation itself
is one deterministic function of the four committed materials; the model does not
reproduce its arithmetic. What matters for a shared schedule is that the input is
fixed by the exact commit reference, that the window bookkeeping is exact, and
that the readers take their values from the replayed state.

The mapping covers four Rust steps.

* `performanceFor` becomes the exact committed material of one commit: its
  index, its commit digest, its named leader, and its sorted committed block
  bodies. `FlexCommitter::build_commit` produces that material, and
  `CommitMaterializerView.buildCommit_materializes_exactly` fixes the block set
  from the exact commit reference.
* `add_commit` keeps the three-deep pending window and scores `C-3` against
  `[C-2, C-1, C]`.
* The running per-authority totals move by `checked_add` and `checked_sub` over
  a sliding entry window. `V3ScheduleState.addCommit_sound` proves that these
  totals stay equal to a recomputation over the retained entries, so no
  subtraction underflows.
* `refresh_current_schedule` recomputes `allowed_leaders` only on an
  update-interval boundary, and `select_allowed_leaders` shuffles with a seed
  taken from the commit digest of the last pending commit. The replayed state
  therefore carries the allowed-leader vector, the next commit index, and the
  minimum next leader round that the proposer and the FlexCommitter read.

The scorer reads the `CommittedSubDag` that the materializer built, so its block
vector is already sorted. Two correct hosts therefore give the scorer the same
list, not only the same set. `V3ScheduleRustSourceMap.localMaterial_agrees`
derives that from the walk and the sort, and the scoring rule is a function, so
equal inputs give equal score entries.
-/

/-- The exact committed material of one commit that the Rust scorer reads. -/
structure V3CommitMaterial (BlockId CommitId : Type) where
  index : Nat
  /-- The commit digest. `select_allowed_leaders` seeds its shuffle with the
  digest of the last pending commit. -/
  commitId : CommitId
  leader : ValidatorBlockRef BlockId
  /-- The sorted committed block vector of the `CommittedSubDag`. -/
  blocks : List (ValidatorBlock BlockId)

/-- One scored commit in the running window, as in Rust `ScoresEntry`. -/
structure V3ScoresEntry where
  contributions : Nat → Nat
  leaderCount : Nat
  leaderStakes : Nat

/-- Sum one measure over a window of scored commits. -/
def sumBy (measure : V3ScoresEntry → Nat) : List V3ScoresEntry → Nat
  | [] => 0
  | entry :: tail => measure entry + sumBy measure tail

theorem sumBy_append (measure : V3ScoresEntry → Nat)
    (left right : List V3ScoresEntry) :
    sumBy measure (left ++ right) =
      sumBy measure left + sumBy measure right := by
  induction left with
  | nil => simp [sumBy]
  | cons entry tail ih => simp only [List.cons_append, sumBy, ih]; omega

/-- The Rust scoring rule for one window `C-3`, `C-2`, `C-1`, `C`.

`LeaderScheduleV3::add_commit` scores `C-3` by scanning `[C-2, C-1, C]` for the
round `r + 1` voting blocks and round `r + 2` certifying blocks of `C-3`'s
leaders. That calculation reads only the four committed materials, so the model
takes it as one deterministic function of them. It is a function, so equal
materials give equal entries; nothing further is needed downstream.

`REF-V3-SCHEDULE-SCORER` carries the source obligation for the calculation
itself. -/
structure V3ScoreRule (BlockId CommitId : Type) where
  entryFor : V3CommitMaterial BlockId CommitId →
    V3CommitMaterial BlockId CommitId → V3CommitMaterial BlockId CommitId →
      V3CommitMaterial BlockId CommitId → V3ScoresEntry

/-- The running `LeaderScheduleV3` state.

`pendingCommits` is the Rust three-deep window. `entries` is the sliding score
window, oldest first. The three totals are the incremental Rust accumulators.
`allowedLeaders` is the ordered vector that the proposer and the FlexCommitter
read. -/
structure V3ScheduleState (BlockId CommitId : Type) where
  pendingCommits : List (V3CommitMaterial BlockId CommitId)
  entries : List V3ScoresEntry
  totalScores : Nat → Nat
  totalLeaderCount : Nat
  totalLeaderStakes : Nat
  allowedLeaders : List Nat

namespace V3ScheduleState

variable {BlockId : Type} [DecidableEq BlockId]
variable {CommitId : Type}

/-- Rust `next_commit_index()`: one past the last pending commit, or 1. -/
def nextCommitIndex (state : V3ScheduleState BlockId CommitId) : Nat :=
  match state.pendingCommits.getLast? with
  | some last => last.index + 1
  | none => 1

/-- Rust `min_next_leader_round()`: one past the last pending leader round, or
1. The proposer gate and the FlexCommitter both read this value. -/
def minNextLeaderRound (state : V3ScheduleState BlockId CommitId) : Nat :=
  match state.pendingCommits.getLast? with
  | some last => last.leader.round + 1
  | none => 1

/-- The shuffle seed of `select_allowed_leaders`: the commit digest of the last
pending commit. Rust falls back to an epoch-derived seed when none exists. -/
def shuffleSeed (state : V3ScheduleState BlockId CommitId) : Option CommitId :=
  state.pendingCommits.getLast?.map fun last => last.commitId

/-- The running totals agree with a recomputation over the retained window. -/
def Sound (state : V3ScheduleState BlockId CommitId) : Prop :=
  (∀ author,
      state.totalScores author =
        sumBy (fun entry => entry.contributions author) state.entries) ∧
    state.totalLeaderCount = sumBy V3ScoresEntry.leaderCount state.entries ∧
    state.totalLeaderStakes = sumBy V3ScoresEntry.leaderStakes state.entries

/-- Drop one named oldest entry and subtract its contributions. -/
def evictEntry (state : V3ScheduleState BlockId CommitId) (oldest : V3ScoresEntry)
    (rest : List V3ScoresEntry) : V3ScheduleState BlockId CommitId :=
  { state with
    entries := rest
    totalScores := fun author =>
      state.totalScores author - oldest.contributions author
    totalLeaderCount := state.totalLeaderCount - oldest.leaderCount
    totalLeaderStakes := state.totalLeaderStakes - oldest.leaderStakes }

/-- One Rust eviction step. -/
def evictOldest (state : V3ScheduleState BlockId CommitId) : V3ScheduleState BlockId CommitId :=
  match state.entries with
  | [] => state
  | oldest :: rest => state.evictEntry oldest rest

/-- The Rust `while self.scores_entries.len() >= self.window_size()` loop. The
caller supplies the entry count as the step budget, which
`evictWhileFull_frees_capacity` proves is enough. -/
def evictWhileFull (windowSize : Nat) :
    Nat → V3ScheduleState BlockId CommitId → V3ScheduleState BlockId CommitId
  | 0, state => state
  | steps + 1, state =>
      if windowSize ≤ state.entries.length then
        evictWhileFull windowSize steps state.evictOldest
      else state

/-- Append one scored entry and add its contributions. -/
def appendEntry (state : V3ScheduleState BlockId CommitId) (entry : V3ScoresEntry) :
    V3ScheduleState BlockId CommitId :=
  { state with
    entries := state.entries ++ [entry]
    totalScores := fun author =>
      state.totalScores author + entry.contributions author
    totalLeaderCount := state.totalLeaderCount + entry.leaderCount
    totalLeaderStakes := state.totalLeaderStakes + entry.leaderStakes }

/-- Rust `refresh_current_schedule` recomputes the allowed-leader vector only at
an update-interval boundary. -/
def refreshSchedule (select : Option CommitId → (Nat → Nat) → List Nat)
    (interval : Nat) (state : V3ScheduleState BlockId CommitId) :
    V3ScheduleState BlockId CommitId :=
  if (state.nextCommitIndex - 1) % interval == 0 then
    { state with allowedLeaders := select state.shuffleSeed state.totalScores }
  else state

/-- Score the oldest pending commit, evict, accumulate, and rotate the pending
window. -/
def scoreAndRotate (rule : V3ScoreRule BlockId CommitId) (windowSize : Nat)
    (state : V3ScheduleState BlockId CommitId)
    (scored second third material : V3CommitMaterial BlockId CommitId) :
    V3ScheduleState BlockId CommitId :=
  { (evictWhileFull windowSize state.entries.length state).appendEntry
      (rule.entryFor scored second third material) with
    pendingCommits := [second, third, material] }

/-- Rust `LeaderScheduleV3::add_commit`. A pending window shorter than three
commits is warmup and produces no score entry. -/
def addCommit (rule : V3ScoreRule BlockId CommitId) (windowSize interval : Nat)
    (select : Option CommitId → (Nat → Nat) → List Nat)
    (state : V3ScheduleState BlockId CommitId)
    (material : V3CommitMaterial BlockId CommitId) :
    V3ScheduleState BlockId CommitId :=
  refreshSchedule select interval
    (match state.pendingCommits with
      | [scored, second, third] =>
          scoreAndRotate rule windowSize state scored second third material
      | pending =>
          { state with pendingCommits := pending ++ [material] })

/-- The empty state at a new instance, a new epoch, or after recovery. -/
def initial (allowedLeaders : List Nat) : V3ScheduleState BlockId CommitId :=
  { pendingCommits := []
    entries := []
    totalScores := fun _ => 0
    totalLeaderCount := 0
    totalLeaderStakes := 0
    allowedLeaders }

omit [DecidableEq BlockId] in
theorem initial_sound (allowedLeaders : List Nat) :
    (initial (BlockId := BlockId) (CommitId := CommitId) allowedLeaders).Sound :=
  ⟨fun _ => rfl, rfl, rfl⟩

/-! ### The incremental accumulators are exact

Rust removes an evicted contribution with `checked_sub(..).unwrap()` and adds a
new one with `checked_add(..).unwrap()`. The results below show that the running
totals always equal a recomputation over the retained entries. The subtraction
therefore never underflows, and a restart that recomputes from the retained
window reaches the same totals.
-/

omit [DecidableEq BlockId] in
theorem evictEntry_sound {state : V3ScheduleState BlockId CommitId}
    {oldest : V3ScoresEntry} {rest : List V3ScoresEntry}
    (shape : state.entries = oldest :: rest) (sound : state.Sound) :
    (state.evictEntry oldest rest).Sound := by
  rcases sound with ⟨scores, count, stakes⟩
  rw [shape] at scores count stakes
  simp only [sumBy] at scores count stakes
  refine ⟨fun author => ?_, ?_, ?_⟩ <;> simp only [evictEntry]
  · have running := scores author
    omega
  · omega
  · omega

omit [DecidableEq BlockId] in
theorem evictOldest_sound {state : V3ScheduleState BlockId CommitId}
    (sound : state.Sound) : state.evictOldest.Sound := by
  unfold evictOldest
  split
  · exact sound
  · next oldest rest shape => exact evictEntry_sound shape sound

omit [DecidableEq BlockId] in
theorem appendEntry_sound {state : V3ScheduleState BlockId CommitId}
    (sound : state.Sound) (entry : V3ScoresEntry) :
    (state.appendEntry entry).Sound := by
  rcases sound with ⟨scores, count, stakes⟩
  refine ⟨fun author => ?_, ?_, ?_⟩ <;>
    simp only [appendEntry, sumBy_append, sumBy]
  · have running := scores author
    omega
  · omega
  · omega

omit [DecidableEq BlockId] in
theorem evictWhileFull_sound (windowSize : Nat) :
    ∀ (steps : Nat) {state : V3ScheduleState BlockId CommitId},
      state.Sound → (evictWhileFull windowSize steps state).Sound := by
  intro steps
  induction steps with
  | zero => intro state sound; exact sound
  | succ steps ih =>
      intro state sound
      unfold evictWhileFull
      split
      · exact ih (evictOldest_sound sound)
      · exact sound

omit [DecidableEq BlockId] in
/-- An eviction removes exactly one entry. -/
theorem evictOldest_length {state : V3ScheduleState BlockId CommitId}
    (nonEmpty : 0 < state.entries.length) :
    state.evictOldest.entries.length + 1 = state.entries.length := by
  unfold evictOldest
  split
  · next shape => rw [shape] at nonEmpty; exact absurd nonEmpty (by simp)
  · next oldest rest shape => rw [shape]; simp [evictEntry]

omit [DecidableEq BlockId] in
/-- The supplied step budget always frees a slot in the window. -/
theorem evictWhileFull_frees_capacity {windowSize : Nat}
    (positiveWindow : 0 < windowSize) :
    ∀ (steps : Nat) {state : V3ScheduleState BlockId CommitId},
      state.entries.length ≤ steps →
      (evictWhileFull windowSize steps state).entries.length < windowSize := by
  intro steps
  induction steps with
  | zero =>
      intro state budget
      have empty : state.entries.length = 0 := Nat.le_zero.mp budget
      simpa [evictWhileFull, empty] using positiveWindow
  | succ steps ih =>
      intro state budget
      unfold evictWhileFull
      split
      · next full =>
          have nonEmpty : 0 < state.entries.length :=
            Nat.lt_of_lt_of_le positiveWindow full
          exact ih (by have := evictOldest_length (state := state) nonEmpty; omega)
      · next notFull => omega

omit [DecidableEq BlockId] in
/-- One scored commit leaves the score window within its configured size. -/
theorem scoreAndRotate_window_bounded (rule : V3ScoreRule BlockId CommitId)
    {windowSize : Nat} (positiveWindow : 0 < windowSize)
    (state : V3ScheduleState BlockId CommitId)
    (scored second third material : V3CommitMaterial BlockId CommitId) :
    (scoreAndRotate rule windowSize state scored second third
      material).entries.length ≤ windowSize := by
  have freed := evictWhileFull_frees_capacity (BlockId := BlockId)
    (CommitId := CommitId) positiveWindow
    state.entries.length (state := state) (Nat.le_refl _)
  simp only [scoreAndRotate, appendEntry, List.length_append, List.length_cons,
    List.length_nil]
  omega

omit [DecidableEq BlockId] in
theorem refreshSchedule_sound
    (select : Option CommitId → (Nat → Nat) → List Nat) (interval : Nat)
    {state : V3ScheduleState BlockId CommitId} (sound : state.Sound) :
    (refreshSchedule select interval state).Sound := by
  unfold refreshSchedule
  split
  · exact sound
  · exact sound

omit [DecidableEq BlockId] in
/-- `add_commit` keeps the running totals exact. -/
theorem addCommit_sound (rule : V3ScoreRule BlockId CommitId)
    (windowSize interval : Nat)
    (select : Option CommitId → (Nat → Nat) → List Nat)
    {state : V3ScheduleState BlockId CommitId} (sound : state.Sound)
    (material : V3CommitMaterial BlockId CommitId) :
    (addCommit rule windowSize interval select state material).Sound := by
  unfold addCommit
  refine refreshSchedule_sound select interval ?_
  split
  · exact appendEntry_sound
      (evictWhileFull_sound windowSize state.entries.length sound) _
  · exact sound

end V3ScheduleState

/-! ### The concrete replay and its Rust reads -/

/-- The replay parameters that one epoch fixes.

`select` is `select_allowed_leaders`: it reads the shuffle seed and the running
scores. `shuffle` is the round-seeded permutation of the allowed-leader list.
`materialOf` is the exact committed material of one commit head. -/
structure V3ReplayParameters (BlockId CommitId : Type) where
  rule : V3ScoreRule BlockId CommitId
  windowSize : Nat
  updateInterval : Nat
  select : Option CommitId → (Nat → Nat) → List Nat
  shuffle : List Nat → Nat → List Nat
  initialAllowedLeaders : List Nat
  materialOf : ValidatorCommitHead CommitId → V3CommitMaterial BlockId CommitId

namespace V3ReplayParameters

variable {BlockId CommitId : Type} [DecidableEq BlockId]

/-- The replay that `LeaderScheduleV3` actually runs. -/
def replay (parameters : V3ReplayParameters BlockId CommitId) :
    ValidatorV3LeaderScheduleReplay CommitId (V3CommitMaterial BlockId CommitId)
      (V3ScheduleState BlockId CommitId) :=
  { initialState := V3ScheduleState.initial parameters.initialAllowedLeaders
    performanceFor := parameters.materialOf
    addCommit := V3ScheduleState.addCommit parameters.rule parameters.windowSize
      parameters.updateInterval parameters.select
    allowedLeaders := fun state => state.allowedLeaders
    selectedOrder := fun state round =>
      parameters.shuffle state.allowedLeaders round }

omit [DecidableEq BlockId] in
/-- Every replayed schedule state keeps exact running totals, so a restart that
recomputes from the retained window agrees with the incremental accumulators and
no `checked_sub` underflows. -/
theorem replayed_state_sound (parameters : V3ReplayParameters BlockId CommitId)
    {successor :
      ValidatorCommitHead CommitId → ValidatorCommitHead CommitId → Prop}
    {genesis head : ValidatorCommitHead CommitId} {length : Nat}
    {state : V3ScheduleState BlockId CommitId}
    (replayed :
      parameters.replay.ReplayedState successor genesis length head state) :
    state.Sound := by
  induction replayed with
  | genesis => exact V3ScheduleState.initial_sound _
  | next _ _ ih =>
      exact V3ScheduleState.addCommit_sound parameters.rule
        parameters.windowSize parameters.updateInterval parameters.select ih _

end V3ReplayParameters

/-- The Rust reads behind adaptive schedule replay at one validator.

`flushIsRun` binds the scorer input to one actual `FlexCommitter::build_commit`
run. `localMaterialIsSortedFlush` says that `add_commit` receives the
`CommittedSubDag` of that run: the commit index, the commit digest, the named
leader, and the sorted committed block vector. `namedLeaderIsLast` records that
the named leader is the last block of that vector. -/
structure V3ScheduleRustSourceMap (BlockId CommitId : Type)
    [DecidableEq BlockId]
    (parameters : V3ReplayParameters BlockId CommitId) where
  view : Nat → ValidatorCommitHead CommitId → CommitMaterializerView BlockId
  leaders : Nat → ValidatorCommitHead CommitId → List (ValidatorBlock BlockId)
  fuel : Nat → ValidatorCommitHead CommitId → Nat
  flush : Nat → ValidatorCommitHead CommitId → List (ValidatorBlock BlockId)
  marks : Nat → ValidatorCommitHead CommitId → List BlockId
  blockSort : CommitBlockSort BlockId
  flushIsRun : ∀ validator head,
    (view validator head).buildCommit (fuel validator head)
        (leaders validator head) =
      some (flush validator head, marks validator head)
  leadersCatalogued : ∀ validator head leader,
    leader ∈ leaders validator head →
      (view validator head).catalog leader.reference.id = some leader
  leadersNodup : ∀ validator head,
    ((leaders validator head).map fun leader => leader.reference.id).Nodup
  localMaterial : Nat → ValidatorCommitHead CommitId →
    V3CommitMaterial BlockId CommitId
  localMaterialIndex : ∀ validator head,
    (localMaterial validator head).index = head.index
  localMaterialCommitId : ∀ validator head,
    (localMaterial validator head).commitId = head.id
  localMaterialBlocks : ∀ validator head,
    (localMaterial validator head).blocks =
      blockSort.sort (leaders validator head) (flush validator head)
  namedLeaderIsLast : ∀ validator head,
    (localMaterial validator head).blocks.getLast?.map ValidatorBlock.reference =
      some (localMaterial validator head).leader
  /-- The replay scores the material that one designated host actually built.
  `materialOf_is_the_common_flush` then carries that choice to any other host
  whose walk reads agree, through `localMaterial_agrees`. Stating this field for
  every host at once would assume the cross-host agreement instead of deriving
  it. -/
  canonicalHost : Nat
  materialOfIsCanonical : ∀ head,
    parameters.materialOf head = localMaterial canonicalHost head
  /-- Proposer ancestor selection and the FlexCommitter read the current
  schedule state rather than a separately derived vector. The FlexCommitter also
  reads the minimum next leader round as its round-state base, and the proposer
  uses it as a gate. This is `REF-V3-SCHEDULE-READERS`. -/
  proposerAllowedLeaders : V3ScheduleState BlockId CommitId → List Nat
  proposerMinNextLeaderRound : V3ScheduleState BlockId CommitId → Nat
  flexAllowedLeaders : V3ScheduleState BlockId CommitId → List Nat
  flexMinNextLeaderRound : V3ScheduleState BlockId CommitId → Nat
  proposerReadsAllowedLeaders : ∀ state,
    proposerAllowedLeaders state = state.allowedLeaders
  proposerReadsRoundGate : ∀ state,
    proposerMinNextLeaderRound state = state.minNextLeaderRound
  flexReadsAllowedLeaders : ∀ state,
    flexAllowedLeaders state = state.allowedLeaders
  flexReadsRoundGate : ∀ state,
    flexMinNextLeaderRound state = state.minNextLeaderRound

namespace V3ScheduleRustSourceMap

variable {BlockId CommitId : Type} [DecidableEq BlockId]
variable {parameters : V3ReplayParameters BlockId CommitId}

/-- Two hosts that materialize one exact commit head from agreeing walk views
give the scorer the same input.

The block set comes from `CommitMaterializerView.buildCommit_outputs_agree`, the
duplicate-free vector from `CommitMaterializerView.buildCommit_output_nodup`, and
the order from the deterministic sort. The named leader follows because it is the
last block of that vector. -/
theorem localMaterial_agrees
    (map : V3ScheduleRustSourceMap BlockId CommitId parameters)
    {left right : Nat} {head : ValidatorCommitHead CommitId}
    (sameLeaders : map.leaders left head = map.leaders right head)
    (agreement : CommitMaterializerView.ViewAgreement (map.view left head)
      (map.view right head)) :
    map.localMaterial left head = map.localMaterial right head := by
  have sameBlocks :
      (map.localMaterial left head).blocks =
        (map.localMaterial right head).blocks := by
    rw [map.localMaterialBlocks, map.localMaterialBlocks, sameLeaders]
    exact map.blockSort.determined _ _ _
      (CommitMaterializerView.buildCommit_output_nodup (map.flushIsRun left head)
        (map.leadersNodup left head))
      (CommitMaterializerView.buildCommit_output_nodup
        (map.flushIsRun right head) (sameLeaders ▸ map.leadersNodup right head))
      (CommitMaterializerView.buildCommit_outputs_agree agreement
        (map.flushIsRun left head) (sameLeaders ▸ map.flushIsRun right head)
        (map.leadersCatalogued left head))
  have sameLeaderRef :
      (map.localMaterial left head).leader =
        (map.localMaterial right head).leader := by
    have leftLast := map.namedLeaderIsLast left head
    rw [sameBlocks, map.namedLeaderIsLast right head] at leftLast
    exact (Option.some.inj leftLast).symm
  rcases leftMaterial : map.localMaterial left head with
    ⟨leftIndex, leftCommitId, leftLeader, leftBlocks⟩
  rcases rightMaterial : map.localMaterial right head with
    ⟨rightIndex, rightCommitId, rightLeader, rightBlocks⟩
  rw [leftMaterial, rightMaterial] at sameBlocks sameLeaderRef
  have indices := map.localMaterialIndex left head
  have rightIndices := map.localMaterialIndex right head
  have commitIds := map.localMaterialCommitId left head
  have rightCommitIds := map.localMaterialCommitId right head
  rw [leftMaterial] at indices commitIds
  rw [rightMaterial] at rightIndices rightCommitIds
  simp only [V3CommitMaterial.mk.injEq]
  exact ⟨indices.trans rightIndices.symm,
    commitIds.trans rightCommitIds.symm, sameLeaderRef, sameBlocks⟩

/-- The replay input is well defined. It is the designated host's actual
materializer output, and any other host whose walk reads agree computes the same
input. The second part is derived from the walk, not assumed. -/
theorem materialOf_is_the_common_flush
    (map : V3ScheduleRustSourceMap BlockId CommitId parameters)
    {validator : Nat} {head : ValidatorCommitHead CommitId}
    (sameLeaders :
      map.leaders map.canonicalHost head = map.leaders validator head)
    (agreement :
      CommitMaterializerView.ViewAgreement (map.view map.canonicalHost head)
        (map.view validator head)) :
    parameters.materialOf head = map.localMaterial validator head := by
  rw [map.materialOfIsCanonical head]
  exact map.localMaterial_agrees sameLeaders agreement

end V3ScheduleRustSourceMap

/-! ### The end-to-end schedule mapping -/

namespace ExactCommitInstallProvenance

variable {BlockId CommitId History Encoding PacketId : Type}
variable [DecidableEq BlockId]
variable {config : ValidatorEpochConfig CommitId}
variable {functions : CommitReferenceFunctions
  CommitId (LeaderBlockRef BlockId) Encoding}
variable {context : ValidatorFlexContextAt BlockId CommitId History}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {source : LocalFlexCommitterSourceMap config functions context program}
variable {runtime : LocalFlexCommitterRuntime timed source}
variable {genesis : ValidatorCommitHead CommitId}
variable {durable : ExactCommitDurablePrefixSourceMap faults
  timed.execution.trace genesis}
variable {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
variable {validBlocks : CommitSyncBundle BlockId CommitId → Prop}

/-- Two correct hosts with exact installed heads at one commit index reach the
same replayed `LeaderScheduleV3` state.

Every read that the proposer and the FlexCommitter take from that state is
therefore the same at both hosts: the allowed-leader vector, its round order,
the next commit index, and the minimum next leader round. The state also keeps
exact running score totals, and each scored input is one host's actual
materializer output. `V3ScheduleRustSourceMap.materialOf_is_the_common_flush`
carries that input to every other host whose walk reads agree. -/
theorem exactInstalledHeadsAtSameIndexShareRustSchedule
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {parameters : V3ReplayParameters BlockId CommitId}
    (map : V3ScheduleRustSourceMap BlockId CommitId parameters)
    {leftTime leftValidator rightTime rightValidator : Nat}
    {left right : ValidatorCommitHead CommitId}
    (leftValidatorInRange : leftValidator < config.authorityCount)
    (leftValidatorCorrect : faults.correctAvailable leftValidator = true)
    (rightValidatorInRange : rightValidator < config.authorityCount)
    (rightValidatorCorrect : faults.correctAvailable rightValidator = true)
    (leftInstalled : durable.exactInstalledHead leftTime leftValidator left)
    (rightInstalled : durable.exactInstalledHead rightTime rightValidator right)
    (sameIndex : left.index = right.index) :
    ∃ state,
      parameters.replay.ReplayedState (ExactFlexSuccessor runtime) genesis
          left.index left state ∧
        parameters.replay.ReplayedState (ExactFlexSuccessor runtime) genesis
          right.index right state ∧
        state.Sound ∧
        parameters.materialOf left = map.localMaterial map.canonicalHost left ∧
        parameters.materialOf right =
          map.localMaterial map.canonicalHost right ∧
        (∀ other,
          parameters.replay.ReplayedState (ExactFlexSuccessor runtime) genesis
              left.index left other →
            other = state) := by
  rcases provenance.exactInstalledHeadsAtSameIndexShareV3ReplayState
      authenticated parameters.replay leftValidatorInRange leftValidatorCorrect
      rightValidatorInRange rightValidatorCorrect leftInstalled rightInstalled
      sameIndex with ⟨state, leftReplayed, rightReplayed⟩
  have successorUnique : ∀ prior first second,
      ExactFlexSuccessor runtime prior first →
      ExactFlexSuccessor runtime prior second → first = second := by
    intro _ _ _ firstStep secondStep
    exact ExactFlexSuccessor.unique authenticated firstStep secondStep
  refine ⟨state, leftReplayed, rightReplayed,
    parameters.replayed_state_sound leftReplayed,
    map.materialOfIsCanonical left, map.materialOfIsCanonical right,
    fun other otherReplayed => ?_⟩
  exact ValidatorV3LeaderScheduleReplay.ReplayedState.unique successorUnique
    otherReplayed leftReplayed

end ExactCommitInstallProvenance

end Mysticeti
