/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.GarbageCollection
import Mysticeti.ValidatorCausalExactNext
import Mysticeti.ValidatorTimedExecutionLemmas

namespace Mysticeti

/-! Local round-frontier facts for proposal catch-up.

A validator can propose in round `R` only after it accepts quorum stake of
parents in round `R - 1`. Thus, a proposal cannot pass the local accepted-quorum
frontier by more than one round. This fact does not need a future block, a common
round, or a witness lock.

The separate GC result is conditional. An arbitrary accepted block does not
bound the current commit-leader round after commit sync or restart.
-/

/-- One local state has an accepted quorum parent list in one exact round. -/
def ValidatorAcceptedQuorumAt
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (state : ValidatorLocalState BlockId CommitId)
    (round : Nat) : Prop :=
  ∃ parents : List (ValidatorBlockRef BlockId),
    ValidatorParentListReady config state (round + 1) parents

/-- `frontier` is an attained upper bound on local accepted quorum rounds. -/
structure ValidatorHighestAcceptedQuorumRoundAt
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (state : ValidatorLocalState BlockId CommitId)
    (frontier : Nat) : Prop where
  attained : ValidatorAcceptedQuorumAt config state frontier
  upperBound : ∀ round,
    ValidatorAcceptedQuorumAt config state round → round ≤ frontier

/-- A ready parent list has at least one parent. -/
theorem validator_parent_list_ready_nonempty
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
  have quorum := ready.2.2
  change config.thresholds.quorum ≤
    weight config.authorityCount config.stake VoterSet.empty at quorum
  rw [weight_empty] at quorum
  omega

/-- A ready proposal target creates an accepted quorum in its previous round. -/
theorem validator_parent_list_ready_gives_previous_quorum
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorLocalState BlockId CommitId}
    {targetRound : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (ready : ValidatorParentListReady config state targetRound parents) :
    0 < targetRound ∧
      ValidatorAcceptedQuorumAt config state (targetRound - 1) := by
  have nonempty := validator_parent_list_ready_nonempty ready
  rcases List.exists_mem_of_ne_nil parents nonempty with ⟨parent, member⟩
  have immediate := (ready.2.1 parent member).1
  have targetPositive : 0 < targetRound := by omega
  have previousTarget : targetRound - 1 + 1 = targetRound := by omega
  refine ⟨targetPositive, parents, ?_⟩
  rw [previousTarget]
  exact ready

/-- Any ready target is at most one round above the highest accepted quorum. -/
theorem validator_parent_list_ready_target_le_frontier_succ
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorLocalState BlockId CommitId}
    {frontier targetRound : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (highest : ValidatorHighestAcceptedQuorumRoundAt config state frontier)
    (ready : ValidatorParentListReady config state targetRound parents) :
    targetRound ≤ frontier + 1 := by
  rcases validator_parent_list_ready_gives_previous_quorum ready with
    ⟨targetPositive, previousQuorum⟩
  have previousLe := highest.upperBound (targetRound - 1) previousQuorum
  omega

/-- The basic normal-proposal guard cannot pass the quorum frontier. -/
theorem normal_proposal_guard_target_le_frontier_succ
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorLocalState BlockId CommitId}
    {validator frontier targetRound : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (highest : ValidatorHighestAcceptedQuorumRoundAt config state frontier)
    (guard : BasicValidatorActionGuard config validator
      (.proposeNormal targetRound parents) state) :
    targetRound ≤ frontier + 1 := by
  exact validator_parent_list_ready_target_le_frontier_succ highest guard.2.1

/-- The basic exact-next recovery guard cannot pass the quorum frontier. -/
theorem recovery_proposal_guard_target_le_frontier_succ
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorLocalState BlockId CommitId}
    {validator frontier : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (highest : ValidatorHighestAcceptedQuorumRoundAt config state frontier)
    (guard : BasicValidatorActionGuard config validator (.proposeNext parents)
      state) :
    state.highestSignedRound + 1 ≤ frontier + 1 := by
  rcases guard with ⟨recovery, _, _, targetIsNext, _, ready⟩
  rw [← targetIsNext]
  exact validator_parent_list_ready_target_le_frontier_succ highest ready

/-- The basic alignment guard cannot pass the quorum frontier. -/
theorem alignment_proposal_guard_target_le_frontier_succ
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorLocalState BlockId CommitId}
    {validator frontier : Nat}
    {witness : ValidatorBlockRef BlockId}
    {parents : List (ValidatorBlockRef BlockId)}
    (highest : ValidatorHighestAcceptedQuorumRoundAt config state frontier)
    (guard : BasicValidatorActionGuard config validator
      (.alignProposal witness parents) state) :
    witness.round ≤ frontier + 1 := by
  rcases guard with ⟨recovery, _, _, _, _, ready⟩
  exact validator_parent_list_ready_target_le_frontier_succ highest ready

/-- A block that is ready for persistence has an accepted quorum in its parent
round. -/
theorem persist_proposal_guard_gives_ready_parent_list
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorLocalState BlockId CommitId}
    {validator : Nat}
    {block : ValidatorBlock BlockId}
    (guard : BasicValidatorActionGuard config validator
      (.persistProposal block) state) :
    ValidatorParentListReady config state block.reference.round block.parents := by
  rcases guard with ⟨_, _, quorumParents, acceptedParents⟩
  exact ⟨quorumParents.1,
    fun parent member => ⟨quorumParents.2.1 parent member,
      acceptedParents parent member⟩,
    quorumParents.2.2⟩

/-- The persistence guard cannot store a block above `frontier + 1`. -/
theorem persist_proposal_guard_round_le_frontier_succ
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorLocalState BlockId CommitId}
    {validator frontier : Nat}
    {block : ValidatorBlock BlockId}
    (highest : ValidatorHighestAcceptedQuorumRoundAt config state frontier)
    (guard : BasicValidatorActionGuard config validator
      (.persistProposal block) state) :
    block.reference.round ≤ frontier + 1 := by
  exact validator_parent_list_ready_target_le_frontier_succ highest
    (persist_proposal_guard_gives_ready_parent_list guard)

/-- While `R = frontier + 1`, a correct local persistence step cannot create a
block in `R + 1` or a later round. The result refers to the exact state before
the atomic persistence action. -/
theorem atomic_persist_cannot_pass_next_frontier_round
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {validator frontier : Nat}
    {block : ValidatorBlock BlockId}
    (step : ValidatorAtomicStep config faults protocolPacket program time before
      (.localAction validator (.persistProposal block)) after)
    (highest : ValidatorHighestAcceptedQuorumRoundAt config
      (before.validatorState validator) frontier) :
    ¬(frontier + 2 ≤ block.reference.round) := by
  have guard := validator_atomic_local_action_has_basic_guard step
  have bounded := persist_proposal_guard_round_le_frontier_succ highest guard
  omega

/-! ### Operational quorum frontier -/

/-- One host has a finite usable quorum in one round.

Positive-round references have exact verified bodies in the current catalog.
Round zero uses the separate static genesis list and does not require copied
genesis bodies. No statement is made about blocks outside this list. -/
structure ValidatorOperationalQuorumAt
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (holder round : Nat) where
  references : List (ValidatorBlockRef BlockId)
  ready : ValidatorParentListReady config (world.validatorState holder)
    (round + 1) references
  retained : ∀ reference, reference ∈ references →
    (world.validatorState holder).retained reference = true
  positiveBodies : 0 < round → ∀ reference, reference ∈ references →
    ∃ block : ValidatorBlock BlockId,
      world.blockCatalog reference.id = some block ∧
        block.reference = reference ∧
        reference.author < config.authorityCount ∧
        block.HasQuorumImmediateParents config

namespace ValidatorOperationalQuorumAt

/-- An operational quorum is an accepted parent quorum at the same host. -/
theorem toAcceptedQuorum
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {holder round : Nat}
    (quorum : ValidatorOperationalQuorumAt config world holder round) :
    ValidatorAcceptedQuorumAt config (world.validatorState holder) round := by
  exact ⟨quorum.references, quorum.ready⟩

end ValidatorOperationalQuorumAt

/-- Current accepted-block provenance above one host's GC boundary.

Acceptance is parent-first above GC. A positive accepted reference also has its
exact valid body in the immutable block catalog. Parents at or below GC are
opaque committed roots and are not required to remain accepted or retained.
This is a current local implementation invariant, not a second source store. -/
structure ValidatorAcceptedCausalClosureAboveGcAt
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (holder : Nat) where
  acceptedPositiveHasBody : ∀ reference,
    0 < reference.round →
    (world.validatorState holder).accepted reference = true →
    ∃ block : ValidatorBlock BlockId,
      world.blockCatalog reference.id = some block ∧
        block.reference = reference ∧
        reference.author < config.authorityCount ∧
        block.HasQuorumImmediateParents config
  acceptedBodyHasAcceptedParentsAboveGc : ∀ child block parent,
    world.blockCatalog child.id = some block →
    block.reference = child →
    (world.validatorState holder).accepted child = true →
    parent ∈ block.parents →
    (world.validatorState holder).gcRound < parent.round →
    (world.validatorState holder).accepted parent = true

/-- One correct host has an attained, sourceable local frontier. -/
structure ValidatorOperationalQuorumFrontierAt
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (holder frontier : Nat)
    (canonicalGenesisParents : List (ValidatorBlockRef BlockId)) where
  quorum : ValidatorOperationalQuorumAt config world holder frontier
  thresholdClockAtSuccessor :
    (world.validatorState holder).thresholdClockRound = frontier + 1
  aboveGcOrGenesis :
    (frontier = 0 ∧ (world.validatorState holder).gcRound = 0) ∨
      (0 < frontier ∧ (world.validatorState holder).gcRound < frontier)
  canonicalGenesisReady : ValidatorParentListReady config
    (world.validatorState holder) 1 canonicalGenesisParents
  genesisFrontierIsCanonical : frontier = 0 →
    quorum.references = canonicalGenesisParents
  upperBound : ∀ round,
    ValidatorAcceptedQuorumAt config (world.validatorState holder) round →
      round ≤ frontier
  acceptedCausalClosure :
    ValidatorAcceptedCausalClosureAboveGcAt config world holder

namespace ValidatorOperationalQuorumFrontierAt

/-- An operational frontier is also the local highest accepted quorum. -/
theorem toHighestAcceptedQuorumRound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {holder frontier : Nat}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (source : ValidatorOperationalQuorumFrontierAt config world holder
      frontier canonicalGenesisParents) :
    ValidatorHighestAcceptedQuorumRoundAt config
      (world.validatorState holder) frontier :=
  ⟨source.quorum.toAcceptedQuorum, source.upperBound⟩

/-- A valid positive block has at least one immediate parent. -/
private theorem operational_valid_positive_block_has_parent
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {block : ValidatorBlock BlockId}
    (_positive : 0 < block.reference.round)
    (valid : block.HasQuorumImmediateParents config) :
    block.parents ≠ [] := by
  intro noParents
  have noAuthors : block.parentAuthors = VoterSet.empty := by
    funext validator
    simp [ValidatorBlock.parentAuthors, noParents, VoterSet.empty]
  have quorumPositive := config.thresholds.quorum_positive
  have quorumParents := valid.2.2
  rw [noAuthors, weight_empty] at quorumParents
  omega

/-- Parent-first acceptance and exact catalog bodies expose one valid body at
every requested positive round above the local GC boundary. -/
theorem accepted_causal_closure_has_valid_block_at_round
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {holder targetRound round : Nat}
    (closure : ValidatorAcceptedCausalClosureAboveGcAt config world holder)
    (targetBlock : ValidatorBlock BlockId)
    (targetCatalogued :
      world.blockCatalog targetBlock.reference.id = some targetBlock)
    (targetAccepted :
      (world.validatorState holder).accepted targetBlock.reference = true)
    (targetExact : targetBlock.reference.round = targetRound)
    (targetValid : targetBlock.HasQuorumImmediateParents config)
    (roundAboveGc : (world.validatorState holder).gcRound < round)
    (roundAtMostTarget : round ≤ targetRound) :
    ∃ block : ValidatorBlock BlockId,
      world.blockCatalog block.reference.id = some block ∧
        (world.validatorState holder).accepted block.reference = true ∧
        block.reference.round = round ∧
        block.HasQuorumImmediateParents config := by
  let distance := targetRound - round
  have targetEquation : round + distance = targetRound := by
    dsimp [distance]
    omega
  have descend : ∀ remaining current,
      current + remaining = targetRound →
      (world.validatorState holder).gcRound < current →
      ∃ block : ValidatorBlock BlockId,
        world.blockCatalog block.reference.id = some block ∧
          (world.validatorState holder).accepted block.reference = true ∧
          block.reference.round = current ∧
          block.HasQuorumImmediateParents config := by
    intro remaining
    induction remaining with
    | zero =>
        intro current currentAtTarget _currentAboveGc
        have currentExact : current = targetRound := by omega
        exact ⟨targetBlock, targetCatalogued, targetAccepted,
          by simpa [currentExact] using targetExact, targetValid⟩
    | succ smaller inductionHypothesis =>
        intro current currentBelowTarget currentAboveGc
        have nextEquation : current + 1 + smaller = targetRound := by omega
        have nextAboveGc :
            (world.validatorState holder).gcRound < current + 1 := by omega
        rcases inductionHypothesis (current + 1) nextEquation nextAboveGc with
          ⟨child, childCatalogued, childAccepted, childRound, childValid⟩
        have childPositive : 0 < child.reference.round := by omega
        obtain ⟨parent, parentMember⟩ := List.exists_mem_of_ne_nil
          child.parents
            (operational_valid_positive_block_has_parent childPositive
              childValid)
        have parentRound := childValid.2.1 parent parentMember
        have parentAtCurrent : parent.round = current := by omega
        have parentAboveGc :
            (world.validatorState holder).gcRound < parent.round := by
          simpa [parentAtCurrent] using currentAboveGc
        have parentAccepted :=
          closure.acceptedBodyHasAcceptedParentsAboveGc child.reference child
            parent childCatalogued rfl childAccepted parentMember parentAboveGc
        rcases closure.acceptedPositiveHasBody parent (by omega) parentAccepted
            with ⟨parentBlock, parentCatalogued, parentReference,
              _parentAuthorInRange, parentValid⟩
        have parentBlockCatalogued :
            world.blockCatalog parentBlock.reference.id = some parentBlock := by
          simpa [parentReference] using parentCatalogued
        exact ⟨parentBlock, parentBlockCatalogued,
          by simpa [parentReference] using parentAccepted,
          by rw [parentReference, parentAtCurrent], parentValid⟩
  exact descend distance round targetEquation roundAboveGc

/-- The current parent-first acceptance source proves every lower accepted
quorum which is still above local GC. Round zero is the separate canonical
genesis boundary. -/
theorem acceptedQuorumAtOrBelow
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {holder frontier lower : Nat}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (source : ValidatorOperationalQuorumFrontierAt config world holder
      frontier canonicalGenesisParents)
    (lowerAtMostFrontier : lower ≤ frontier)
    (lowerAboveGcOrGenesis : lower = 0 ∨
      (world.validatorState holder).gcRound < lower) :
    ValidatorAcceptedQuorumAt config (world.validatorState holder) lower := by
  by_cases lowerZero : lower = 0
  · subst lower
    exact ⟨canonicalGenesisParents, source.canonicalGenesisReady⟩
  by_cases atFrontier : lower = frontier
  · subst lower
    exact source.quorum.toAcceptedQuorum
  have lowerBelowFrontier : lower < frontier := by omega
  have frontierPositive : 0 < frontier := by omega
  have frontierReferencesNonempty : source.quorum.references ≠ [] :=
    validator_parent_list_ready_nonempty source.quorum.ready
  obtain ⟨targetReference, targetMember⟩ :=
    List.exists_mem_of_ne_nil source.quorum.references
      frontierReferencesNonempty
  rcases source.quorum.positiveBodies frontierPositive targetReference
      targetMember with
    ⟨targetBlock, targetCatalogued, targetBlockReference,
      _targetAuthorInRange, targetValid⟩
  have targetAccepted :=
    (source.quorum.ready.2.1 targetReference targetMember).2
  have targetRound := (source.quorum.ready.2.1 targetReference targetMember).1
  have targetBlockCatalogued :
      world.blockCatalog targetBlock.reference.id = some targetBlock := by
    simpa [targetBlockReference] using targetCatalogued
  have childRoundAboveGc :
      (world.validatorState holder).gcRound < lower + 1 := by
    rcases lowerAboveGcOrGenesis with lowerZero' | lowerAboveGc
    · exact False.elim (lowerZero lowerZero')
    · omega
  have childRoundAtMostFrontier : lower + 1 ≤ frontier := by omega
  rcases accepted_causal_closure_has_valid_block_at_round
      source.acceptedCausalClosure targetBlock targetBlockCatalogued
        (by simpa [targetBlockReference] using targetAccepted)
          (by rw [targetBlockReference]; omega) targetValid childRoundAboveGc
            childRoundAtMostFrontier with
    ⟨child, childCatalogued, childAccepted, childRound, childValid⟩
  refine ⟨child.parents, childValid.1, ?_, childValid.2.2⟩
  intro parent parentMember
  have parentRound := childValid.2.1 parent parentMember
  have parentAtLower : parent.round = lower := by omega
  have lowerAboveGc : (world.validatorState holder).gcRound < lower := by
    rcases lowerAboveGcOrGenesis with lowerZero' | lowerAboveGc
    · exact False.elim (lowerZero lowerZero')
    · exact lowerAboveGc
  have parentAccepted : (world.validatorState holder).accepted parent = true :=
    source.acceptedCausalClosure
      |>.acceptedBodyHasAcceptedParentsAboveGc child.reference child parent
        childCatalogued rfl childAccepted parentMember (by
          simpa [parentAtLower] using lowerAboveGc)
  exact ⟨by omega, parentAccepted⟩

/-- The operational frontier is a legal retained parent list for its exact
successor round. -/
theorem successorParentListReady
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {holder frontier : Nat}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (source : ValidatorOperationalQuorumFrontierAt config world holder
      frontier canonicalGenesisParents) :
    ValidatorProposalParentListReady .normal config
      (world.validatorState holder) (frontier + 1)
        source.quorum.references := by
  refine ⟨source.quorum.ready, ?_⟩
  intro parent parentMember
  refine ⟨source.quorum.retained parent parentMember, ?_⟩
  have parentRound := (source.quorum.ready.2.1 parent parentMember).1
  rcases source.aboveGcOrGenesis with
    ⟨frontierZero, _gcZero⟩ | ⟨_frontierPositive, frontierAboveGc⟩
  · exact Or.inl (by omega)
  · exact Or.inr (by omega)

end ValidatorOperationalQuorumFrontierAt

/-- Local current-state sources for operational quorum frontiers.

This map has only current finite storage facts. It does not assert a future
proposal, delivery, quorum, or commit. -/
structure ValidatorOperationalQuorumFrontierSourceMap
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (canonicalGenesisParents : List (ValidatorBlockRef BlockId)) where
  frontier : Time → Nat → Nat
  currentSource : ∀ time holder,
    holder < config.authorityCount →
    faults.correctAvailable holder = true →
    (timed.execution.trace time).epochActive = true →
    Nonempty (ValidatorOperationalQuorumFrontierAt config
      (timed.execution.trace time) holder (frontier time holder)
        canonicalGenesisParents)

/-- The finite maximum of correct, available operational frontiers. -/
def correctOperationalQuorumFrontierMaximumUpTo
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (source : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (time : Time) : Nat → Nat
  | 0 => 0
  | count + 1 =>
      max (correctOperationalQuorumFrontierMaximumUpTo source time count)
        (if faults.correctAvailable count = true then
          source.frontier time count else 0)

/-- Every in-range correct host frontier is at most the finite maximum. -/
theorem correct_operational_quorum_frontier_le_maximum
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (source : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {time holder count : Nat}
    (holderInRange : holder < count)
    (holderCorrect : faults.correctAvailable holder = true) :
    source.frontier time holder ≤
      correctOperationalQuorumFrontierMaximumUpTo source time count := by
  induction count with
  | zero => omega
  | succ previous inductionHypothesis =>
      by_cases lastHolder : holder = previous
      · subst holder
        simp only [correctOperationalQuorumFrontierMaximumUpTo, holderCorrect,
          if_pos]
        exact Nat.le_max_right _ _
      · have holderBefore : holder < previous := by omega
        have bounded := inductionHypothesis holderBefore
        exact Nat.le_trans bounded (Nat.le_max_left _ _)

/-- A positive finite maximum is attained by one correct, available host. -/
theorem positive_correct_operational_quorum_frontier_maximum_has_owner
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (source : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {time count : Nat}
    (positive : 0 <
      correctOperationalQuorumFrontierMaximumUpTo source time count) :
    ∃ holder,
      holder < count ∧
        faults.correctAvailable holder = true ∧
        source.frontier time holder =
          correctOperationalQuorumFrontierMaximumUpTo source time count := by
  induction count with
  | zero =>
      simp [correctOperationalQuorumFrontierMaximumUpTo] at positive
  | succ previous inductionHypothesis =>
      simp only [correctOperationalQuorumFrontierMaximumUpTo] at positive ⊢
      let earlier :=
        correctOperationalQuorumFrontierMaximumUpTo source time previous
      let last := if faults.correctAvailable previous = true then
        source.frontier time previous else 0
      by_cases lastAtMostEarlier : last ≤ earlier
      · have maximumIsEarlier : max earlier last = earlier :=
          Nat.max_eq_left lastAtMostEarlier
        have earlierPositive : 0 < earlier := by
          rw [maximumIsEarlier] at positive
          exact positive
        rcases inductionHypothesis earlierPositive with
          ⟨holder, holderInRange, holderCorrect, holderMaximum⟩
        exact ⟨holder, by omega, holderCorrect, by
          rw [maximumIsEarlier]
          exact holderMaximum⟩
      · have earlierAtMostLast : earlier ≤ last :=
          Nat.le_of_lt (Nat.lt_of_not_ge lastAtMostEarlier)
        have maximumIsLast : max earlier last = last :=
          Nat.max_eq_right earlierAtMostLast
        have lastPositive : 0 < last := by
          rw [maximumIsLast] at positive
          exact positive
        have previousCorrect : faults.correctAvailable previous = true := by
          unfold last at lastPositive
          split at lastPositive
          · assumption
          · omega
        refine ⟨previous, by omega, previousCorrect, ?_⟩
        rw [maximumIsLast]
        simp [last, previousCorrect]

/-- The finite maximum has one actual correct-host source in every active
state, including the all-genesis case. -/
theorem correct_operational_quorum_frontier_maximum_has_source
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (source : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {time : Time}
    (epochActive : (timed.execution.trace time).epochActive = true) :
    ∃ holder,
      holder < config.authorityCount ∧
        faults.correctAvailable holder = true ∧
        source.frontier time holder =
          correctOperationalQuorumFrontierMaximumUpTo source time
            config.authorityCount ∧
        Nonempty (ValidatorOperationalQuorumFrontierAt config
          (timed.execution.trace time) holder (source.frontier time holder)
            canonicalGenesisParents) := by
  let maximum := correctOperationalQuorumFrontierMaximumUpTo source time
    config.authorityCount
  by_cases maximumPositive : 0 < maximum
  · rcases positive_correct_operational_quorum_frontier_maximum_has_owner source
        maximumPositive with ⟨holder, holderInRange, holderCorrect,
          holderMaximum⟩
    exact ⟨holder, holderInRange, holderCorrect, holderMaximum,
      source.currentSource time holder holderInRange holderCorrect epochActive⟩
  · have correctWeightPositive :
        0 < weight config.authorityCount config.stake
          faults.correctAvailable :=
      Nat.lt_of_lt_of_le config.thresholds.quorum_positive
        faults.correct_available_stake_is_quorum
    rcases positive_weight_has_member correctWeightPositive with
      ⟨holder, holderInRange, holderCorrect, _holderStakePositive⟩
    have holderAtMostMaximum :=
      correct_operational_quorum_frontier_le_maximum source
        (time := time) holderInRange holderCorrect
    have maximumZero : maximum = 0 := by omega
    have maximumZeroExact :
        correctOperationalQuorumFrontierMaximumUpTo source time
            config.authorityCount = 0 := by
      simpa only [maximum] using maximumZero
    have holderZero : source.frontier time holder = 0 := by
      rw [maximumZeroExact] at holderAtMostMaximum
      omega
    exact ⟨holder, holderInRange, holderCorrect, by
      rw [maximumZeroExact]
      exact holderZero,
      source.currentSource time holder holderInRange holderCorrect epochActive⟩

/-- The maximum owner exposes the exact current parent source for round
`maximum + 1`.

This theorem selects no future proposal. It returns the current threshold-clock
target and one retained, above-GC parent list. The proposer can select different
exact representatives in the same round. An actual child must therefore expose
its own parent list as the next operational witness. -/
theorem correct_operational_quorum_frontier_maximum_gives_successor_source
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (source : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {time : Time}
    (epochActive : (timed.execution.trace time).epochActive = true) :
    ∃ (holder : Nat)
        (localSource : ValidatorOperationalQuorumFrontierAt config
          (timed.execution.trace time) holder (source.frontier time holder)
            canonicalGenesisParents),
      holder < config.authorityCount ∧
        faults.correctAvailable holder = true ∧
        source.frontier time holder =
          correctOperationalQuorumFrontierMaximumUpTo source time
            config.authorityCount ∧
        ((timed.execution.trace time).validatorState holder).thresholdClockRound =
          correctOperationalQuorumFrontierMaximumUpTo source time
            config.authorityCount + 1 ∧
        ValidatorProposalParentListReady .normal config
          ((timed.execution.trace time).validatorState holder)
          (correctOperationalQuorumFrontierMaximumUpTo source time
            config.authorityCount + 1) localSource.quorum.references := by
  rcases correct_operational_quorum_frontier_maximum_has_source source
      epochActive with
    ⟨holder, holderInRange, holderCorrect, holderMaximum,
      localSourceNonempty⟩
  rcases localSourceNonempty with ⟨localSource⟩
  refine ⟨holder, localSource, holderInRange, holderCorrect, holderMaximum,
    ?_, ?_⟩
  · rw [← holderMaximum]
    exact localSource.thresholdClockAtSuccessor
  · rw [← holderMaximum]
    exact localSource.successorParentListReady

/-- A current normal proposal cannot pass the finite correct-host maximum. -/
theorem current_normal_proposal_target_le_operational_maximum_succ
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (source : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {time holder targetRound : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (holderInRange : holder < config.authorityCount)
    (holderCorrect : faults.correctAvailable holder = true)
    (epochActive : (timed.execution.trace time).epochActive = true)
    (guard : BasicValidatorActionGuard config holder
      (.proposeNormal targetRound parents)
      ((timed.execution.trace time).validatorState holder)) :
    targetRound ≤
      correctOperationalQuorumFrontierMaximumUpTo source time
        config.authorityCount + 1 := by
  rcases source.currentSource time holder holderInRange holderCorrect epochActive
      with ⟨localSource⟩
  have localBound := normal_proposal_guard_target_le_frontier_succ
    localSource.toHighestAcceptedQuorumRound guard
  have frontierBound := correct_operational_quorum_frontier_le_maximum source
    (time := time) holderInRange holderCorrect
  omega

/-- A current exact-next recovery proposal cannot pass the finite maximum. -/
theorem current_recovery_proposal_target_le_operational_maximum_succ
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (source : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {time holder : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (holderInRange : holder < config.authorityCount)
    (holderCorrect : faults.correctAvailable holder = true)
    (epochActive : (timed.execution.trace time).epochActive = true)
    (guard : BasicValidatorActionGuard config holder (.proposeNext parents)
      ((timed.execution.trace time).validatorState holder)) :
    ((timed.execution.trace time).validatorState holder).highestSignedRound + 1 ≤
      correctOperationalQuorumFrontierMaximumUpTo source time
        config.authorityCount + 1 := by
  rcases source.currentSource time holder holderInRange holderCorrect epochActive
      with ⟨localSource⟩
  have localBound := recovery_proposal_guard_target_le_frontier_succ
    localSource.toHighestAcceptedQuorumRound guard
  have frontierBound := correct_operational_quorum_frontier_le_maximum source
    (time := time) holderInRange holderCorrect
  omega

/-- A current alignment proposal cannot pass the finite maximum. -/
theorem current_alignment_proposal_target_le_operational_maximum_succ
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (source : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {time holder : Nat}
    {witness : ValidatorBlockRef BlockId}
    {parents : List (ValidatorBlockRef BlockId)}
    (holderInRange : holder < config.authorityCount)
    (holderCorrect : faults.correctAvailable holder = true)
    (epochActive : (timed.execution.trace time).epochActive = true)
    (guard : BasicValidatorActionGuard config holder
      (.alignProposal witness parents)
      ((timed.execution.trace time).validatorState holder)) :
    witness.round ≤
      correctOperationalQuorumFrontierMaximumUpTo source time
        config.authorityCount + 1 := by
  rcases source.currentSource time holder holderInRange holderCorrect epochActive
      with ⟨localSource⟩
  have localBound := alignment_proposal_guard_target_le_frontier_succ
    localSource.toHighestAcceptedQuorumRound guard
  have frontierBound := correct_operational_quorum_frontier_le_maximum source
    (time := time) holderInRange holderCorrect
  omega

/-- An actual persisted proposal cannot pass the next round after the finite
operational maximum at the end of its event batch.

Parent acceptance is durable through the action and the remaining batch. Thus,
the bound does not assume that the persistence action is the first event in the
batch. -/
theorem persisted_proposal_occurrence_round_le_operational_maximum_succ
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (source : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {time holder : Time}
    {block : ValidatorBlock BlockId}
    (epochActiveAfter :
      (timed.execution.trace (time + 1)).epochActive = true)
    (occurs : ValidatorLocalActionOccurs (timed.execution.events time) holder
      (.persistProposal block)) :
    block.reference.round ≤
      correctOperationalQuorumFrontierMaximumUpTo source (time + 1)
        config.authorityCount + 1 := by
  have completeStep := timed.execution.stepsFollowRules time
  have holderScope := validator_local_action_occurrence_is_correct_available
    completeStep occurs
  rcases holderScope with ⟨holderInRange, holderCorrect⟩
  rcases validator_world_step_local_action_with_suffix completeStep occurs with
    ⟨actionBefore, actionAfter, _suffix, actionStep, suffixStep⟩
  have guard := validator_atomic_local_action_has_basic_guard actionStep
  have readyBefore := persist_proposal_guard_gives_ready_parent_list guard
  have readyAfter : ValidatorParentListReady config
      ((timed.execution.trace (time + 1)).validatorState holder)
      block.reference.round block.parents := by
    refine ⟨readyBefore.1, ?_, readyBefore.2.2⟩
    intro parent parentMember
    have parentBefore := readyBefore.2.1 parent parentMember
    have parentAfterAction :=
      (validator_atomic_step_durable_monotone actionStep holder)
        |>.accepted_block_persists parentBefore.2
    have parentAtEnd := validator_world_step_accepted_block_persists suffixStep
      parentAfterAction
    exact ⟨parentBefore.1, parentAtEnd⟩
  rcases source.currentSource (time + 1) holder holderInRange holderCorrect
      epochActiveAfter with ⟨localSource⟩
  have localBound := validator_parent_list_ready_target_le_frontier_succ
    localSource.toHighestAcceptedQuorumRound readyAfter
  have frontierBound := correct_operational_quorum_frontier_le_maximum source
    (time := time + 1) holderInRange holderCorrect
  omega

/-- Exact installed-head provenance is enough to place the local GC boundary
below an operational frontier.

The parent-quorum premise is for the exact current installed head. It must come
from the installed-head bootstrap source. An arbitrary accepted block does not
supply this premise. -/
theorem exact_installed_head_parent_quorum_bounds_gc_below_frontier
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {holder frontier : Nat}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (source : ValidatorOperationalQuorumFrontierAt config world holder
      frontier canonicalGenesisParents)
    {head : ValidatorCommitHead CommitId}
    (_currentHead : (world.validatorState holder).commitHead = head)
    (frontierPositive : 0 < frontier)
    (gcZeroOrHeadDepth : (world.validatorState holder).gcRound = 0 ∨
      (world.validatorState holder).gcRound + 2 ≤ head.round)
    (headParentQuorum : ValidatorAcceptedQuorumAt config
      (world.validatorState holder) (head.round - 1)) :
    (world.validatorState holder).gcRound < frontier := by
  have headParentAtMostFrontier := source.upperBound (head.round - 1)
    headParentQuorum
  rcases gcZeroOrHeadDepth with gcZero | headDepth
  · omega
  · omega

/-- The arithmetic part of the proposed GC bridge.

The accepted-block premise must separately prove `commitRound < observedRound`.
The main validator model does not derive that inequality from acceptance alone.
The positive-round premise avoids truncated natural-number subtraction. -/
theorem gc_is_below_observed_round_minus_three
    {commitRound gcDepth observedRound : Nat}
    (depthExceedsIndirect : indirectCommitDepth < gcDepth)
    (commitBeforeObserved : commitRound < observedRound)
    (observedRoundAboveThree : 3 < observedRound) :
    gcRound commitRound gcDepth < observedRound - 3 := by
  unfold indirectCommitDepth at depthExceedsIndirect
  unfold gcRound
  omega

/-- A block can be accepted above GC while its round is not later than the
commit-leader round. Thus, acceptance alone cannot supply the commit-round
premise of the GC theorem. -/
theorem accepted_block_does_not_bound_commit_round_example :
    let commitRound := 10
    let gcDepth := 5
    let observedRound := 7
    gcRound commitRound gcDepth < observedRound ∧
      ¬(commitRound < observedRound) := by
  decide

end Mysticeti
