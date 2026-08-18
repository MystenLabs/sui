/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.CommitReferenceAgreement
import Mysticeti.ValidatorCausalExactNext
import Mysticeti.ValidatorFlexCommitter
import Mysticeti.ValidatorOriginAwareTraceBridge
import Mysticeti.ValidatorProposalObligation
import Mysticeti.ValidatorProposalLatchBridge
import Mysticeti.ValidatorRecoveryParentNeedExecution
import Mysticeti.ValidatorRecoveryMode

namespace Mysticeti

/-! Unbounded normal block production from one-host proposal continuity.

This file does not use a stalled commit, a common round, a quorum block layer,
or a recovery result. A correct, available host keeps one local work item:

* one legal proposal that is ready for persistence; or
* its newest durable own block with an uncleared send goal.

A commit or GC update can select a different legal target. It cannot leave the
host without one of these work items. The durable signer floor prevents a later
work item from going back to an old round.
-/

/-- A deterministic different receiver when the validator set has at least two
members. -/
def validatorOtherReceiver (validator : Nat) : Nat :=
  if validator = 0 then 1 else 0

theorem validator_other_receiver_in_range
    {validator authorityCount : Nat}
    (authorityCountAtLeastTwo : 1 < authorityCount) :
    validatorOtherReceiver validator < authorityCount := by
  simp only [validatorOtherReceiver]
  split
  · exact authorityCountAtLeastTwo
  · omega

theorem validator_other_receiver_is_different
    {validator : Nat} :
    validatorOtherReceiver validator ≠ validator := by
  simp only [validatorOtherReceiver]
  split <;> omega

/-- Fixed round-zero parent data at every correct, available validator.

The list and its stake are static epoch data. Acceptance starts in local
storage, and round-zero bodies are never removed by GC. -/
structure ValidatorCanonicalGenesisParentRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  parents : List (ValidatorBlockRef BlockId)
  parentAuthorsNodup : (parents.map ValidatorBlockRef.author).Nodup
  parentsAreRoundZero : ∀ parent, parent ∈ parents → parent.round = 0
  parentStakeIsQuorum : config.thresholds.quorum ≤
    weight config.authorityCount config.stake (validatorParentAuthors parents)
  initiallyAccepted : ∀ validator parent,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    parent ∈ parents →
    ((timed.execution.trace 0).validatorState validator).accepted parent = true
  retainedAtCorrectValidator : ∀ time validator parent,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    parent ∈ parents →
    ((timed.execution.trace time).validatorState validator).retained parent =
      true

/-- The concrete post-install decision DAG above one host's GC boundary.

The root is bound to the installed head. Every stored body is current local
data. Parents at or below GC are committed roots and are not in this DAG. -/
structure ValidatorGcTruncatedDecisionDag
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (validator : Nat) (head : ValidatorCommitHead CommitId) where
  rootBlock : ValidatorBlock BlockId
  rootRound : rootBlock.reference.round = head.round
  history : List (ValidatorBlock BlockId)
  rootInHistory : rootBlock ∈ history
  historyReferencesNodup : (history.map ValidatorBlock.reference).Nodup
  historyAboveGc : ∀ block, block ∈ history →
    (world.validatorState validator).gcRound < block.reference.round
  historyCatalogued : ∀ block, block ∈ history →
    world.blockCatalog block.reference.id = some block
  historyAccepted : ∀ block, block ∈ history →
    (world.validatorState validator).accepted block.reference = true
  historyRetained : ∀ block, block ∈ history →
    (world.validatorState validator).retained block.reference = true
  historyValid : ∀ block, block ∈ history →
    block.HasQuorumImmediateParents config
  historyClosedAboveGc : ∀ block, block ∈ history →
    ∀ parent, parent ∈ block.parents →
      (world.validatorState validator).gcRound < parent.round →
      ∃ parentBlock,
        parentBlock ∈ history ∧ parentBlock.reference = parent

/-- One actual commit install projects one exact GC-truncated decision DAG in
the same host's post-state.

This source map contains no future action or network result. The selected list
is the host's current one-branch-per-author representative list for one target
parent round. -/
structure ValidatorInstalledCommitParentSourceMap
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  projectedDecisionDag : ∀ time validator head targetRound,
    Option (ValidatorGcTruncatedDecisionDag config
      (timed.execution.trace (time + 1)) validator head)
  selectedParents : Time → Nat → ValidatorCommitHead CommitId → Nat →
    List (ValidatorBlockRef BlockId)
  /-- The concrete local record action projects its exact post-state DAG. A
  synchronized install does not supply this liveness source. -/
  recordCommitActionProjectsDecisionDag : ∀ time validator head targetRound,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.recordCommit head) →
    ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 1 <
      targetRound →
    targetRound ≤ head.round →
    ∃ dag record,
      projectedDecisionDag time validator head targetRound = some dag ∧
        constructExactCommitReference functions record =
          { index := head.index, digest := head.id } ∧
        referenceLeaderBlockToValidatorBlockRef record.leader =
          dag.rootBlock.reference ∧
        record.leader ∈ record.blocks ∧
        dag.history.map ValidatorBlock.reference =
          (record.blocks.map referenceLeaderBlockToValidatorBlockRef).filter
            fun reference =>
            ((timed.execution.trace (time + 1)).validatorState
              validator).gcRound < reference.round
  /-- The record batch re-pins a current accepted representative when that
  exact block remains above the new local GC boundary. -/
  recordCommitRepinsAboveGcRepresentative :
    ∀ time validator head round author reference,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.recordCommit head) →
    ((timed.execution.trace time).validatorState
      validator).acceptedRepresentative round author = some reference →
    ((timed.execution.trace (time + 1)).validatorState validator).gcRound <
      reference.round →
    ((timed.execution.trace (time + 1)).validatorState validator).retained
      reference = true
  /-- A non-genesis local record keeps at least one full proposal parent round
  above the post-record GC boundary. -/
  recordCommitHeadHasPostGcParentRound : ∀ time validator head,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.recordCommit head) →
    0 < head.round →
    ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 2 ≤
      head.round
  selectedParentAuthorsNodup : ∀ time validator head targetRound dag,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.recordCommit head) →
    projectedDecisionDag time validator head targetRound = some dag →
    ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 1 <
      targetRound →
    targetRound ≤ head.round →
    ((selectedParents time validator head targetRound).map
      ValidatorBlockRef.author).Nodup
  selectedParentAuthorsInRange : ∀ time validator head targetRound dag parent,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.recordCommit head) →
    projectedDecisionDag time validator head targetRound = some dag →
    ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 1 <
      targetRound →
    targetRound ≤ head.round →
    parent ∈ selectedParents time validator head targetRound →
    parent.author < config.authorityCount
  selectedParentIsCurrentRepresentative :
    ∀ time validator head targetRound dag parent,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.recordCommit head) →
    projectedDecisionDag time validator head targetRound = some dag →
    ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 1 <
      targetRound →
    targetRound ≤ head.round →
    parent ∈ selectedParents time validator head targetRound →
    ((timed.execution.trace (time + 1)).validatorState
      validator).acceptedRepresentative (targetRound - 1) parent.author =
        some parent
  selectedParentIsRetained :
    ∀ time validator head targetRound dag parent,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.recordCommit head) →
    projectedDecisionDag time validator head targetRound = some dag →
    ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 1 <
      targetRound →
    targetRound ≤ head.round →
    parent ∈ selectedParents time validator head targetRound →
    ((timed.execution.trace (time + 1)).validatorState
      validator).retained parent = true
  selectedParentsIncludeCurrentRepresentatives :
    ∀ time validator head targetRound dag author parent,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.recordCommit head) →
    projectedDecisionDag time validator head targetRound = some dag →
    ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 1 <
      targetRound →
    targetRound ≤ head.round →
    author < config.authorityCount →
    ((timed.execution.trace (time + 1)).validatorState
      validator).acceptedRepresentative (targetRound - 1) author = some parent →
    parent ∈ selectedParents time validator head targetRound
  /-- Replacing an equivocating branch does not remove its author's stake. -/
  selectedParentsCoverDecisionBlockAuthors :
    ∀ time validator head targetRound
      (dag : ValidatorGcTruncatedDecisionDag config
        (timed.execution.trace (time + 1)) validator head)
      block parent,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.recordCommit head) →
    projectedDecisionDag time validator head targetRound = some dag →
    ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 1 <
      targetRound →
    targetRound ≤ head.round →
    block ∈ dag.history →
    block.reference.round = targetRound →
    parent ∈ block.parents →
    parent.author < config.authorityCount →
    ∃ selected,
      selected ∈ selectedParents time validator head targetRound ∧
        selected.author = parent.author

/-- A valid positive-round block has at least one immediate parent. -/
private theorem normal_liveness_valid_block_has_parent
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {block : ValidatorBlock BlockId}
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

/-- Walking one concrete parent at each round finds a valid block at each round
strictly above the local GC boundary. -/
theorem gc_truncated_decision_dag_has_valid_block_at_round
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {validator : Nat} {head : ValidatorCommitHead CommitId}
    (dag : ValidatorGcTruncatedDecisionDag config world validator head)
    {targetRound : Nat}
    (targetAboveGc :
      (world.validatorState validator).gcRound < targetRound)
    (targetAtMostRoot : targetRound ≤ head.round) :
    ∃ block,
      block ∈ dag.history ∧
        block.reference.round = targetRound ∧
        block.HasQuorumImmediateParents config := by
  let distance := head.round - targetRound
  have targetEquation : targetRound + distance = head.round := by
    dsimp [distance]
    omega
  have descend : ∀ remaining current,
      current + remaining = head.round →
      (world.validatorState validator).gcRound < current →
      ∃ block,
        block ∈ dag.history ∧
          block.reference.round = current ∧
          block.HasQuorumImmediateParents config := by
    intro remaining
    induction remaining with
    | zero =>
        intro current currentAtRoot _currentAboveGc
        have sameRound : current = head.round := by omega
        exact ⟨dag.rootBlock, dag.rootInHistory,
          by simpa [sameRound] using dag.rootRound,
          dag.historyValid dag.rootBlock dag.rootInHistory⟩
    | succ smaller inductionHypothesis =>
        intro current currentBelowRoot currentAboveGc
        have nextEquation : current + 1 + smaller = head.round := by omega
        rcases inductionHypothesis (current + 1) nextEquation (by omega) with
          ⟨child, childMember, childRound, childValid⟩
        obtain ⟨parentReference, parentMember⟩ :=
          List.exists_mem_of_ne_nil child.parents
            (normal_liveness_valid_block_has_parent childValid)
        have parentRound := childValid.2.1 parentReference parentMember
        have parentAboveGc :
            (world.validatorState validator).gcRound <
              parentReference.round := by
          omega
        rcases dag.historyClosedAboveGc child childMember parentReference
            parentMember parentAboveGc with
          ⟨parent, parentMemberHistory, parentReferenceExact⟩
        exact ⟨parent, parentMemberHistory, by
          rw [parentReferenceExact]
          omega,
          dag.historyValid parent parentMemberHistory⟩
  exact descend distance targetRound targetEquation targetAboveGc

/-- A current GC-truncated local DAG supplies one legal normal parent list.

This result uses only stored blocks in the current host. It does not depend on
how the current commit head was installed. In particular, a verified sync can
use this theorem only after its block data is already accepted and retained in
the local DAG. -/
theorem gc_truncated_decision_dag_gives_normal_parent_list
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {validator : Nat} {head : ValidatorCommitHead CommitId}
    (dag : ValidatorGcTruncatedDecisionDag config world validator head)
    {targetRound : Nat}
    (targetParentsAboveGc :
      (world.validatorState validator).gcRound + 1 < targetRound)
    (targetAtMostRoot : targetRound ≤ head.round) :
    ∃ parents,
      ValidatorProposalParentListReady .normal config
        (world.validatorState validator) targetRound parents := by
  rcases gc_truncated_decision_dag_has_valid_block_at_round dag
      (by omega) targetAtMostRoot with
    ⟨targetBlock, targetMember, targetRoundExact, targetValid⟩
  refine ⟨targetBlock.parents, ⟨⟨targetValid.1, ?_, targetValid.2.2⟩, ?_⟩⟩
  · intro parent parentMember
    have parentImmediate := targetValid.2.1 parent parentMember
    have parentAboveGc :
        (world.validatorState validator).gcRound < parent.round := by
      rw [targetRoundExact] at parentImmediate
      omega
    rcases dag.historyClosedAboveGc targetBlock targetMember parent
        parentMember parentAboveGc with
      ⟨parentBlock, parentBlockMember, parentReference⟩
    exact ⟨by
      rw [targetRoundExact] at parentImmediate
      exact parentImmediate,
      by
        rw [← parentReference]
        exact dag.historyAccepted parentBlock parentBlockMember⟩
  · intro parent parentMember
    have parentImmediate := targetValid.2.1 parent parentMember
    have parentAboveGc :
        (world.validatorState validator).gcRound < parent.round := by
      rw [targetRoundExact] at parentImmediate
      omega
    rcases dag.historyClosedAboveGc targetBlock targetMember parent
        parentMember parentAboveGc with
      ⟨parentBlock, parentBlockMember, parentReference⟩
    exact ⟨by
      rw [← parentReference]
      exact dag.historyRetained parentBlock parentBlockMember,
      Or.inr parentAboveGc⟩

/-- Lower Rust refinement contract for one commit install.

Before the install changes GC, the host has accepted and catalogued every
exact commit-body block. After the change, it retains each such block that is
above the new GC round. Applying this rule at each install accumulates usable
earlier-prefix ancestry; one current commit record need not contain that full
ancestry. This local contract alone does not derive the current bootstrap map.
That derivation also needs induction over the installed prefix and the retained
parent closure. -/
def ValidatorCommitBodyImportBeforeGcEffect
    {BlockId CommitId PacketId Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (before after : ValidatorWorldState BlockId CommitId PacketId)
    (validator : Nat) (head : ValidatorCommitHead CommitId)
    (record : ExactCommitRecord CommitId (LeaderBlockRef BlockId)) : Prop :=
  constructExactCommitReference functions record =
      { index := head.index, digest := head.id } ∧
    ∀ reference,
      reference ∈
          record.blocks.map referenceLeaderBlockToValidatorBlockRef →
        ((before.validatorState validator).accepted reference = true ∧
          (∃ block,
            before.blockCatalog reference.id = some block ∧
              block.reference = reference) ∧
          ((after.validatorState validator).gcRound < reference.round →
            (after.validatorState validator).retained reference = true))

/-- Current local storage for bootstrapping above GC from an installed head.

This is independent of the install origin. For a local install, the
implementation can project the DAG from the accumulated installed-prefix
material. For a verified synchronized install, it must first store and check
the bundle, then import its blocks into the same local catalog before GC moves.
Earlier commit bodies can supply ancestors that are absent from the current
record. The record states no future synchronization or proposal result. -/
structure ValidatorInstalledHeadBootstrapSourceMap
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  projectedBootstrapDag : ∀ time validator head,
    Option (ValidatorGcTruncatedDecisionDag config
      (timed.execution.trace time) validator head)
  positiveCurrentHeadProjectsBootstrapDag : ∀ time validator head,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (timed.execution.trace time).epochActive = true →
    ((timed.execution.trace time).validatorState validator).commitHead = head →
    0 < head.index →
    ∃ dag record,
      projectedBootstrapDag time validator head = some dag ∧
        constructExactCommitReference functions record =
          { index := head.index, digest := head.id } ∧
        referenceLeaderBlockToValidatorBlockRef record.leader =
          dag.rootBlock.reference ∧
        record.leader ∈ record.blocks ∧
        (∀ reference,
          reference ∈
              (record.blocks.map referenceLeaderBlockToValidatorBlockRef).filter
                (fun current =>
                  ((timed.execution.trace time).validatorState
                    validator).gcRound < current.round) →
          reference ∈ dag.history.map ValidatorBlock.reference)
  /-- A positive GC boundary cannot belong to the genesis commit head. -/
  positiveGcCurrentHeadIsNonGenesis : ∀ time validator head,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (timed.execution.trace time).epochActive = true →
    ((timed.execution.trace time).validatorState validator).commitHead = head →
    0 < ((timed.execution.trace time).validatorState validator).gcRound →
    0 < head.index
  /-- After GC moves above genesis, the deterministic GC policy retains at
  least one full parent round below each positive current head. Early heads at
  GC round zero use the separate genesis bootstrap. -/
  positiveGcCurrentHeadHasPostGcParentRound : ∀ time validator head,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (timed.execution.trace time).epochActive = true →
    ((timed.execution.trace time).validatorState validator).commitHead = head →
    0 < head.index →
    0 < ((timed.execution.trace time).validatorState validator).gcRound →
    ((timed.execution.trace time).validatorState validator).gcRound + 2 ≤
      head.round

/-- A correct host with a positive installed head and a signer floor behind
its GC bootstrap round has a legal fresh normal parent list now.

The target is the normal accumulator's canonical target. All parent bodies are
already accepted and retained above GC. -/
theorem installed_head_bootstrap_gives_fresh_normal_parent_list
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (source : ValidatorInstalledHeadBootstrapSourceMap functions timed)
    {time validator : Time} {head : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (epochActive : (timed.execution.trace time).epochActive = true)
    (currentHead :
      ((timed.execution.trace time).validatorState validator).commitHead = head)
    (gcHasMoved :
      0 < ((timed.execution.trace time).validatorState validator).gcRound)
    (signerFloorBehindBootstrap :
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1 ≤
      ((timed.execution.trace time).validatorState validator).gcRound + 2) :
    ∃ parents,
      ValidatorProposalParentListReady .normal config
        ((timed.execution.trace time).validatorState validator)
        (Nat.max
          (((timed.execution.trace time).validatorState
            validator).highestSignedRound + 1)
        (((timed.execution.trace time).validatorState validator).gcRound + 2))
        parents := by
  have positiveHead := source.positiveGcCurrentHeadIsNonGenesis time validator
    head validatorInRange validatorCorrectAvailable epochActive currentHead
      gcHasMoved
  rcases source.positiveCurrentHeadProjectsBootstrapDag time validator head
      validatorInRange validatorCorrectAvailable epochActive currentHead
        positiveHead with
    ⟨dag, _record, _projected, _exactReference, _exactRoot, _leaderInBody,
      _currentBodyInHistory⟩
  have canonicalTarget :
      Nat.max
          (((timed.execution.trace time).validatorState
            validator).highestSignedRound + 1)
          (((timed.execution.trace time).validatorState validator).gcRound + 2) =
        ((timed.execution.trace time).validatorState validator).gcRound + 2 :=
    Nat.max_eq_right signerFloorBehindBootstrap
  have targetAtMostHead :
      Nat.max
          (((timed.execution.trace time).validatorState
            validator).highestSignedRound + 1)
          (((timed.execution.trace time).validatorState validator).gcRound + 2) ≤
        head.round := by
    rw [canonicalTarget]
    exact source.positiveGcCurrentHeadHasPostGcParentRound time validator head
      validatorInRange validatorCorrectAvailable epochActive currentHead
        positiveHead gcHasMoved
  apply gc_truncated_decision_dag_gives_normal_parent_list dag
  · rw [canonicalTarget]
    omega
  · exact targetAtMostHead

/-- Commit indexes do not decrease inside one local event batch. -/
private theorem normal_liveness_world_step_commit_index_monotone
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
    (validator : Nat) :
    (before.validatorState validator).commitHead.index ≤
      (after.validatorState validator).commitHead.index := by
  induction step with
  | nil => exact Nat.le_refl _
  | cons firstStep remainingSteps inductionHypothesis =>
      exact Nat.le_trans
        (validator_atomic_step_durable_monotone firstStep validator).1
        inductionHypothesis

private theorem commit_install_action_occurrence_advances_commit_index
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {time validator : Time} {head : ValidatorCommitHead CommitId}
    {action : ValidatorLocalAction BlockId CommitId}
    (installAction : action = .recordCommit head ∨
      action = .applySyncedCommit head)
    (occurs : ValidatorLocalActionOccurs (timed.execution.events time)
      validator action) :
    ((timed.execution.trace time).validatorState validator).commitHead.index <
      ((timed.execution.trace (time + 1)).validatorState
        validator).commitHead.index := by
  rcases occurs with ⟨headEvents, tailEvents, eventsExact⟩
  have step := timed.execution.stepsFollowRules time
  rw [eventsExact] at step
  rcases validator_world_step_append_split step with
    ⟨actionBefore, prefixStep, actionAndSuffix⟩
  cases actionAndSuffix with
  | cons actionStep suffixStep =>
      rename_i actionAfter
      have prefixMonotone :=
        normal_liveness_world_step_commit_index_monotone prefixStep validator
      have suffixMonotone :=
        normal_liveness_world_step_commit_index_monotone suffixStep validator
      have guard := validator_atomic_local_action_has_basic_guard actionStep
      have guardIndex : head.index =
          (actionBefore.validatorState validator).commitHead.index + 1 := by
        rcases installAction with actionExact | actionExact
        · subst action
          simpa [BasicValidatorActionGuard] using guard
        · subst action
          simpa [BasicValidatorActionGuard] using guard
      have structural :=
        validator_atomic_local_action_has_structural_effect actionStep
      have installed :
          (actionAfter.validatorState validator).commitHead = head := by
        rcases installAction with actionExact | actionExact
        · subst action
          exact structural.2.2.2.2.2.1
        · subst action
          exact structural.2.2.2.2.2.1
      calc
        ((timed.execution.trace time).validatorState
            validator).commitHead.index ≤
            (actionBefore.validatorState validator).commitHead.index :=
          prefixMonotone
        _ < head.index := by omega
        _ = (actionAfter.validatorState validator).commitHead.index := by
          rw [installed]
        _ ≤ ((timed.execution.trace (time + 1)).validatorState
            validator).commitHead.index := suffixMonotone

/-- Every actual local commit install strictly advances that host's commit
index. Other events in the same batch cannot undo the advance. -/
theorem commit_install_occurrence_advances_commit_index
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {time validator : Time} {head : ValidatorCommitHead CommitId}
    (installed : ValidatorCommitInstallOccurs (timed.execution.events time)
      validator head) :
    ((timed.execution.trace time).validatorState validator).commitHead.index <
      ((timed.execution.trace (time + 1)).validatorState
        validator).commitHead.index := by
  rcases installed with recorded | synchronized
  · exact commit_install_action_occurrence_advances_commit_index timed
      (Or.inl rfl) recorded
  · exact commit_install_action_occurrence_advances_commit_index timed
      (Or.inr rfl) synchronized

/-- A local commit record is one concrete commit-install occurrence. -/
theorem record_commit_occurrence_advances_commit_index
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {time validator : Time} {head : ValidatorCommitHead CommitId}
    (recorded : ValidatorLocalActionOccurs (timed.execution.events time)
      validator (.recordCommit head)) :
    ((timed.execution.trace time).validatorState validator).commitHead.index <
      ((timed.execution.trace (time + 1)).validatorState
        validator).commitHead.index :=
  commit_install_occurrence_advances_commit_index timed (Or.inl recorded)

/-- Static genesis storage supplies the local round-one timer input. -/
theorem canonical_genesis_parent_rules_give_ready
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (genesis : ValidatorCanonicalGenesisParentRules timed)
    {time validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (signerFloorIsZero :
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound = 0)
    (noArmedTimer :
      ((timed.execution.trace time).validatorState
        validator).recovery.isNone = true)
    (epochActive : (timed.execution.trace time).epochActive = true) :
    ValidatorCanonicalGenesisParentReadyAt timed time validator := by
  refine ⟨signerFloorIsZero, noArmedTimer, epochActive, genesis.parents, ?_⟩
  refine ⟨⟨genesis.parentAuthorsNodup, ?_,
    genesis.parentStakeIsQuorum⟩, ?_⟩
  · intro parent parentMember
    refine ⟨?_, ?_⟩
    · rw [genesis.parentsAreRoundZero parent parentMember]
    · exact timed.execution.accepted_block_persists validatorInRange
        (Nat.zero_le time)
        (genesis.initiallyAccepted validator parent validatorInRange
          validatorCorrectAvailable parentMember)
  · intro parent parentMember
    exact ⟨genesis.retainedAtCorrectValidator time validator parent
      validatorInRange validatorCorrectAvailable parentMember,
      Or.inl (genesis.parentsAreRoundZero parent parentMember)⟩

/-- An actual installed-commit window supplies one current normal parent list.

The proof counts each author once. A Byzantine author's selected branch can
differ from the branch in the installed DAG, but it must be a current accepted
and retained representative at the same round. -/
theorem installed_commit_parent_window_gives_normal_parent_list
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (source : ValidatorInstalledCommitParentSourceMap functions timed)
    {time validator targetRound : Time}
    {head : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (installed : ValidatorLocalActionOccurs
      (timed.execution.events time) validator (.recordCommit head))
    (targetParentsAboveGc :
      ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 1 <
        targetRound)
    (targetAtMostFrontier : targetRound ≤ head.round) :
    ∃ parents,
      ValidatorProposalParentListReady .normal config
          ((timed.execution.trace (time + 1)).validatorState validator)
          targetRound parents ∧
        (∀ parent, parent ∈ parents →
          parent.author < config.authorityCount) ∧
        (∀ parent, parent ∈ parents →
          ((timed.execution.trace (time + 1)).validatorState
            validator).acceptedRepresentative (targetRound - 1)
              parent.author = some parent) ∧
        (∀ author parent,
          author < config.authorityCount →
          ((timed.execution.trace (time + 1)).validatorState
            validator).acceptedRepresentative (targetRound - 1) author =
              some parent →
          parent ∈ parents) := by
  rcases source.recordCommitActionProjectsDecisionDag time validator head
      targetRound validatorInRange validatorCorrectAvailable installed
        targetParentsAboveGc targetAtMostFrontier with
    ⟨dag, _record, projected, _exactReference, _recordLeader,
      _leaderInBody, _historyFromBody⟩
  rcases gc_truncated_decision_dag_has_valid_block_at_round dag
      (by omega) targetAtMostFrontier with
    ⟨targetBlock, targetBlockMember, targetBlockRound, targetBlockValid⟩
  let parents := source.selectedParents time validator head targetRound
  have localWellFormed := timed.execution.statesWellFormed (time + 1)
    validator validatorInRange
  have targetPositive : 0 < targetRound :=
    Nat.lt_trans (Nat.zero_lt_succ _) targetParentsAboveGc
  have selectedFacts : ∀ parent, parent ∈ parents →
      parent.author < config.authorityCount ∧
        parent.round + 1 = targetRound ∧
        ((timed.execution.trace (time + 1)).validatorState
          validator).accepted parent = true := by
    intro parent parentMember
    have representative := source.selectedParentIsCurrentRepresentative time
      validator head targetRound dag parent validatorInRange
        validatorCorrectAvailable installed projected targetParentsAboveGc
        targetAtMostFrontier parentMember
    have sound := localWellFormed.acceptedRepresentativeIsSound
      (targetRound - 1) parent.author parent representative
    have parentRound : parent.round = targetRound - 1 := sound.2.1
    have parentInRange := source.selectedParentAuthorsInRange time validator head
      targetRound dag parent validatorInRange validatorCorrectAvailable installed
        projected targetParentsAboveGc targetAtMostFrontier parentMember
    refine ⟨parentInRange, ?_, sound.2.2.1⟩
    calc
      parent.round + 1 = (targetRound - 1) + 1 := by rw [parentRound]
      _ = targetRound := Nat.sub_add_cancel (Nat.succ_le_iff.mpr targetPositive)
  have parentAuthorSubset : VoterSet.SubsetAt config.authorityCount
      targetBlock.parentAuthors (validatorParentAuthors parents) := by
    intro author authorInRange parentAuthor
    simp [ValidatorBlock.parentAuthors] at parentAuthor
    rcases parentAuthor with ⟨parent, parentMember, parentAuthorExact⟩
    have parentAuthorInRange : parent.author < config.authorityCount := by
      simpa [parentAuthorExact] using authorInRange
    rcases source.selectedParentsCoverDecisionBlockAuthors time validator head
        targetRound dag targetBlock parent validatorInRange
          validatorCorrectAvailable installed projected targetParentsAboveGc
          targetAtMostFrontier targetBlockMember targetBlockRound parentMember
          parentAuthorInRange with
      ⟨selected, selectedMember, selectedAuthor⟩
    simp [validatorParentAuthors]
    exact ⟨selected, selectedMember,
      selectedAuthor.trans parentAuthorExact⟩
  have selectedQuorum : config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (validatorParentAuthors parents) := by
    exact Nat.le_trans targetBlockValid.2.2
      (weight_mono config.stake parentAuthorSubset)
  refine ⟨parents, ⟨⟨?_, ?_, selectedQuorum⟩, ?_⟩, ?_, ?_, ?_⟩
  · exact source.selectedParentAuthorsNodup time validator head targetRound
      dag validatorInRange validatorCorrectAvailable installed projected
        targetParentsAboveGc targetAtMostFrontier
  · intro parent parentMember
    exact ⟨(selectedFacts parent parentMember).2.1,
      (selectedFacts parent parentMember).2.2⟩
  · intro parent parentMember
    have retained := source.selectedParentIsRetained time validator head
      targetRound dag parent validatorInRange validatorCorrectAvailable installed
        projected targetParentsAboveGc targetAtMostFrontier parentMember
    have parentRound := (selectedFacts parent parentMember).2.1
    exact ⟨retained, Or.inr (by omega)⟩
  · intro parent parentMember
    exact (selectedFacts parent parentMember).1
  · intro parent parentMember
    exact source.selectedParentIsCurrentRepresentative time validator head
      targetRound dag parent validatorInRange validatorCorrectAvailable installed
        projected targetParentsAboveGc targetAtMostFrontier parentMember
  · intro author parent authorInRange representative
    exact source.selectedParentsIncludeCurrentRepresentatives time validator
      head targetRound dag author parent validatorInRange
        validatorCorrectAvailable installed projected targetParentsAboveGc
        targetAtMostFrontier authorInRange representative

/-- A local record keeps one complete current parent list usable when its
parent round remains above the new GC boundary. -/
theorem record_commit_preserves_above_gc_normal_parent_list
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (source : ValidatorInstalledCommitParentSourceMap functions timed)
    {time validator targetRound : Time}
    {head : ValidatorCommitHead CommitId}
    {parents : List (ValidatorBlockRef BlockId)}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (recorded : ValidatorLocalActionOccurs (timed.execution.events time)
      validator (.recordCommit head))
    (parentsReady : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace time).validatorState validator)
      targetRound parents)
    (parentsInRange : ∀ parent, parent ∈ parents →
      parent.author < config.authorityCount)
    (parentsAreRepresentatives : ∀ parent, parent ∈ parents →
      ((timed.execution.trace time).validatorState
        validator).acceptedRepresentative (targetRound - 1)
          parent.author = some parent)
    (targetParentsAboveGc :
      ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 1 <
        targetRound) :
    ValidatorProposalParentListReady .normal config
        ((timed.execution.trace (time + 1)).validatorState validator)
        targetRound parents ∧
      (∀ parent, parent ∈ parents →
        ((timed.execution.trace (time + 1)).validatorState
          validator).acceptedRepresentative (targetRound - 1)
            parent.author = some parent) := by
  have durable := timed.execution.durableStateMonotone validator time (time + 1)
    validatorInRange (Nat.le_succ _)
  refine ⟨⟨⟨parentsReady.1.1, ?_, parentsReady.1.2.2⟩, ?_⟩, ?_⟩
  · intro parent parentMember
    have before := parentsReady.1.2.1 parent parentMember
    exact ⟨before.1, durable.accepted_block_persists before.2⟩
  · intro parent parentMember
    have parentRound := (parentsReady.1.2.1 parent parentMember).1
    have parentAboveGc :
        ((timed.execution.trace (time + 1)).validatorState validator).gcRound <
          parent.round := by
      omega
    exact ⟨source.recordCommitRepinsAboveGcRepresentative time validator head
      (targetRound - 1) parent.author parent validatorInRange
        validatorCorrectAvailable recorded
          (parentsAreRepresentatives parent parentMember) parentAboveGc,
      Or.inr parentAboveGc⟩
  · intro parent parentMember
    exact durable.accepted_representative_persists
      (parentsAreRepresentatives parent parentMember)

/-- Distinct authors imply distinct block references. -/
private theorem normal_parent_refs_nodup_of_authors_nodup
    {BlockId : Type} {parents : List (ValidatorBlockRef BlockId)}
    (authorsNodup : (parents.map ValidatorBlockRef.author).Nodup) :
    parents.Nodup := by
  induction parents with
  | nil => simp
  | cons parent tail inductionHypothesis =>
      simp only [List.map_cons, List.nodup_cons] at authorsNodup ⊢
      constructor
      · intro parentInTail
        exact authorsNodup.1 (List.mem_map.mpr
          ⟨parent, parentInTail, rfl⟩)
      · exact inductionHypothesis authorsNodup.2

/-- Every reference in the explicit one-per-author parent list comes from one
in-range selector entry. -/
private theorem selected_parent_refs_from_member_has_choice
    {BlockId : Type}
    {choice : Nat → Option (ValidatorBlockRef BlockId)}
    {authorityCount : Nat} {parent : ValidatorBlockRef BlockId}
    (member : parent ∈
      ImmediateParentSelection.selectedParentRefsFrom authorityCount choice) :
    ∃ author,
      author < authorityCount ∧ choice author = some parent := by
  induction authorityCount with
  | zero => simp [ImmediateParentSelection.selectedParentRefsFrom] at member
  | succ count inductionHypothesis =>
      cases selected : choice count with
      | none =>
          have earlier : parent ∈
              ImmediateParentSelection.selectedParentRefsFrom count choice := by
            simpa [ImmediateParentSelection.selectedParentRefsFrom, selected]
              using member
          rcases inductionHypothesis earlier with
            ⟨author, authorInRange, chosen⟩
          exact ⟨author, Nat.lt_succ_of_lt authorInRange, chosen⟩
      | some lastParent =>
          have cases : parent ∈
                ImmediateParentSelection.selectedParentRefsFrom count choice ∨
              parent = lastParent := by
            simpa [ImmediateParentSelection.selectedParentRefsFrom, selected]
              using member
          rcases cases with earlier | last
          · rcases inductionHypothesis earlier with
              ⟨author, authorInRange, chosen⟩
            exact ⟨author, Nat.lt_succ_of_lt authorInRange, chosen⟩
          · subst parent
            exact ⟨count, Nat.lt_succ_self count, selected⟩

/-- One current normal parent list becomes a compatible local accumulator
source. This construction stores no remote holder or future result. -/
theorem normal_parent_list_gives_compatible_local_source
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {time validator : Time}
    {need : ValidatorRecoveryParentNeed BlockId CommitId config}
    {parents : List (ValidatorBlockRef BlockId)}
    (normalOrigin : need.proposalOrigin = .normal)
    (signerFloorMatches : need.signerFloor =
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound)
    (parentsReady : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace time).validatorState validator)
      need.targetRound parents)
    (parentsAreRepresentatives : ∀ parent, parent ∈ parents →
      ((timed.execution.trace time).validatorState
        validator).acceptedRepresentative (need.targetRound - 1)
          parent.author = some parent)
    (parentsIncludeRepresentatives : ∀ author parent,
      author < config.authorityCount →
      ((timed.execution.trace time).validatorState
        validator).acceptedRepresentative (need.targetRound - 1) author =
          some parent →
      parent ∈ parents) :
    ∃ sourceNeed,
      ValidatorLocalAccumulatorNeedSourceAt timed time validator sourceNeed ∧
        sourceNeed.signerFloor = need.signerFloor ∧
        sourceNeed.targetRound = need.targetRound ∧
        ValidatorRecoveryParentNeedReadyAt
          ((timed.execution.trace time).validatorState validator) sourceNeed := by
  let sourceNeed : ValidatorRecoveryParentNeed BlockId CommitId config :=
    { proposalOrigin := .normal
      discoveryOrigin := .localAccumulator
      capsuleKey := none
      baselineCommit :=
        ((timed.execution.trace time).validatorState validator).commitHead
      signerFloor :=
        ((timed.execution.trace time).validatorState
          validator).highestSignedRound
      targetRound := need.targetRound
      targetAboveSignerFloor := by
        rw [← signerFloorMatches]
        exact need.targetAboveSignerFloor
      recoveryTargetIsExactNext := by simp
      sourceBlock := none
      candidateRefs := parents
      candidateRefsNodup :=
        normal_parent_refs_nodup_of_authors_nodup parentsReady.1.1
      deliveredChildSourceShape := by simp
      deliveredChildHasNoCapsuleKey := by simp
      pinnedTipHasNoChild := by simp
      pinnedTipHasCapsuleKey := by simp
      localAccumulatorHasNoChild := by simp
      localAccumulatorHasNoCapsuleKey := by simp
      candidatesAreImmediate := fun reference member =>
        (parentsReady.1.2.1 reference member).1 }
  have localSource : ValidatorLocalAccumulatorNeedSourceAt timed time validator
      sourceNeed := by
    refine ⟨rfl, rfl, rfl, rfl, ?_, ?_⟩
    · intro reference member
      have immediate := (parentsReady.1.2.1 reference member).1
      have representative := parentsAreRepresentatives reference member
      simpa [sourceNeed, show reference.round = need.targetRound - 1 by omega]
        using representative
    · intro author reference authorInRange representative
      exact parentsIncludeRepresentatives author reference authorInRange
        (by simpa [sourceNeed] using representative)
  have sourceReady : ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) sourceNeed := by
    exact ⟨parents, fun _ member => member, by
      simpa [sourceNeed] using parentsReady⟩
  exact ⟨sourceNeed, localSource, by
    simp [sourceNeed, signerFloorMatches], rfl, sourceReady⟩

/-- A retained local causal DAG gives a normal quorum parent list for any
positive target below its frontier. The GC fence belongs to the preserved
normal need. No future block layer is an input. -/
theorem accepted_causal_history_supplies_normal_parent_list
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    (state : ValidatorLocalState BlockId CommitId)
    {targetRound : Nat}
    (targetPositive : 0 < targetRound)
    (targetBelowFrontier : targetRound < capsule.targetRound)
    (historyAccepted : ∀ block, block ∈ capsule.history →
      state.accepted block.reference = true)
    (historyRetained : ∀ block, block ∈ capsule.history →
      state.retained block.reference = true)
    (genesisUsable : ∀ reference : ValidatorBlockRef BlockId,
      reference.round = 0 →
      state.accepted reference = true ∧ state.retained reference = true)
    (gcFence : targetRound = 1 ∨ state.gcRound + 1 < targetRound) :
    ∃ parents,
      ValidatorProposalParentListReady .normal config state targetRound
        parents := by
  let floor := targetRound - 1
  have floorBelowFrontier : floor < capsule.targetRound := by
    dsimp [floor]
    omega
  rcases causal_history_supplies_exact_next_parent_list capsule
      floorBelowFrontier with
    ⟨child, _childMember, childRound, childValid, parentSources⟩
  have floorNext : floor + 1 = targetRound := by
    dsimp [floor]
    omega
  refine ⟨child.parents, ⟨childValid.1, ?_, childValid.2.2⟩, ?_⟩
  · intro parent parentMember
    have source := parentSources parent parentMember
    refine ⟨?_, ?_⟩
    · rw [← floorNext, ← childRound]
      exact childValid.2.1 parent parentMember
    · rcases source.2 with parentGenesis | ⟨parentBlock, parentMember,
          parentExact⟩
      · exact (genesisUsable parent parentGenesis).1
      · rw [← parentExact]
        exact historyAccepted parentBlock parentMember
  · intro parent parentMember
    have source := parentSources parent parentMember
    have retained : state.retained parent = true := by
      rcases source.2 with parentGenesis | ⟨parentBlock, parentMember,
          parentExact⟩
      · exact (genesisUsable parent parentGenesis).2
      · rw [← parentExact]
        exact historyRetained parentBlock parentMember
    refine ⟨retained, ?_⟩
    have parentRound : parent.round + 1 = targetRound := by
      rw [← floorNext, ← childRound]
      exact childValid.2.1 parent parentMember
    rcases gcFence with targetOne | gcBelowTarget
    · exact Or.inl (by omega)
    · exact Or.inr (by omega)

/-- One local proposal or send work item at one execution time. -/
inductive ValidatorProposalWorkAt
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
    (time validator round : Nat) : Prop where
  | ready (proposal : ValidatorReadyProposal BlockId) :
      (obligations.trace time validator).readyProposal = some proposal →
      proposal.block.reference.round = round →
      ValidatorProposalWorkAt obligations time validator round
  | persistedUnsent
      (reference : ValidatorBlockRef BlockId) (receiver : Nat) :
      ((timed.execution.trace time).validatorState validator).highestSignedRound =
        round →
      ((timed.execution.trace time).validatorState validator).ownBlockAt round =
        some reference →
      ((timed.execution.trace time).validatorState validator).sentOwnBlockAt
          round = false →
      receiver < config.authorityCount →
      receiver ≠ validator →
      (obligations.trace time validator).sendGoal reference receiver = true →
      ValidatorProposalWorkAt obligations time validator round

/-- One current exact-next proposer action. Protection makes this snapshot
action stable until it runs. -/
structure ValidatorExactNextProposalAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Nat) where
  parents : List (ValidatorBlockRef BlockId)
  actionProtected : timed.protectedAction time validator (.proposeNext parents)

/-- One protected normal proposal-builder action at the exact next signer
round. Parent readiness is part of the concrete action guard. -/
structure ValidatorNormalExactNextProposalAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Nat) where
  targetRound : Nat
  parents : List (ValidatorBlockRef BlockId)
  targetIsExactNext : targetRound =
    ((timed.execution.trace time).validatorState
      validator).highestSignedRound + 1
  actionProtected : timed.protectedAction time validator
    (.proposeNormal targetRound parents)

/-- One protected normal proposal-builder action. Normal operation can choose
a higher legal target after a commit or GC change. -/
structure ValidatorNormalProposalAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Nat) where
  targetRound : Nat
  parents : List (ValidatorBlockRef BlockId)
  actionProtected : timed.protectedAction time validator
    (.proposeNormal targetRound parents)

/-- The one-host boundary for a concrete normal proposal attempt.

The implementation-specific guard records that `try_propose` can run for the
chosen target. Once Core starts that synchronous attempt, it does not discard
the work before the proposal action runs. This rule does not say that every
ready parent layer starts an attempt. Leader selection, an immediate Core call,
or a leader-timeout callback must first make the program guard true. -/
structure ValidatorReadyNormalProposalProtectionRule
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) : Prop where
  readyProgramActionIsProtected : ∀ time validator targetRound parents,
    (timed.execution.trace time).epochActive = true →
    ((timed.execution.trace time).validatorState
      validator).highestSignedRound < targetRound →
    ValidatorProposalParentListReady .normal config
      ((timed.execution.trace time).validatorState validator) targetRound
        parents →
    program.actions.enabled validator (.proposeNormal targetRound parents)
      ((timed.execution.trace time).validatorState validator) →
    timed.protectedAction time validator
      (.proposeNormal targetRound parents)

/-- A persistent parent need keeps the signer floor fixed until it changes
into one protected exact-next action. -/
structure ValidatorPinnedExactNextProposalAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (needAt readyAt validator : Nat) where
  sameSignerFloor :
    ((timed.execution.trace readyAt).validatorState
      validator).highestSignedRound =
        ((timed.execution.trace needAt).validatorState
          validator).highestSignedRound
  proposal : ValidatorExactNextProposalAt timed readyAt validator

/-- A local parent need for one exact-next proposal.

The persistence action identifies the finite causal history. This local phase
does not contain a remote holder or assume that a holder still serves it. -/
structure ValidatorExactNextParentSyncAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (time validator : Nat) where
  persistTime : Time
  author : Nat
  block : ValidatorBlock BlockId
  persistenceBeforeNeed : persistTime + 1 ≤ time

/-- A local parent need whose causal source is a durable signer tip from the
restart snapshot. The phase contains only the source owner and local need. -/
structure ValidatorExactNextRestartParentSyncAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (snapshot time validator : Nat) where
  author : Nat
  snapshotBeforeNeed : snapshot ≤ time

/-- Source provenance for requester-local parent needs.

This record maps a local need to either the concrete persistence action that
created its source or the durable owner tip at the restart snapshot. It does
not assume delivery or completed synchronization. -/
structure ValidatorParentNeedSourceRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (restartSnapshot : Time) where
  persistedNeedWasProduced : ∀ time validator
      (need : ValidatorExactNextParentSyncAt (timed := timed) time validator),
    ValidatorLocalActionOccurs (timed.execution.events need.persistTime)
      need.author (.persistProposal need.block)
  restartNeedOwnerInRange : ∀ time validator
      (need : ValidatorExactNextRestartParentSyncAt
        (timed := timed) restartSnapshot time validator),
    need.author < config.authorityCount
  restartNeedOwnerCorrectAvailable : ∀ time validator
      (need : ValidatorExactNextRestartParentSyncAt
        (timed := timed) restartSnapshot time validator),
    faults.correctAvailable need.author = true
  restartNeedOwnerHasPositiveTip : ∀ time validator
      (need : ValidatorExactNextRestartParentSyncAt
        (timed := timed) restartSnapshot time validator),
    0 < ((timed.execution.trace restartSnapshot).validatorState
      need.author).highestSignedRound

/-- Same-state requester rules after all blocks for one local parent need are
accepted. The rule latches one protected exact-next proposer action. -/
structure ValidatorParentNeedProposalRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (restartSnapshot : Time) where
  persistedNeedAcceptedEnables : ∀ time validator
      (need : ValidatorExactNextParentSyncAt (timed := timed) time validator)
      (capsule : CausalRecoveryCapsule (BlockId := BlockId) config),
    capsule.targetBlock = need.block →
    ∀ current,
      time ≤ current →
      (∀ item, item ∈ capsule.history →
        ((timed.execution.trace current).validatorState validator).accepted
          item.reference = true) →
      ValidatorPinnedExactNextProposalAt timed time current validator
  restartNeedAcceptedEnables : ∀ time validator
      (need : ValidatorExactNextRestartParentSyncAt
        (timed := timed) restartSnapshot time validator)
      (block : ValidatorBlock BlockId)
      (capsule : CausalRecoveryCapsule (BlockId := BlockId) config),
    ((timed.execution.trace restartSnapshot).validatorState
        need.author).ownBlockAt
        ((timed.execution.trace restartSnapshot).validatorState
          need.author).highestSignedRound = some block.reference →
    capsule.targetBlock = block →
    ∀ current,
      time ≤ current →
      (∀ item, item ∈ capsule.history →
        ((timed.execution.trace current).validatorState validator).accepted
          item.reference = true) →
      ValidatorPinnedExactNextProposalAt timed time current validator

/-- The current local proposal-engine phase. Every constructor contains only
facts about the current host state and protected local work. -/
inductive ValidatorStrictProposalPhaseAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    (obligations : ValidatorProposalObligationExecution timed)
    (restartSnapshot : Time)
    (time validator : Nat) : Prop where
  | ready {round : Nat} {proposal : ValidatorReadyProposal BlockId} :
      (obligations.trace time validator).readyProposal = some proposal →
      proposal.block.reference.round = round →
      ValidatorStrictProposalPhaseAt syncRules obligations restartSnapshot time
        validator
  | exactNext : ValidatorExactNextProposalAt timed time validator →
      ValidatorStrictProposalPhaseAt syncRules obligations restartSnapshot time
        validator
  | persistedUnsent
      {round : Nat} {reference : ValidatorBlockRef BlockId} {receiver : Nat} :
      ((timed.execution.trace time).validatorState validator).highestSignedRound =
        round →
      ((timed.execution.trace time).validatorState validator).ownBlockAt round =
        some reference →
      ((timed.execution.trace time).validatorState validator).sentOwnBlockAt
          round = false →
      receiver < config.authorityCount →
      receiver ≠ validator →
      (obligations.trace time validator).sendGoal reference receiver = true →
      ValidatorStrictProposalPhaseAt syncRules obligations restartSnapshot time
        validator
  | parentSync : ValidatorExactNextParentSyncAt (timed := timed) time validator →
      ValidatorStrictProposalPhaseAt syncRules obligations restartSnapshot time
        validator
  | restartParentSync : ValidatorExactNextRestartParentSyncAt
        (timed := timed) restartSnapshot time validator →
      ValidatorStrictProposalPhaseAt syncRules obligations restartSnapshot time
        validator

/-- A source mapping for the one-host persistent proposal engine.

The first two fields are initial and one-step transition invariants. A commit,
GC, or activation transition must preserve a current local work case. -/
structure ValidatorStrictProposalRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    (obligations : ValidatorProposalObligationExecution timed)
    (restartSnapshot : Time) where
  initialStateHasObligation : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (timed.execution.trace 0).epochActive = false ∨
      ValidatorStrictProposalPhaseAt syncRules obligations restartSnapshot 0
        validator
  stepPreservesObligation : ∀ time validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((timed.execution.trace time).epochActive = false ∨
      ValidatorStrictProposalPhaseAt syncRules obligations restartSnapshot time
        validator) →
    (timed.execution.trace (time + 1)).epochActive = false ∨
      ValidatorStrictProposalPhaseAt syncRules obligations restartSnapshot
        (time + 1) validator
  broadcastReceiver : Nat → Nat
  broadcastReceiverInRange : ∀ validator,
    validator < config.authorityCount →
    broadcastReceiver validator < config.authorityCount
  broadcastReceiverIsOther : ∀ validator,
    validator < config.authorityCount →
    broadcastReceiver validator ≠ validator

/-- The one-step host rule gives a current work phase at every active time. -/
theorem active_host_has_strict_proposal_phase
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {obligations : ValidatorProposalObligationExecution timed}
    {restartSnapshot time validator : Time}
    (rules : ValidatorStrictProposalRules syncRules obligations restartSnapshot)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (timed.execution.trace time).epochActive = true) :
    ValidatorStrictProposalPhaseAt syncRules obligations restartSnapshot time
      validator := by
  have invariant : ∀ current,
      (timed.execution.trace current).epochActive = false ∨
        ValidatorStrictProposalPhaseAt syncRules obligations restartSnapshot
          current validator := by
    intro current
    induction current with
    | zero =>
        exact rules.initialStateHasObligation validator validatorInRange
          validatorCorrectAvailable
    | succ previous inductionHypothesis =>
        simpa [Nat.succ_eq_add_one] using rules.stepPreservesObligation previous
          validator validatorInRange validatorCorrectAvailable
          inductionHypothesis
  rcases invariant time with inactive | phase
  · rw [active] at inactive
    contradiction
  · exact phase

/-- Recovery-only local work. A ready proposal must come from commit progress
recovery. The other cases contain exact-next work or the current unsent tip. -/
inductive ValidatorExactRecoveryPhaseAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    (obligations : ValidatorProposalObligationExecution timed)
    (restartSnapshot time validator : Time) : Prop where
  | ready {proposal : ValidatorReadyProposal BlockId} :
      (obligations.trace time validator).readyProposal = some proposal →
      proposal.origin = .commitProgressRecovery →
      ValidatorExactRecoveryPhaseAt syncRules obligations restartSnapshot time
        validator
  | exactNext : ValidatorExactNextProposalAt timed time validator →
      ValidatorExactRecoveryPhaseAt syncRules obligations restartSnapshot time
        validator
  | persistedUnsent
      {round : Nat} {reference : ValidatorBlockRef BlockId} {receiver : Nat} :
      ((timed.execution.trace time).validatorState validator).highestSignedRound =
        round →
      ((timed.execution.trace time).validatorState validator).ownBlockAt round =
        some reference →
      ((timed.execution.trace time).validatorState validator).sentOwnBlockAt
          round = false →
      receiver < config.authorityCount →
      receiver ≠ validator →
      (obligations.trace time validator).sendGoal reference receiver = true →
      ValidatorExactRecoveryPhaseAt syncRules obligations restartSnapshot time
        validator
  | parentSync : ValidatorExactNextParentSyncAt (timed := timed) time validator →
      ValidatorExactRecoveryPhaseAt syncRules obligations restartSnapshot time
        validator
  | restartParentSync : ValidatorExactNextRestartParentSyncAt
        (timed := timed) restartSnapshot time validator →
      ValidatorExactRecoveryPhaseAt syncRules obligations restartSnapshot time
        validator

/-- Initial and one-step host rules for recovery-only exact continuation. -/
structure ValidatorExactRecoveryRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    (obligations : ValidatorProposalObligationExecution timed)
    (restartSnapshot recoveryWait : Time) where
  initialStateHasRecoveryObligation : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ¬ValidatorCommitProgressRecoveryModeAt timed recoveryWait 0 validator ∨
      (timed.execution.trace 0).epochActive = false ∨
      ValidatorExactRecoveryPhaseAt syncRules obligations restartSnapshot 0
        validator
  stepPreservesRecoveryObligation : ∀ time validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (¬ValidatorCommitProgressRecoveryModeAt timed recoveryWait time validator ∨
      (timed.execution.trace time).epochActive = false ∨
      ValidatorExactRecoveryPhaseAt syncRules obligations restartSnapshot time
        validator) →
    ¬ValidatorCommitProgressRecoveryModeAt timed recoveryWait (time + 1)
        validator ∨
      (timed.execution.trace (time + 1)).epochActive = false ∨
      ValidatorExactRecoveryPhaseAt syncRules obligations restartSnapshot
        (time + 1) validator

/-- The one-step recovery rule gives an exact phase at each active recovery
state. -/
theorem active_recovery_host_has_exact_phase
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {obligations : ValidatorProposalObligationExecution timed}
    {restartSnapshot recoveryWait time validator : Time}
    (rules : ValidatorExactRecoveryRules syncRules obligations restartSnapshot
      recoveryWait)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (timed.execution.trace time).epochActive = true)
    (recovering : ValidatorCommitProgressRecoveryModeAt timed recoveryWait time
      validator) :
    ValidatorExactRecoveryPhaseAt syncRules obligations restartSnapshot time
      validator := by
  have invariant : ∀ current,
      ¬ValidatorCommitProgressRecoveryModeAt timed recoveryWait current
          validator ∨
        (timed.execution.trace current).epochActive = false ∨
        ValidatorExactRecoveryPhaseAt syncRules obligations restartSnapshot
          current validator := by
    intro current
    induction current with
    | zero =>
        exact rules.initialStateHasRecoveryObligation validator validatorInRange
          validatorCorrectAvailable
    | succ previous inductionHypothesis =>
        simpa [Nat.succ_eq_add_one] using
          rules.stepPreservesRecoveryObligation previous validator
            validatorInRange validatorCorrectAvailable inductionHypothesis
  rcases invariant time with notRecovering | inactive | phase
  · exact False.elim (notRecovering recovering)
  · rw [active] at inactive
    contradiction
  · exact phase

/-- One local work item finishes with one durable, sent own block. -/
theorem proposal_work_eventually_produces_sent_block_with_peer
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
    {time validator round : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (receiver : Nat)
    (receiverInRange : receiver < config.authorityCount)
    (receiverIsOther : receiver ≠ validator)
    (work : ValidatorProposalWorkAt obligations time validator round) :
    ∃ finish reference,
      time ≤ finish ∧
      ((timed.execution.trace finish).validatorState validator).ownBlockAt
          round = some reference ∧
      ((timed.execution.trace finish).validatorState validator).sentOwnBlockAt
          round = true := by
  cases work with
  | ready proposal ready sameRound =>
      rcases latched_proposal_run_creates_send_goal obligations validatorInRange
          validatorCorrectAvailable receiverInRange receiverIsOther ready with
        ⟨persistCompletion, readyBeforePersist, _, sendGoal⟩
      let persistedAt := persistCompletion.event.completedAt
      have storedAtPersist := persist_proposal_occurrence_stores_own_block
        timed.execution persistCompletion.occurs
      rcases latched_send_goal_runs_within_bound obligations validatorInRange
          validatorCorrectAvailable sendGoal with
        ⟨sendCompletion, persistBeforeSend, _, sendOccurs⟩
      let sendAt := sendCompletion.event.completedAt
      have persistBeforeSendAt : persistedAt + 1 ≤ sendAt := by
        simpa [persistedAt, sendAt] using persistBeforeSend
      have readyBeforePersistAt : time ≤ persistedAt := by
        simpa [persistedAt] using readyBeforePersist
      have storedAtFinish :=
        (timed.execution.durableStateMonotone validator (persistedAt + 1)
          (sendAt + 1) validatorInRange
          (Nat.le_trans persistBeforeSendAt (Nat.le_succ _))).own_block_persists
            storedAtPersist
      have sentAtFinish :=
        send_block_occurrence_records_sent_own_block timed.execution sendOccurs
      refine ⟨sendAt + 1, proposal.block.reference, ?_, ?_, ?_⟩
      · exact Nat.le_trans readyBeforePersistAt
          (Nat.le_trans (Nat.le_succ _) (Nat.le_trans persistBeforeSendAt
            (Nat.le_succ _)))
      · simpa [sameRound] using storedAtFinish
      · simpa [sameRound] using sentAtFinish
  | persistedUnsent reference receiver _ own _ receiverInRange _ sendGoal =>
      rcases latched_send_goal_runs_within_bound obligations validatorInRange
          validatorCorrectAvailable sendGoal with
        ⟨sendCompletion, timeBeforeSend, _, sendOccurs⟩
      let sendAt := sendCompletion.event.completedAt
      have timeBeforeSendAt : time ≤ sendAt := by
        simpa [sendAt] using timeBeforeSend
      have referenceRound :=
        (timed.execution.statesWellFormed time validator validatorInRange)
          |>.ownBlockIsSound round reference own |>.2.1
      have storedAtFinish :=
        (timed.execution.durableStateMonotone validator time (sendAt + 1)
          validatorInRange (by
            exact Nat.le_trans timeBeforeSendAt (Nat.le_succ _))).own_block_persists
              own
      have sentAtFinish :=
        send_block_occurrence_records_sent_own_block timed.execution sendOccurs
      refine ⟨sendAt + 1, reference, ?_, storedAtFinish, ?_⟩
      · exact Nat.le_trans timeBeforeSendAt (Nat.le_succ _)
      · simpa [referenceRound] using sentAtFinish

/-- One block-production completion from one current proposal-engine phase. -/
def ValidatorStrictPhaseProduction
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (start validator : Nat) : Prop :=
  ∃ finish round reference,
    start ≤ finish ∧
    ((timed.execution.trace start).validatorState
      validator).highestSignedRound ≤ round ∧
    ((timed.execution.trace start).validatorState validator).sentOwnBlockAt
      round = false ∧
    ((timed.execution.trace finish).validatorState validator).ownBlockAt round =
      some reference ∧
    ((timed.execution.trace finish).validatorState validator).sentOwnBlockAt
      round = true

/-- A protected recovery proposal produces and sends the exact next own block.
-/
def ValidatorExactNextProduction
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (start validator : Nat) : Prop :=
  ∃ persistTime finish block,
    start ≤ persistTime ∧
    persistTime + 1 ≤ finish ∧
    block.reference.round =
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound + 1 ∧
    ValidatorLocalActionOccurs (timed.execution.events persistTime) validator
      (.persistProposal block) ∧
    ((timed.execution.trace finish).validatorState validator).ownBlockAt
        (((timed.execution.trace start).validatorState
          validator).highestSignedRound + 1) = some block.reference ∧
    ((timed.execution.trace finish).validatorState validator).sentOwnBlockAt
        (((timed.execution.trace start).validatorState
          validator).highestSignedRound + 1) = true ∧
    ((timed.execution.trace finish).validatorState
      validator).highestSignedRound =
        ((timed.execution.trace start).validatorState
          validator).highestSignedRound + 1

/-- One normal proposal persists and sends a block strictly above the signer
floor at the start. The finish state records the same round as its signer
floor. -/
def ValidatorNormalProposalProduction
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (start validator targetRound : Nat) : Prop :=
  ∃ persistTime finish block,
    start ≤ persistTime ∧
    persistTime + 1 ≤ finish ∧
    block.reference.round = targetRound ∧
    ((timed.execution.trace start).validatorState
      validator).highestSignedRound < targetRound ∧
    ValidatorLocalActionOccurs (timed.execution.events persistTime) validator
      (.persistProposal block) ∧
    ((timed.execution.trace finish).validatorState validator).ownBlockAt
        block.reference.round = some block.reference ∧
    ((timed.execution.trace finish).validatorState validator).sentOwnBlockAt
        block.reference.round = true ∧
    ((timed.execution.trace finish).validatorState
      validator).highestSignedRound = block.reference.round

/-- One normal proposal keeps its concrete action, parent list, persisted block,
and addressed send result for every other validator. -/
structure ValidatorNormalProposalBroadcastProduction
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
    (start validator targetRound : Nat) where
  parents : List (ValidatorBlockRef BlockId)
  proposalActionAt : Time
  proposal : ValidatorReadyProposal BlockId
  persistedAt : Time
  finish : Time
  startBeforeProposalAction : start ≤ proposalActionAt
  proposalActionWithinBound :
    proposalActionAt ≤ start + timed.localActionBound
  proposalActionOccurs : ValidatorLocalActionOccurs
    (timed.execution.events proposalActionAt) validator
    (.proposeNormal targetRound parents)
  proposalLatched : obligations.event proposalActionAt validator =
    .latchProposal proposal
  proposalLatchedAt : proposal.latchedAt = proposalActionAt + 1
  proposalOrigin : proposal.origin = .normal
  proposalAuthor : proposal.block.reference.author = validator
  proposalRound : proposal.block.reference.round = targetRound
  proposalParents : proposal.block.parents = parents
  targetAboveStartFloor :
    ((timed.execution.trace start).validatorState
      validator).highestSignedRound < targetRound
  proposalBeforePersistence : proposalActionAt + 1 ≤ persistedAt
  persistenceBeforeFinish : persistedAt + 1 ≤ finish
  persistenceOccurs : ValidatorLocalActionOccurs
    (timed.execution.events persistedAt) validator
    (.persistProposal proposal.block)
  ownBlockStoredAtFinish :
    ((timed.execution.trace finish).validatorState validator).ownBlockAt
      proposal.block.reference.round = some proposal.block.reference
  sentOwnBlockAtFinish :
    ((timed.execution.trace finish).validatorState validator).sentOwnBlockAt
      proposal.block.reference.round = true
  signerFloorAtFinish :
    ((timed.execution.trace finish).validatorState validator).highestSignedRound =
      proposal.block.reference.round
  broadcasts : ∀ receiver,
    receiver < config.authorityCount →
    receiver ≠ validator →
    Nonempty (ValidatorLatchedProposalBroadcast timed obligations
      (proposalActionAt + 1) validator receiver proposal)

/-- One persisted proposal keeps its concrete latch origin, exact block, and
addressed send result for every other validator. The proposal can have normal
or commit-progress-recovery origin, and its latch can be before `start`. -/
structure ValidatorPersistedProposalBroadcastProduction
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
    (start validator : Nat) where
  readyAt : Time
  latchTime : Time
  proposal : ValidatorReadyProposal BlockId
  persistedAt : Time
  finish : Time
  startBeforePersistence : start ≤ persistedAt
  latchBeforeReady : latchTime < readyAt
  proposalLatchedAt : proposal.latchedAt = latchTime + 1
  proposalLatchOrigin :
    ValidatorProposalLatchMainOriginAt timed latchTime validator proposal
  proposalAuthor : proposal.block.reference.author = validator
  proposalRoundAboveStartFloor :
    ((timed.execution.trace start).validatorState
      validator).highestSignedRound < proposal.block.reference.round
  persistenceBeforeFinish : persistedAt + 1 ≤ finish
  persistenceOccurs : ValidatorLocalActionOccurs
    (timed.execution.events persistedAt) validator
    (.persistProposal proposal.block)
  ownBlockStoredAtFinish :
    ((timed.execution.trace finish).validatorState validator).ownBlockAt
      proposal.block.reference.round = some proposal.block.reference
  sentOwnBlockAtFinish :
    ((timed.execution.trace finish).validatorState validator).sentOwnBlockAt
      proposal.block.reference.round = true
  signerFloorAtFinish :
    ((timed.execution.trace finish).validatorState validator).highestSignedRound =
      proposal.block.reference.round
  broadcasts : ∀ receiver,
    receiver < config.authorityCount →
    receiver ≠ validator →
    Nonempty (ValidatorLatchedProposalBroadcast timed obligations readyAt
      validator receiver proposal)

/-- Forget the concrete proposal action and addressed broadcasts. -/
theorem ValidatorNormalProposalBroadcastProduction.toProduction
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
    {start validator targetRound : Nat}
    (production : ValidatorNormalProposalBroadcastProduction timed obligations
      start validator targetRound) :
    ValidatorNormalProposalProduction timed start validator targetRound := by
  exact ⟨production.persistedAt, production.finish, production.proposal.block,
    Nat.le_trans production.startBeforeProposalAction
      (Nat.le_trans (Nat.le_succ _) production.proposalBeforePersistence),
    production.persistenceBeforeFinish, production.proposalRound,
    production.targetAboveStartFloor, production.persistenceOccurs,
    production.ownBlockStoredAtFinish, production.sentOwnBlockAtFinish,
    production.signerFloorAtFinish⟩

/-- Production that starts after an unchanged signer floor also proves
production from the earlier state. -/
theorem normal_proposal_production_starts_at_same_floor
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {earlier later validator targetRound : Nat}
    (earlierBeforeLater : earlier ≤ later)
    (sameFloor :
      ((timed.execution.trace earlier).validatorState
          validator).highestSignedRound =
        ((timed.execution.trace later).validatorState
          validator).highestSignedRound)
    (production :
      ValidatorNormalProposalProduction timed later validator targetRound) :
    ValidatorNormalProposalProduction timed earlier validator targetRound := by
  rcases production with
    ⟨persistTime, finish, block, laterBeforePersist, persistBeforeFinish,
      blockRound, aboveLaterFloor, persisted, stored, sent, floorAtFinish⟩
  refine ⟨persistTime, finish, block,
    Nat.le_trans earlierBeforeLater laterBeforePersist, persistBeforeFinish,
    blockRound, ?_, persisted, stored, sent, floorAtFinish⟩
  simpa only [sameFloor] using aboveLaterFloor

/-- One paced recovery step either observes a newer local commit head or sends
the exact next own block. -/
def ValidatorExactNextOrCommitAdvance
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (start validator : Nat) : Prop :=
  (∃ finish,
    start ≤ finish ∧
      ((timed.execution.trace start).validatorState
          validator).commitHead.index <
        ((timed.execution.trace finish).validatorState
          validator).commitHead.index) ∨
    ValidatorExactNextProduction timed start validator

/-- One exact persisted and sent block at a positive offset from a start floor.
-/
def ValidatorExactOffsetProduction
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (start validator offset : Nat) : Prop :=
  ∃ persistTime finish block,
    start ≤ persistTime ∧
    persistTime + 1 ≤ finish ∧
    block.reference.round =
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound + offset ∧
    ValidatorLocalActionOccurs (timed.execution.events persistTime) validator
      (.persistProposal block) ∧
    ((timed.execution.trace finish).validatorState validator).ownBlockAt
        (((timed.execution.trace start).validatorState
          validator).highestSignedRound + offset) = some block.reference ∧
    ((timed.execution.trace finish).validatorState validator).sentOwnBlockAt
        (((timed.execution.trace start).validatorState
          validator).highestSignedRound + offset) = true ∧
    ((timed.execution.trace finish).validatorState
      validator).highestSignedRound =
        ((timed.execution.trace start).validatorState
          validator).highestSignedRound + offset

/-- A round above the signer floor cannot already have a sent own block. -/
theorem round_above_signer_floor_is_not_sent
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {time validator round : Nat}
    (validatorInRange : validator < config.authorityCount)
    (aboveFloor :
      ((execution.trace time).validatorState validator).highestSignedRound <
        round) :
    ((execution.trace time).validatorState validator).sentOwnBlockAt round =
      false := by
  cases sent :
      ((execution.trace time).validatorState validator).sentOwnBlockAt round with
  | false => rfl
  | true =>
      have durable :=
        (execution.statesWellFormed time validator validatorInRange)
          |>.sentOwnBlockIsDurable round sent
      cases own :
          ((execution.trace time).validatorState validator).ownBlockAt round with
      | none => simp [own] at durable
      | some reference =>
          have belowFloor :=
            (execution.statesWellFormed time validator validatorInRange)
              |>.ownBlockDoesNotExceedSignerFloor round reference own
          omega

/-- A later phase production is also a production result for every earlier
state. Durable state makes the signer floor increase and sent flags persist. -/
theorem strict_phase_production_starts_earlier
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {earlier later validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (ordered : earlier ≤ later)
    (production : ValidatorStrictPhaseProduction timed later validator) :
    ValidatorStrictPhaseProduction timed earlier validator := by
  rcases production with
    ⟨finish, round, reference, laterBeforeFinish, floorBelowRound,
      notSentLater, stored, sent⟩
  have durable := timed.execution.durableStateMonotone validator earlier later
    validatorInRange ordered
  have notSentEarlier :
      ((timed.execution.trace earlier).validatorState validator).sentOwnBlockAt
          round = false := by
    cases sentEarlier :
        ((timed.execution.trace earlier).validatorState validator).sentOwnBlockAt
          round with
    | false => rfl
    | true =>
        have sentLater := durable.sent_own_block_persists sentEarlier
        rw [notSentLater] at sentLater
        contradiction
  exact ⟨finish, round, reference,
    Nat.le_trans ordered laterBeforeFinish,
    Nat.le_trans durable.2.2.2.2.2.2.1 floorBelowRound,
    notSentEarlier, stored, sent⟩

/-- Protected parent synchronization supplies one protected exact-next action.
The only progress result in this theorem comes from addressed delivery and
bounded protected local actions. -/
theorem exact_next_parent_sync_eventually_supplies_proposal
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {restartSnapshot : Time}
    {start validator : Nat}
    (sourceRules : ValidatorParentNeedSourceRules
      (timed := timed) restartSnapshot)
    (proposalRules : ValidatorParentNeedProposalRules
      (timed := timed) restartSnapshot)
    (historyRules : ValidatorProducedCausalHistoryRules syncRules)
    (pending : ValidatorExactNextParentSyncAt (timed := timed) start validator)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ current, start ≤ current →
      (timed.execution.trace current).epochActive = true) :
    ∃ finish,
      ∃ _next : ValidatorPinnedExactNextProposalAt timed start finish
          validator,
        start ≤ finish := by
  rcases historyRules.sourceForPersistedBlock pending.persistTime pending.author
      pending.block (sourceRules.persistedNeedWasProduced start validator
        pending) with
    ⟨capsule, targetBlock, capsuleSource, sourceProtection, required⟩
  by_cases acceptedAtStart : ∀ item, item ∈ capsule.history →
      ((timed.execution.trace start).validatorState validator).accepted
        item.reference = true
  · exact ⟨start,
      proposalRules.persistedNeedAcceptedEnables start validator pending capsule
        targetBlock start (Nat.le_refl _) acceptedAtStart,
      Nat.le_refl _⟩
  · have incompleteBeforeStart : ∀ current,
        pending.persistTime + 1 ≤ current →
        current ≤ start →
        ¬∀ item, item ∈ capsule.history →
          ((timed.execution.trace current).validatorState validator).accepted
            item.reference = true := by
      intro current _ currentBeforeStart acceptedAtCurrent
      apply acceptedAtStart
      intro item member
      exact (timed.execution.durableStateMonotone validator current start
        validatorInRange currentBeforeStart).accepted_block_persists
          (acceptedAtCurrent item member)
    have retainedAtStart := retained_validator_block_history_persists syncRules
      (causal_recovery_capsule_to_retained_validator_history capsuleSource)
      pending.persistenceBeforeNeed (by
        intro item member current afterPersistence currentBeforeStart
        exact sourceProtection validator validatorInRange
          validatorCorrectAvailable current afterPersistence
          (incompleteBeforeStart current afterPersistence currentBeforeStart)
          item member)
    have parentFirst := capsule.parent_first_validator_history (by
      intro reference genesis
      exact historyRules.genesisAccepted start validator reference
        validatorInRange validatorCorrectAvailable genesis)
    rcases retained_parent_first_history_eventually_accepted syncRules
        retainedAtStart validatorInRange validatorCorrectAvailable afterGst active
        (by
          intro current startBeforeCurrent incomplete item member
          exact sourceProtection validator validatorInRange
            validatorCorrectAvailable current
            (Nat.le_trans pending.persistenceBeforeNeed startBeforeCurrent)
            incomplete item member)
        (by
          intro item member current startBeforeCurrent notAccepted
          exact required validator validatorInRange validatorCorrectAvailable
            item member current
            (Nat.le_trans pending.persistenceBeforeNeed startBeforeCurrent)
            notAccepted)
        parentFirst with
      ⟨finish, startBeforeFinish, accepted⟩
    exact ⟨finish,
      proposalRules.persistedNeedAcceptedEnables start validator pending capsule
        targetBlock finish startBeforeFinish accepted,
      startBeforeFinish⟩

/-- A durable restart tip supplies one protected exact-next action. The remote
source is derived from the tip owner's restart retention rule. -/
theorem restart_parent_sync_eventually_supplies_proposal
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {snapshot start validator : Nat}
    (sourceRules : ValidatorParentNeedSourceRules (timed := timed) snapshot)
    (proposalRules : ValidatorParentNeedProposalRules (timed := timed) snapshot)
    (historyRules : ValidatorProducedCausalHistoryRules syncRules)
    (restartRules : ValidatorDurableRestartCausalSourceRules syncRules snapshot)
    (pending : ValidatorExactNextRestartParentSyncAt
      (timed := timed) snapshot start validator)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ current, start ≤ current →
      (timed.execution.trace current).epochActive = true) :
    ∃ finish,
      ∃ _next : ValidatorPinnedExactNextProposalAt timed start finish
          validator,
        start ≤ finish := by
  rcases restartRules.currentPositiveTipHasSource pending.author
      (sourceRules.restartNeedOwnerInRange start validator pending)
      (sourceRules.restartNeedOwnerCorrectAvailable start validator pending)
      (sourceRules.restartNeedOwnerHasPositiveTip start validator pending) with
    ⟨block, capsule, ownTip, targetBlock, capsuleSource, sourceProtection,
      required⟩
  by_cases acceptedAtStart : ∀ item, item ∈ capsule.history →
      ((timed.execution.trace start).validatorState validator).accepted
        item.reference = true
  · exact ⟨start,
      proposalRules.restartNeedAcceptedEnables start validator pending block
        capsule ownTip targetBlock start (Nat.le_refl _) acceptedAtStart,
      Nat.le_refl _⟩
  · have incompleteBeforeStart : ∀ current,
        snapshot ≤ current →
        current ≤ start →
        ¬∀ item, item ∈ capsule.history →
          ((timed.execution.trace current).validatorState validator).accepted
            item.reference = true := by
      intro current _ currentBeforeStart acceptedAtCurrent
      apply acceptedAtStart
      intro item member
      exact (timed.execution.durableStateMonotone validator current start
        validatorInRange currentBeforeStart).accepted_block_persists
          (acceptedAtCurrent item member)
    have retainedAtStart := retained_validator_block_history_persists syncRules
      (causal_recovery_capsule_to_retained_validator_history capsuleSource)
      pending.snapshotBeforeNeed (by
        intro item member current afterSnapshot currentBeforeStart
        exact sourceProtection validator validatorInRange
          validatorCorrectAvailable current afterSnapshot
          (incompleteBeforeStart current afterSnapshot currentBeforeStart)
          item member)
    have parentFirst := capsule.parent_first_validator_history (by
      intro reference genesis
      exact historyRules.genesisAccepted start validator reference
        validatorInRange validatorCorrectAvailable genesis)
    rcases retained_parent_first_history_eventually_accepted syncRules
        retainedAtStart validatorInRange validatorCorrectAvailable afterGst active
        (by
          intro current startBeforeCurrent incomplete item member
          exact sourceProtection validator validatorInRange
            validatorCorrectAvailable current
            (Nat.le_trans pending.snapshotBeforeNeed startBeforeCurrent)
            incomplete item member)
        (by
          intro item member current startBeforeCurrent notAccepted
          exact required validator validatorInRange validatorCorrectAvailable
            item member current
            (Nat.le_trans pending.snapshotBeforeNeed startBeforeCurrent)
            notAccepted)
        parentFirst with
      ⟨finish, startBeforeFinish, accepted⟩
    exact ⟨finish,
      proposalRules.restartNeedAcceptedEnables start validator pending block
        capsule ownTip targetBlock finish startBeforeFinish accepted,
      startBeforeFinish⟩

/-- One protected exact-next proposer action persists and sends the exact next
block. The persistence occurrence remains available to causal-retention proofs.
-/
theorem protected_exact_next_eventually_produces_exact_block_using_receiver
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
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (receiver : Nat)
    (receiverInRange : receiver < config.authorityCount)
    (receiverIsOther : receiver ≠ validator)
    (next : ValidatorExactNextProposalAt timed start validator) :
    ValidatorExactNextProduction timed start validator := by
  rcases enabled_propose_next_completes_exact_latched_pipeline source
      effects validatorInRange validatorCorrectAvailable
      next.actionProtected with
    ⟨proposalActionAt, proposal, startBeforeAction, _actionWithin,
      _proposalOccurs, _latchedAt, _origin, _author, _parents,
      proposalRoundFromStart, result⟩
  have broadcasts := result.2.2.2.2.2.2
  let broadcast := Classical.choice
    (broadcasts receiver
      receiverInRange receiverIsOther)
  have ownAtSend :=
    (timed.execution.durableStateMonotone validator
      (broadcast.persistedAt + 1) broadcast.sendActionAt validatorInRange
      broadcast.persistenceBeforeSend)
      |>.own_block_persists broadcast.ownBlockStored
  have sendGoalAtAction :
      (obligations.trace broadcast.sendActionAt validator).sendGoal
        proposal.block.reference receiver = true := by
    have reflected := obligations.sendActionIsReflected broadcast.sendActionAt
      validator receiver proposal.block.reference broadcast.sendOccurs
    have transition := obligations.transitionsFollowRules broadcast.sendActionAt
      validator
    rw [reflected] at transition
    cases transition with
    | markBlockSent required _ _ _ => exact required
  have serialized := obligations.sendGoalSerializesProposalPersistence
    broadcast.sendActionAt validator receiver proposal.block.reference
      sendGoalAtAction ownAtSend
  have floorAtFinish := serialized.2 broadcast.sendOccurs
  have ownAtFinish :=
    (timed.execution.durableStateMonotone validator
      (broadcast.persistedAt + 1) (broadcast.sendActionAt + 1)
      validatorInRange
      (Nat.le_trans broadcast.persistenceBeforeSend (Nat.le_succ _)))
      |>.own_block_persists broadcast.ownBlockStored
  refine ⟨broadcast.persistedAt, broadcast.sendActionAt + 1, proposal.block,
    ?_, Nat.le_trans broadcast.persistenceBeforeSend (Nat.le_succ _),
    proposalRoundFromStart, broadcast.persistenceOccurs, ?_, ?_, ?_⟩
  · exact Nat.le_trans startBeforeAction
      (Nat.le_trans (Nat.le_succ _) broadcast.readyBeforePersistence)
  · simpa [proposalRoundFromStart] using ownAtFinish
  · simpa [proposalRoundFromStart] using broadcast.sentOwnBlockRecorded
  · simpa [proposalRoundFromStart] using floorAtFinish

/-- A protected recovery proposal uses the canonical different receiver when
the validator set contains at least two validators. -/
theorem protected_exact_next_eventually_produces_exact_block_from_authority_count
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
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (next : ValidatorExactNextProposalAt timed start validator) :
    ValidatorExactNextProduction timed start validator := by
  exact protected_exact_next_eventually_produces_exact_block_using_receiver
    source effects validatorInRange validatorCorrectAvailable
      (validatorOtherReceiver validator)
      (validator_other_receiver_in_range authorityCountAtLeastTwo)
      validator_other_receiver_is_different next

/-- Compatibility form for a proposal engine that supplies its receiver. -/
theorem protected_exact_next_eventually_produces_exact_block
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {restartSnapshot : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (continuity : ValidatorStrictProposalRules syncRules obligations
      restartSnapshot)
    {start validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (next : ValidatorExactNextProposalAt timed start validator) :
    ValidatorExactNextProduction timed start validator := by
  exact protected_exact_next_eventually_produces_exact_block_using_receiver
    source effects validatorInRange validatorCorrectAvailable
      (continuity.broadcastReceiver validator)
      (continuity.broadcastReceiverInRange validator validatorInRange)
      (continuity.broadcastReceiverIsOther validator validatorInRange) next

/-- A latched recovery proposal persists and sends the exact next block. -/
theorem recovery_ready_proposal_eventually_produces_exact_block
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {restartSnapshot : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (continuity : ValidatorStrictProposalRules syncRules obligations
      restartSnapshot)
    {start validator : Nat}
    {proposal : ValidatorReadyProposal BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ready : (obligations.trace start validator).readyProposal = some proposal)
    (recovery : proposal.origin = .commitProgressRecovery) :
    ValidatorExactNextProduction timed start validator := by
  let receiver := continuity.broadcastReceiver validator
  rcases latched_proposal_run_creates_send_goal obligations validatorInRange
      validatorCorrectAvailable
      (continuity.broadcastReceiverInRange validator validatorInRange)
      (continuity.broadcastReceiverIsOther validator validatorInRange) ready with
    ⟨persistCompletion, startBeforePersist, _persistWithin, sendGoal⟩
  let persistTime := persistCompletion.event.completedAt
  have storedAtPersist := persist_proposal_occurrence_stores_own_block
    timed.execution persistCompletion.occurs
  rcases latched_send_goal_runs_within_bound obligations validatorInRange
      validatorCorrectAvailable sendGoal with
    ⟨sendCompletion, persistBeforeSend, _sendWithin, sendOccurs⟩
  let sendTime := sendCompletion.event.completedAt
  have ownAtSend :=
    (timed.execution.durableStateMonotone validator (persistTime + 1) sendTime
      validatorInRange (by simpa [persistTime, sendTime] using persistBeforeSend))
      |>.own_block_persists storedAtPersist
  have sendGoalAtAction :
      (obligations.trace sendTime validator).sendGoal proposal.block.reference
        receiver = true := by
    have reflected := obligations.sendActionIsReflected sendTime validator
      receiver proposal.block.reference (by simpa [sendTime] using sendOccurs)
    have transition := obligations.transitionsFollowRules sendTime validator
    rw [reflected] at transition
    cases transition with
    | markBlockSent required _ _ _ => exact required
  have serialized := obligations.sendGoalSerializesProposalPersistence sendTime
    validator receiver proposal.block.reference sendGoalAtAction ownAtSend
  have floorAtFinish := serialized.2 (by simpa [sendTime] using sendOccurs)
  have ownAtFinish :=
    (timed.execution.durableStateMonotone validator (persistTime + 1)
      (sendTime + 1) validatorInRange (by
        exact Nat.le_trans (by simpa [persistTime, sendTime] using
          persistBeforeSend) (Nat.le_succ _)))
      |>.own_block_persists storedAtPersist
  have sentAtFinish := send_block_occurrence_records_sent_own_block
    timed.execution (by simpa [sendTime] using sendOccurs)
  have legal := obligations.readyProposalIsLegal start validator proposal ready
  have proposalRound := legal_recovery_proposal_is_exact_next legal recovery
  refine ⟨persistTime, sendTime + 1, proposal.block, ?_, ?_, proposalRound,
    persistCompletion.occurs, ?_, ?_, ?_⟩
  · simpa [persistTime] using startBeforePersist
  · exact Nat.le_trans (by simpa [persistTime, sendTime] using
      persistBeforeSend) (Nat.le_succ _)
  · simpa [proposalRound] using ownAtFinish
  · simpa [proposalRound] using sentAtFinish
  · simpa [proposalRound] using floorAtFinish

/-- If one atomic step raises one host's durable signer floor, that step is an
actual proposal-persistence action by that host. -/
private theorem atomic_signer_floor_advance_is_persist_proposal
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {event : ValidatorAtomicEvent BlockId CommitId PacketId}
    {validator : Nat}
    (step : ValidatorAtomicStep config faults protocolPacket program time before
      event after)
    (advanced :
      (before.validatorState validator).highestSignedRound <
        (after.validatorState validator).highestSignedRound) :
    ∃ block,
      event = .localAction validator (.persistProposal block) ∧
        (before.validatorState validator).highestSignedRound <
          block.reference.round := by
  cases step
  case localAction =>
      rename_i actingValidator action _ _ _ _ structural otherUnchanged _ _
      by_cases sameValidator : validator = actingValidator
      · subst actingValidator
        have ownEffect := structural.2.2.1
        cases action with
        | persistProposal block =>
            simp only [OwnBlockActionEffect] at ownEffect
            refine ⟨block, rfl, ?_⟩
            rw [← ownEffect.2.2]
            exact advanced
        | sendReplayManifest _ _ =>
            simp only [OwnBlockActionEffect] at ownEffect
            rw [ownEffect.2] at advanced
            omega
        | enterRecovery | requestBlock | serveBlock | acceptBlock | sendBlock |
            proposeNormal | proposeNext | alignProposal | runCommitter |
            runReplayCommitter | recordCommit | applySyncedCommit =>
            simp only [OwnBlockActionEffect] at ownEffect
            rw [ownEffect.2] at advanced
            omega
      · rw [otherUnchanged validator sameValidator] at advanced
        omega
  case deliverPacket =>
      rename_i _ packet _ _ _ _ _ _ _ structural otherUnchanged _ _ _
      by_cases sameValidator : validator = packet.receiver
      · subst validator
        rw [structural.2.2.2.2.2.2.1] at advanced
        omega
      · rw [otherUnchanged validator sameValidator] at advanced
        omega
  case clockTick =>
      rename_i _ _ updated
      subst after
      simp [ValidatorWorldState.updateClocks] at advanced

/-- A one-time signer-floor increase has one concrete proposal-persistence
occurrence in that execution batch. The returned block is above the floor at
the start of the batch. -/
theorem signer_floor_advance_has_persist_proposal_occurrence
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
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (advanced :
      (before.validatorState validator).highestSignedRound <
        (after.validatorState validator).highestSignedRound) :
    ∃ block,
      ValidatorLocalActionOccurs events validator (.persistProposal block) ∧
        (before.validatorState validator).highestSignedRound <
          block.reference.round := by
  induction step with
  | nil => omega
  | @cons before middle after event events firstStep remainingSteps
      inductionHypothesis =>
      have firstMonotone :=
        validator_atomic_step_durable_monotone firstStep validator
      have startBeforeMiddle :
          (before.validatorState validator).highestSignedRound ≤
            (middle.validatorState validator).highestSignedRound := by
        rcases firstMonotone with
          ⟨_, _, _, _, _, _, signerFloorMonotone, _⟩
        exact signerFloorMonotone
      by_cases firstAdvanced :
          (before.validatorState validator).highestSignedRound <
            (middle.validatorState validator).highestSignedRound
      · rcases atomic_signer_floor_advance_is_persist_proposal firstStep
            firstAdvanced with ⟨block, eventExact, blockAboveStart⟩
        subst event
        refine ⟨block, ?_, blockAboveStart⟩
        exact ⟨[], events, by simp⟩
      · have sameFloor :
            (middle.validatorState validator).highestSignedRound =
              (before.validatorState validator).highestSignedRound := by
          omega
        have laterAdvanced :
            (middle.validatorState validator).highestSignedRound <
              (after.validatorState validator).highestSignedRound := by
          omega
        rcases inductionHypothesis laterAdvanced with
          ⟨block, ⟨eventPrefix, eventSuffix, eventsExact⟩,
            blockAboveMiddle⟩
        refine ⟨block, ?_, ?_⟩
        · refine ⟨event :: eventPrefix, eventSuffix, ?_⟩
          simp [eventsExact]
        · simpa only [sameFloor] using blockAboveMiddle

/-- If a later state reaches a target above the starting signer floor, some
intervening batch contains the concrete proposal persistence that raised the
floor. -/
theorem signer_floor_target_reached_has_persist_proposal_occurrence
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start target validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (targetAboveStart :
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound < target) :
    ∀ offset,
      target ≤
        ((timed.execution.trace (start + offset)).validatorState
          validator).highestSignedRound →
      ∃ persistTime block,
        start ≤ persistTime ∧
          persistTime < start + offset ∧
          ValidatorLocalActionOccurs (timed.execution.events persistTime)
            validator (.persistProposal block) ∧
          ((timed.execution.trace start).validatorState
              validator).highestSignedRound < block.reference.round := by
  intro offset
  induction offset with
  | zero =>
      intro reached
      simp only [Nat.add_zero] at reached
      omega
  | succ offset inductionHypothesis =>
      intro reached
      by_cases reachedBefore : target ≤
          ((timed.execution.trace (start + offset)).validatorState
            validator).highestSignedRound
      · rcases inductionHypothesis reachedBefore with
          ⟨persistTime, block, startBeforePersist, persistBefore,
            occurs, blockAboveStart⟩
        exact ⟨persistTime, block, startBeforePersist, by omega, occurs,
          blockAboveStart⟩
      · have currentBelowTarget :
            ((timed.execution.trace (start + offset)).validatorState
                validator).highestSignedRound < target := by
          omega
        have currentAdvances :
            ((timed.execution.trace (start + offset)).validatorState
                validator).highestSignedRound <
              ((timed.execution.trace ((start + offset) + 1)).validatorState
                validator).highestSignedRound := by
          simpa [Nat.add_assoc] using
            Nat.lt_of_lt_of_le currentBelowTarget reached
        rcases signer_floor_advance_has_persist_proposal_occurrence
            (timed.execution.stepsFollowRules (start + offset)) currentAdvances with
          ⟨block, occurs, blockAboveCurrent⟩
        have startFloorLeCurrent :=
          (timed.execution.durableStateMonotone validator start
            (start + offset) validatorInRange
              (Nat.le_add_right start offset)).2.2.2.2.2.2.1
        exact ⟨start + offset, block, by omega, by omega, occurs,
          Nat.lt_of_le_of_lt startFloorLeCurrent blockAboveCurrent⟩

/-- A concrete persistence occurrence creates a protected send goal. The same
host then records the exact own block as sent. -/
theorem persist_proposal_occurrence_eventually_sends_block
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
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {persistTime validator : Time} {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (occurs : ValidatorLocalActionOccurs (timed.execution.events persistTime)
      validator (.persistProposal block)) :
    ∃ finish,
      persistTime + 1 ≤ finish ∧
        ((timed.execution.trace finish).validatorState validator).ownBlockAt
            block.reference.round = some block.reference ∧
          ((timed.execution.trace finish).validatorState
            validator).sentOwnBlockAt block.reference.round = true ∧
          ((timed.execution.trace finish).validatorState
            validator).highestSignedRound = block.reference.round := by
  let receiver := validatorOtherReceiver validator
  have receiverInRange : receiver < config.authorityCount :=
    validator_other_receiver_in_range authorityCountAtLeastTwo
  have receiverIsOther : receiver ≠ validator :=
    validator_other_receiver_is_different
  have reflected := obligations.persistActionIsReflected persistTime validator
    block occurs
  have obligationStep := obligations.transitionsFollowRules persistTime
    validator
  rw [reflected] at obligationStep
  have sendGoal :
      (obligations.trace (persistTime + 1) validator).sendGoal
        block.reference receiver = true :=
    persist_proposal_creates_send_goals obligationStep receiverInRange
      receiverIsOther
  rcases latched_send_goal_run_records_sent obligations validatorInRange
      validatorCorrectAvailable sendGoal with
    ⟨completion, afterPersist, _withinBound, sentAtFinish, _cleared⟩
  let finish := completion.event.completedAt + 1
  have storedAfterPersist := persist_proposal_occurrence_stores_own_block
    timed.execution occurs
  have storedAtSend :=
    (timed.execution.durableStateMonotone validator (persistTime + 1)
      completion.event.completedAt validatorInRange afterPersist)
      |>.own_block_persists storedAfterPersist
  have sendGoalAtAction :
      (obligations.trace completion.event.completedAt validator).sendGoal
        block.reference receiver = true := by
    have reflected := obligations.sendActionIsReflected
      completion.event.completedAt validator receiver block.reference
        completion.occurs
    have transition := obligations.transitionsFollowRules
      completion.event.completedAt validator
    rw [reflected] at transition
    cases transition with
    | markBlockSent required _ _ _ => exact required
  have serialized := obligations.sendGoalSerializesProposalPersistence
    completion.event.completedAt validator receiver block.reference
      sendGoalAtAction storedAtSend
  have floorAtFinish := serialized.2 completion.occurs
  have storedAtFinish :=
    (timed.execution.durableStateMonotone validator (persistTime + 1) finish
      validatorInRange (by
        exact Nat.le_trans afterPersist (Nat.le_succ _)))
      |>.own_block_persists storedAfterPersist
  exact ⟨finish, Nat.le_trans afterPersist (Nat.le_succ _), storedAtFinish,
    sentAtFinish, floorAtFinish⟩

/-- A concrete proposal persistence keeps its exact block and time. It comes
from one earlier local proposal latch and creates one addressed packet for
every other validator. -/
theorem persist_proposal_occurrence_eventually_produces_exact_broadcast
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
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start persistTime validator : Time} {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (startBeforePersistence : start ≤ persistTime)
    (blockAboveStartFloor :
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound < block.reference.round)
    (occurs : ValidatorLocalActionOccurs (timed.execution.events persistTime)
      validator (.persistProposal block)) :
    Nonempty { production :
        ValidatorPersistedProposalBroadcastProduction timed obligations start
          validator //
      production.persistedAt = persistTime ∧
        production.proposal.block = block } := by
  have reflected := obligations.persistActionIsReflected persistTime validator
    block occurs
  have transition := obligations.transitionsFollowRules persistTime validator
  rw [reflected] at transition
  have readyExists : ∃ proposal,
      (obligations.trace persistTime validator).readyProposal = some proposal ∧
        proposal.block = block := by
    cases transition with
    | persistProposal proposalReady proposalBlock =>
        exact ⟨_, proposalReady, proposalBlock⟩
  rcases readyExists with ⟨proposal, proposalReady, proposalBlock⟩
  have proposalOccurs : ValidatorLocalActionOccurs
      (timed.execution.events persistTime) validator
        (.persistProposal proposal.block) := by
    simpa only [proposalBlock] using occurs
  have proposalAboveStartFloor :
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound < proposal.block.reference.round := by
    simpa only [proposalBlock] using blockAboveStartFloor
  have proposalLegal := obligations.readyProposalIsLegal persistTime
    validator proposal proposalReady
  rcases ready_proposal_has_main_action_origin obligations latchSource
      proposalReady with
    ⟨latchTime, latchBeforeReady, proposalLatchedAt,
      _proposalReadyAtPersistence, proposalLatchOrigin⟩
  have broadcasts := ready_proposal_broadcasts_to_every_other_validator
    effects validatorInRange validatorCorrectAvailable proposalReady
  rcases persist_proposal_occurrence_eventually_sends_block obligations
      authorityCountAtLeastTwo validatorInRange validatorCorrectAvailable
      proposalOccurs with
    ⟨finish, persistenceBeforeFinish, ownBlockStoredAtFinish,
      sentOwnBlockAtFinish, signerFloorAtFinish⟩
  exact ⟨⟨
    { readyAt := persistTime
      latchTime
      proposal
      persistedAt := persistTime
      finish
      startBeforePersistence
      latchBeforeReady
      proposalLatchedAt
      proposalLatchOrigin
      proposalAuthor := proposalLegal.1
      proposalRoundAboveStartFloor := proposalAboveStartFloor
      persistenceBeforeFinish
      persistenceOccurs := proposalOccurs
      ownBlockStoredAtFinish
      sentOwnBlockAtFinish
      signerFloorAtFinish
      broadcasts }, rfl, proposalBlock⟩⟩

/-- Forget the exact persistence time and block. -/
theorem persist_proposal_occurrence_eventually_produces_broadcast
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
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start persistTime validator : Time} {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (startBeforePersistence : start ≤ persistTime)
    (blockAboveStartFloor :
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound < block.reference.round)
    (occurs : ValidatorLocalActionOccurs (timed.execution.events persistTime)
      validator (.persistProposal block)) :
    Nonempty (ValidatorPersistedProposalBroadcastProduction timed obligations
      start validator) := by
  rcases persist_proposal_occurrence_eventually_produces_exact_broadcast
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable startBeforePersistence blockAboveStartFloor
          occurs with
    ⟨⟨production, _exact⟩⟩
  exact ⟨production⟩

/-- One protected normal proposal-builder action latches and persists its exact
parent list. It creates an addressed block send to every other validator. -/
theorem protected_normal_proposal_eventually_produces_broadcast_with_exact_parents
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
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (next : ValidatorNormalProposalAt timed start validator) :
    Nonempty { production : ValidatorNormalProposalBroadcastProduction timed
        obligations start validator next.targetRound //
      production.parents = next.parents } := by
  have enabledAtStart := timed.protectedActionIsEnabled start validator
    (.proposeNormal next.targetRound next.parents) next.actionProtected
  have targetAboveStart :
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound < next.targetRound :=
    enabledAtStart.2.1.1
  let proposalCompletion := timed.completeProtectedAction validator
    (.proposeNormal next.targetRound next.parents) start validatorInRange
      validatorCorrectAvailable next.actionProtected
  let proposalActionAt := proposalCompletion.event.completedAt
  have proposalOccurs : ValidatorLocalActionOccurs
      (timed.execution.events proposalActionAt) validator
        (.proposeNormal next.targetRound next.parents) :=
    proposalCompletion.occurs
  rcases effects.normalProposalEnablesPersistence proposalActionAt validator
      next.targetRound next.parents proposalOccurs with
    ⟨block, blockAuthor, blockRound, blockParents, persistenceEnabled⟩
  rcases source.normalProposalResultIsLatched proposalActionAt validator
      next.targetRound next.parents block proposalOccurs blockAuthor blockRound
      blockParents persistenceEnabled with
    ⟨proposal, latched, latchedAt, proposalOrigin, proposalBlock⟩
  have ready := latch_event_sets_ready_proposal
    obligations.transitionsFollowRules latched
  have broadcasts := ready_proposal_broadcasts_to_every_other_validator effects
    validatorInRange validatorCorrectAvailable ready
  let receiver := validatorOtherReceiver validator
  have receiverInRange : receiver < config.authorityCount :=
    validator_other_receiver_in_range authorityCountAtLeastTwo
  have receiverIsOther : receiver ≠ validator :=
    validator_other_receiver_is_different
  rcases broadcasts receiver receiverInRange receiverIsOther with ⟨broadcast⟩
  have ownAtSend :=
    (timed.execution.durableStateMonotone validator
      (broadcast.persistedAt + 1) broadcast.sendActionAt validatorInRange
        broadcast.persistenceBeforeSend)
      |>.own_block_persists broadcast.ownBlockStored
  have sendGoalAtAction :
      (obligations.trace broadcast.sendActionAt validator).sendGoal
        proposal.block.reference receiver = true := by
    have reflected := obligations.sendActionIsReflected broadcast.sendActionAt
      validator receiver proposal.block.reference broadcast.sendOccurs
    have transition := obligations.transitionsFollowRules broadcast.sendActionAt
      validator
    rw [reflected] at transition
    cases transition with
    | markBlockSent required _ _ _ => exact required
  have serialized := obligations.sendGoalSerializesProposalPersistence
    broadcast.sendActionAt validator receiver proposal.block.reference
      sendGoalAtAction
      ownAtSend
  have floorAtFinish := serialized.2 broadcast.sendOccurs
  have ownAtFinish :=
    (timed.execution.durableStateMonotone validator
      (broadcast.persistedAt + 1) (broadcast.sendActionAt + 1)
      validatorInRange (Nat.le_trans broadcast.persistenceBeforeSend
        (Nat.le_succ _)))
      |>.own_block_persists broadcast.ownBlockStored
  have proposalAuthor : proposal.block.reference.author = validator := by
    rw [proposalBlock]
    exact blockAuthor
  have proposalRound : proposal.block.reference.round =
      next.targetRound := by
    rw [proposalBlock, blockRound]
  have proposalParents : proposal.block.parents = next.parents := by
    rw [proposalBlock]
    exact blockParents
  have startBeforeAction : start ≤ proposalActionAt := by
    simpa [proposalActionAt, proposalCompletion.sameEnableTime] using
      proposalCompletion.enableBeforeCompletion
  have actionWithinBound :
      proposalActionAt ≤ start + timed.localActionBound := by
    simpa [proposalActionAt, proposalCompletion.sameEnableTime] using
      proposalCompletion.completesWithinBound
  have actionBeforePersist : proposalActionAt + 1 ≤ broadcast.persistedAt := by
    simpa only [latchedAt] using broadcast.readyBeforePersistence
  exact ⟨⟨
    { parents := next.parents
      proposalActionAt
      proposal
      persistedAt := broadcast.persistedAt
      finish := broadcast.sendActionAt + 1
      startBeforeProposalAction := startBeforeAction
      proposalActionWithinBound := actionWithinBound
      proposalActionOccurs := proposalOccurs
      proposalLatched := latched
      proposalLatchedAt := latchedAt
      proposalOrigin
      proposalAuthor
      proposalRound
      proposalParents
      targetAboveStartFloor := targetAboveStart
      proposalBeforePersistence := actionBeforePersist
      persistenceBeforeFinish := Nat.le_trans
        broadcast.persistenceBeforeSend (Nat.le_succ _)
      persistenceOccurs := broadcast.persistenceOccurs
      ownBlockStoredAtFinish := ownAtFinish
      sentOwnBlockAtFinish := broadcast.sentOwnBlockRecorded
      signerFloorAtFinish := floorAtFinish
      broadcasts }, rfl⟩⟩

/-- Forget the equality to the protected action's input parent list. -/
theorem protected_normal_proposal_eventually_produces_broadcast
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
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (next : ValidatorNormalProposalAt timed start validator) :
    Nonempty (ValidatorNormalProposalBroadcastProduction timed obligations start
      validator next.targetRound) := by
  rcases protected_normal_proposal_eventually_produces_broadcast_with_exact_parents
      source effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable next with
    ⟨⟨production, _parentsExact⟩⟩
  exact ⟨production⟩

/-- One current accepted and retained quorum parent layer produces one exact
normal block after Core starts an enabled proposal attempt.

The parent layer is a current-state fact. The program guard is the local
`try_propose` trigger, which can come from the block-processing path or a
leader-timeout callback. The protection rule keeps that synchronous work until
it runs. Bounded local execution then proves persistence and one addressed
broadcast to every other validator. No future layer, commit, or sync result is
an input. -/
theorem ready_normal_parent_quorum_eventually_produces_exact_broadcast
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
    (protection : ValidatorReadyNormalProposalProtectionRule timed)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator targetRound : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (epochActive : (timed.execution.trace time).epochActive = true)
    (targetAboveSignerFloor :
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound < targetRound)
    (parentsReady : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace time).validatorState validator) targetRound
        parents)
    (programEnabled : program.actions.enabled validator
      (.proposeNormal targetRound parents)
      ((timed.execution.trace time).validatorState validator)) :
    Nonempty { production : ValidatorNormalProposalBroadcastProduction timed
        obligations time validator targetRound //
      production.parents = parents } := by
  have actionProtected := protection.readyProgramActionIsProtected time
    validator targetRound parents epochActive targetAboveSignerFloor
      parentsReady programEnabled
  exact
    protected_normal_proposal_eventually_produces_broadcast_with_exact_parents
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable
        ({ targetRound := targetRound
           parents := parents
           actionProtected := actionProtected } :
          ValidatorNormalProposalAt timed time validator)

/-- Forget the exact normal action and addressed broadcasts. -/
theorem protected_normal_proposal_eventually_produces_advancing_block
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
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (next : ValidatorNormalProposalAt timed start validator) :
    ValidatorNormalProposalProduction timed start validator
      next.targetRound :=
  by
    rcases protected_normal_proposal_eventually_produces_broadcast source effects
        authorityCountAtLeastTwo validatorInRange validatorCorrectAvailable
          next with
      ⟨production⟩
    exact production.toProduction

/-- A ready normal parent accumulator keeps the exact proposal and addressed
broadcast result. -/
theorem normal_parent_build_ready_eventually_produces_broadcast
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
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ready : ValidatorNormalParentBuildReadyAt timed start validator) :
    ∃ targetRound,
      Nonempty (ValidatorNormalProposalBroadcastProduction timed obligations
        start validator targetRound) := by
  rcases ready with
    ⟨targetRound, parents, _aboveFloor, _parentsReady, protectedBuild⟩
  refine ⟨targetRound, ?_⟩
  exact protected_normal_proposal_eventually_produces_broadcast source
    effects authorityCountAtLeastTwo validatorInRange
      validatorCorrectAvailable
      ({ targetRound := targetRound
         parents := parents
         actionProtected := protectedBuild } :
        ValidatorNormalProposalAt timed start validator)

/-- Forget the exact action and addressed sends from a ready normal build. -/
theorem normal_parent_build_ready_eventually_produces_advancing_block
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
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ready : ValidatorNormalParentBuildReadyAt timed start validator) :
    ∃ targetRound,
      ValidatorNormalProposalProduction timed start validator targetRound := by
  rcases normal_parent_build_ready_eventually_produces_broadcast source effects
      authorityCountAtLeastTwo validatorInRange validatorCorrectAvailable ready with
    ⟨targetRound, production⟩
  exact ⟨targetRound, production.elim fun result => result.toProduction⟩

/-- Replace each raw ready parent branch with the host's current representative
for the same author.

A Byzantine author can have a different current branch. The author set and
therefore its stake do not change. Fresh-need completeness puts each selected
representative in the need, and the need pins every selected accepted block
above GC. -/
theorem fresh_normal_need_maps_ready_frontier_to_representatives
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {time validator : Time} {need}
    {rawParents : List (ValidatorBlockRef BlockId)}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (needs.trace time validator).active = some need)
    (fresh : ValidatorFreshNormalAccumulatorNeedSourceAt timed time validator
      need)
    (rawReady : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace time).validatorState validator)
      need.targetRound rawParents)
    (targetParentsAboveGc :
      ((timed.execution.trace time).validatorState validator).gcRound + 1 <
        need.targetRound) :
    ∃ parents,
      (∀ parent, parent ∈ parents →
        parent.author < config.authorityCount) ∧
      (∀ parent, parent ∈ parents →
        ((timed.execution.trace time).validatorState
          validator).acceptedRepresentative (need.targetRound - 1)
            parent.author = some parent) ∧
      (∀ parent, parent ∈ parents → parent ∈ need.candidateRefs) ∧
      ValidatorProposalParentListReady .normal config
        ((timed.execution.trace time).validatorState validator)
        need.targetRound parents := by
  let state := (timed.execution.trace time).validatorState validator
  let view : ImmediateParentView BlockId :=
    { authorityCount := config.authorityCount
      proposalRound := need.targetRound
      accepted := state.accepted
      valid := state.accepted
      timely := fun _ => true }
  let selection : ImmediateParentSelection view :=
    { choice := fun author =>
        if author < config.authorityCount then
          state.acceptedRepresentative (need.targetRound - 1) author
        else none
      selectedMatchesAuthor := by
        intro author parent selected
        by_cases authorInRange : author < config.authorityCount
        · have representative :
              state.acceptedRepresentative (need.targetRound - 1) author =
                some parent := by
            simpa [state, authorInRange] using selected
          exact (representatives.representativeIsSound time validator
            (need.targetRound - 1) author parent validatorInRange
              validatorCorrectAvailable representative).1
        · simp [authorInRange] at selected
      selectedIsAcceptedValid := by
        intro author parent selected
        have authorInRange : author < config.authorityCount := by
          by_cases inRange : author < config.authorityCount
          · exact inRange
          · simp [inRange] at selected
        have representative :
            state.acceptedRepresentative (need.targetRound - 1) author =
              some parent := by
          simpa [state, authorInRange] using selected
        have sound := representatives.representativeIsSound time validator
          (need.targetRound - 1) author parent validatorInRange
            validatorCorrectAvailable representative
        change parent.author < config.authorityCount ∧
          parent.round + 1 = need.targetRound ∧
          state.accepted parent = true ∧ state.accepted parent = true
        refine ⟨?_, ?_, ?_, ?_⟩
        · simpa [sound.1] using authorInRange
        · omega
        · exact sound.2.2
        · exact sound.2.2 }
  let parents := selection.selectedParentRefs
  rcases fresh with
    ⟨⟨_discovery, _noChild, _baseline, _signerFloor, _candidatesSound,
      candidatesComplete⟩, normalOrigin, _canonicalTarget⟩
  have memberChoice : ∀ parent, parent ∈ parents →
      ∃ author,
        author < config.authorityCount ∧
          selection.choice author = some parent := by
    intro parent parentMember
    exact selected_parent_refs_from_member_has_choice
      (by simpa [parents, ImmediateParentSelection.selectedParentRefs,
        view] using parentMember)
  have parentsAreRepresentatives : ∀ parent, parent ∈ parents →
      state.acceptedRepresentative (need.targetRound - 1) parent.author =
        some parent := by
    intro parent parentMember
    rcases memberChoice parent parentMember with
      ⟨author, authorInRange, selected⟩
    have authorExact := selection.selectedMatchesAuthor author parent selected
    have representative :
        state.acceptedRepresentative (need.targetRound - 1) author =
          some parent := by
      simpa [selection, state, authorInRange] using selected
    simpa [authorExact] using representative
  have parentsInRange : ∀ parent, parent ∈ parents →
      parent.author < config.authorityCount := by
    intro parent parentMember
    rcases memberChoice parent parentMember with
      ⟨author, authorInRange, selected⟩
    have authorExact := selection.selectedMatchesAuthor author parent selected
    simpa [authorExact] using authorInRange
  have parentsWithin : ∀ parent, parent ∈ parents →
      parent ∈ need.candidateRefs := by
    intro parent parentMember
    rcases memberChoice parent parentMember with
      ⟨author, authorInRange, selected⟩
    have authorExact := selection.selectedMatchesAuthor author parent selected
    exact candidatesComplete parent.author parent
      (by simpa [authorExact] using authorInRange)
      (by simpa [state] using
        parentsAreRepresentatives parent parentMember)
  have rawAuthorSubset : VoterSet.SubsetAt config.authorityCount
      (validatorParentAuthors rawParents) (validatorParentAuthors parents) := by
    intro author authorInRange rawAuthor
    simp [validatorParentAuthors] at rawAuthor
    rcases rawAuthor with ⟨rawParent, rawMember, rawAuthorExact⟩
    have rawRound := (rawReady.1.2.1 rawParent rawMember).1
    have rawRoundExact : rawParent.round = need.targetRound - 1 := by
      omega
    have rawAccepted := (rawReady.1.2.1 rawParent rawMember).2
    rcases representatives.acceptedReferenceHasRepresentative time validator
        rawParent validatorInRange validatorCorrectAvailable
        (by simpa [rawAuthorExact] using authorInRange) rawAccepted with
      ⟨selectedParent, representative, selectedAuthor, _selectedRound,
        _selectedAccepted⟩
    have selected : selection.choice author = some selectedParent := by
      simpa [selection, state, authorInRange, rawAuthorExact, rawRoundExact] using
        representative
    have selectedMember : selectedParent ∈ parents := by
      exact selection.selected_parent_mem authorInRange selected
    simp [validatorParentAuthors]
    exact ⟨selectedParent, selectedMember,
      selectedAuthor.trans rawAuthorExact⟩
  have selectedQuorum : config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (validatorParentAuthors parents) :=
    Nat.le_trans rawReady.1.2.2
      (weight_mono config.stake rawAuthorSubset)
  have parentsReady : ValidatorProposalParentListReady .normal config state
      need.targetRound parents := by
    refine ⟨⟨selection.selected_parent_authors_nodup, ?_, selectedQuorum⟩,
      ?_⟩
    · intro parent parentMember
      rcases memberChoice parent parentMember with
        ⟨author, _authorInRange, selected⟩
      have acceptedValid := selection.selectedIsAcceptedValid author parent
        selected
      exact ⟨acceptedValid.2.1, by
        simpa [view, state] using acceptedValid.2.2.1⟩
    · intro parent parentMember
      rcases memberChoice parent parentMember with
        ⟨author, _authorInRange, selected⟩
      have acceptedValid := selection.selectedIsAcceptedValid author parent
        selected
      have parentAboveGc : state.gcRound < parent.round := by
        have immediate := acceptedValid.2.1
        simp only [view] at immediate
        change state.gcRound + 1 < need.targetRound at targetParentsAboveGc
        omega
      have accepted : state.accepted parent = true := by
        simpa [view, state] using acceptedValid.2.2.1
      exact ⟨needs.acceptedCandidateIsPinned time validator need parent active
        (parentsWithin parent parentMember) (Or.inr parentAboveGc)
          (by simpa [state] using accepted), Or.inr parentAboveGc⟩
  exact ⟨parents, parentsInRange,
    by simpa [state] using parentsAreRepresentatives,
    parentsWithin, by simpa [state, normalOrigin] using parentsReady⟩

/-- Current installed-head storage gives one exact ready representative list
for an active fresh normal need.

The result keeps the need target. It does not replace it with an existential
normal target. -/
theorem installed_head_bootstrap_fresh_need_gives_ready_parent_list
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    (source : ValidatorInstalledHeadBootstrapSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {time validator : Time} {need}
    {head : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (needs.trace time validator).active = some need)
    (fresh : ValidatorFreshNormalAccumulatorNeedSourceAt timed time validator
      need)
    (epochActive : (timed.execution.trace time).epochActive = true)
    (currentHead :
      ((timed.execution.trace time).validatorState validator).commitHead = head)
    (gcHasMoved :
      0 < ((timed.execution.trace time).validatorState validator).gcRound)
    (signerFloorBehindBootstrap :
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1 ≤
      ((timed.execution.trace time).validatorState validator).gcRound + 2) :
    ∃ parents,
      (∀ parent, parent ∈ parents →
        parent.author < config.authorityCount) ∧
      (∀ parent, parent ∈ parents →
        ((timed.execution.trace time).validatorState
          validator).acceptedRepresentative (need.targetRound - 1)
            parent.author = some parent) ∧
      (∀ parent, parent ∈ parents → parent ∈ need.candidateRefs) ∧
      ValidatorProposalParentListReady .normal config
        ((timed.execution.trace time).validatorState validator)
        need.targetRound parents := by
  let state := (timed.execution.trace time).validatorState validator
  have needTarget :
      need.targetRound = Nat.max (state.highestSignedRound + 1)
        (state.gcRound + 2) := by
    calc
      need.targetRound = Nat.max (need.signerFloor + 1)
          (((timed.execution.trace time).validatorState
            validator).gcRound + 2) := fresh.2.2
      _ = Nat.max (state.highestSignedRound + 1) (state.gcRound + 2) := by
        rw [fresh.1.2.2.2.1]
  rcases installed_head_bootstrap_gives_fresh_normal_parent_list source
      validatorInRange validatorCorrectAvailable epochActive currentHead
        gcHasMoved signerFloorBehindBootstrap with
    ⟨rawParents, rawReady⟩
  have rawReadyAtNeed : ValidatorProposalParentListReady .normal config state
      need.targetRound rawParents := by
    rw [needTarget]
    simpa [state] using rawReady
  have canonicalAtGc :
      Nat.max (state.highestSignedRound + 1) (state.gcRound + 2) =
        state.gcRound + 2 := by
    apply Nat.max_eq_right
    simpa [state] using signerFloorBehindBootstrap
  have targetParentsAboveGc : state.gcRound + 1 < need.targetRound := by
    rw [needTarget, canonicalAtGc]
    omega
  exact fresh_normal_need_maps_ready_frontier_to_representatives needs
    representatives validatorInRange validatorCorrectAvailable active fresh
      rawReadyAtNeed targetParentsAboveGc

/-- Current installed-head storage closes one active fresh normal need without a
caller-supplied ready parent list.

The stored DAG first supplies a raw quorum frontier. The local representative
adapter replaces each Byzantine branch, puts those representatives in the
fresh need, and pins them. The requester then protects the concrete normal
proposal-builder action. -/
theorem installed_head_bootstrap_fresh_need_gives_protected_build
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    (source : ValidatorInstalledHeadBootstrapSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {time validator : Time} {need}
    {head : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (needs.trace time validator).active = some need)
    (fresh : ValidatorFreshNormalAccumulatorNeedSourceAt timed time validator
      need)
    (epochActive : (timed.execution.trace time).epochActive = true)
    (currentHead :
      ((timed.execution.trace time).validatorState validator).commitHead = head)
    (gcHasMoved :
      0 < ((timed.execution.trace time).validatorState validator).gcRound)
    (signerFloorBehindBootstrap :
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1 ≤
      ((timed.execution.trace time).validatorState validator).gcRound + 2) :
    ValidatorNormalParentBuildReadyAt timed time validator := by
  rcases installed_head_bootstrap_fresh_need_gives_ready_parent_list source
      needs representatives validatorInRange validatorCorrectAvailable active
        fresh epochActive currentHead gcHasMoved signerFloorBehindBootstrap with
    ⟨parents, _parentsInRange, _parentsAreRepresentatives, parentsWithin,
      parentsReady⟩
  apply needs.ready_normal_need_gives_protected_build validatorInRange
    validatorCorrectAvailable active fresh.2.1
  exact ⟨parents, parentsWithin, by simpa only [fresh.2.1] using parentsReady⟩

/-- An idle recovery host at or below a moved GC boundary starts fresh normal
work and closes it from its current installed-head storage.

This includes a restored signer floor of zero. The result uses the need created
by the next local transition. It does not assume a future ready parent layer. -/
theorem installed_head_bootstrap_recovery_root_gives_protected_build
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    (source : ValidatorInstalledHeadBootstrapSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {time validator : Time} {head : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (noCommitInstall : ∀ currentHead,
      ¬ValidatorCommitInstallOccurs (timed.execution.events time) validator
        currentHead)
    (idle : (needs.trace time validator).active = none)
    (recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
      (time + 1) validator)
    (gcHasMoved : 0 < ((timed.execution.trace (time + 1)).validatorState
      validator).gcRound)
    (floorIsCommittedRoot :
      ((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound ≤
        ((timed.execution.trace (time + 1)).validatorState validator).gcRound)
    (epochActive : (timed.execution.trace (time + 1)).epochActive = true)
    (currentHead :
      ((timed.execution.trace (time + 1)).validatorState
        validator).commitHead = head) :
    ValidatorNormalParentBuildReadyAt timed (time + 1) validator := by
  rcases needs.recovery_root_while_idle_starts_fresh_normal_need
      validatorInRange validatorCorrectAvailable noCommitInstall idle
        recoveryMode gcHasMoved floorIsCommittedRoot epochActive with
    ⟨need, active, fresh⟩
  apply installed_head_bootstrap_fresh_need_gives_protected_build source needs
    representatives validatorInRange validatorCorrectAvailable active fresh
      epochActive currentHead gcHasMoved
  omega

/-- A fresh normal accumulator with one current quorum parent list produces the
exact normal proposal and its addressed broadcasts. -/
theorem fresh_normal_parent_list_eventually_produces_broadcast_with_exact_parents
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator : Time} {need}
    {parents : List (ValidatorBlockRef BlockId)}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (needs.trace time validator).active = some need)
    (fresh : ValidatorFreshNormalAccumulatorNeedSourceAt timed time validator
      need)
    (parentsReady : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace time).validatorState validator)
      need.targetRound parents)
    (parentsInRange : ∀ parent, parent ∈ parents →
      parent.author < config.authorityCount)
    (parentsAreRepresentatives : ∀ parent, parent ∈ parents →
      ((timed.execution.trace time).validatorState
        validator).acceptedRepresentative (need.targetRound - 1)
          parent.author = some parent) :
    Nonempty { production : ValidatorNormalProposalBroadcastProduction timed
        obligations time validator need.targetRound //
      production.parents = parents } := by
  rcases fresh with
    ⟨localSource, normalOrigin, _canonicalTarget⟩
  rcases localSource with
    ⟨_discovery, _noChild, _baseline, _signerFloor, _candidatesSound,
      candidatesComplete⟩
  have parentsWithin : ∀ parent, parent ∈ parents →
      parent ∈ need.candidateRefs := by
    intro parent parentMember
    exact candidatesComplete parent.author parent
      (parentsInRange parent parentMember)
      (parentsAreRepresentatives parent parentMember)
  have protectedBuild := needs.readyNormalNeedProtectsBuild time validator need
    parents validatorInRange validatorCorrectAvailable active normalOrigin
      parentsWithin parentsReady
  exact
    protected_normal_proposal_eventually_produces_broadcast_with_exact_parents
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable
        ({ targetRound := need.targetRound
           parents := parents
           actionProtected := protectedBuild } :
          ValidatorNormalProposalAt timed time validator)

/-- Forget the equality to the supplied complete parent list. -/
theorem fresh_normal_parent_list_eventually_produces_broadcast
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator : Time} {need}
    {parents : List (ValidatorBlockRef BlockId)}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (needs.trace time validator).active = some need)
    (fresh : ValidatorFreshNormalAccumulatorNeedSourceAt timed time validator
      need)
    (parentsReady : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace time).validatorState validator)
      need.targetRound parents)
    (parentsInRange : ∀ parent, parent ∈ parents →
      parent.author < config.authorityCount)
    (parentsAreRepresentatives : ∀ parent, parent ∈ parents →
      ((timed.execution.trace time).validatorState
        validator).acceptedRepresentative (need.targetRound - 1)
          parent.author = some parent) :
    Nonempty (ValidatorNormalProposalBroadcastProduction timed obligations time
      validator need.targetRound) := by
  rcases fresh_normal_parent_list_eventually_produces_broadcast_with_exact_parents
      needs latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable active fresh parentsReady parentsInRange
          parentsAreRepresentatives with
    ⟨⟨production, _parentsExact⟩⟩
  exact ⟨production⟩

/-- Current installed-head storage produces the exact target of one active
fresh normal need.

Unlike the protected-build projection, this result keeps the canonical target
in the broadcast type. No ready parent list is a caller input. -/
theorem installed_head_bootstrap_fresh_need_eventually_produces_exact_broadcast
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (source : ValidatorInstalledHeadBootstrapSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator : Time} {need}
    {head : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (needs.trace time validator).active = some need)
    (fresh : ValidatorFreshNormalAccumulatorNeedSourceAt timed time validator
      need)
    (epochActive : (timed.execution.trace time).epochActive = true)
    (currentHead :
      ((timed.execution.trace time).validatorState validator).commitHead = head)
    (gcHasMoved :
      0 < ((timed.execution.trace time).validatorState validator).gcRound)
    (signerFloorBehindBootstrap :
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1 ≤
      ((timed.execution.trace time).validatorState validator).gcRound + 2) :
    Nonempty (ValidatorNormalProposalBroadcastProduction timed obligations time
      validator need.targetRound) := by
  rcases installed_head_bootstrap_fresh_need_gives_ready_parent_list source
      needs representatives validatorInRange validatorCorrectAvailable active
        fresh epochActive currentHead gcHasMoved signerFloorBehindBootstrap with
    ⟨parents, parentsInRange, parentsAreRepresentatives, _parentsWithin,
      parentsReady⟩
  exact fresh_normal_parent_list_eventually_produces_broadcast needs
    latchSource effects authorityCountAtLeastTwo validatorInRange
      validatorCorrectAvailable active fresh parentsReady parentsInRange
        parentsAreRepresentatives

/-- An idle recovery root starts and broadcasts the exact canonical safe-resume
target from current installed-head storage. -/
theorem installed_head_bootstrap_idle_recovery_root_eventually_produces_canonical_broadcast
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (source : ValidatorInstalledHeadBootstrapSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator : Time} {head : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (noCommitInstall : ∀ currentHead,
      ¬ValidatorCommitInstallOccurs (timed.execution.events time) validator
        currentHead)
    (idle : (needs.trace time validator).active = none)
    (recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
      (time + 1) validator)
    (gcHasMoved : 0 < ((timed.execution.trace (time + 1)).validatorState
      validator).gcRound)
    (floorIsCommittedRoot :
      ((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound ≤
        ((timed.execution.trace (time + 1)).validatorState validator).gcRound)
    (epochActive : (timed.execution.trace (time + 1)).epochActive = true)
    (currentHead :
      ((timed.execution.trace (time + 1)).validatorState
        validator).commitHead = head) :
    Nonempty (ValidatorNormalProposalBroadcastProduction timed obligations
      (time + 1) validator
        (Nat.max
          (((timed.execution.trace (time + 1)).validatorState
            validator).highestSignedRound + 1)
          (((timed.execution.trace (time + 1)).validatorState
            validator).gcRound + 2))) := by
  rcases needs.recovery_root_while_idle_starts_fresh_normal_need
      validatorInRange validatorCorrectAvailable noCommitInstall idle
        recoveryMode gcHasMoved floorIsCommittedRoot epochActive with
    ⟨need, active, fresh⟩
  have signerFloorBehindBootstrap :
      ((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound + 1 ≤
        ((timed.execution.trace (time + 1)).validatorState validator).gcRound +
          2 := by
    omega
  have broadcast :=
    installed_head_bootstrap_fresh_need_eventually_produces_exact_broadcast
      source needs representatives latchSource effects authorityCountAtLeastTwo
        validatorInRange validatorCorrectAvailable active fresh epochActive
          currentHead gcHasMoved signerFloorBehindBootstrap
  have targetExact : need.targetRound =
      Nat.max
        (((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound + 1)
        (((timed.execution.trace (time + 1)).validatorState
          validator).gcRound + 2) := by
    calc
      need.targetRound = Nat.max (need.signerFloor + 1)
          (((timed.execution.trace (time + 1)).validatorState
            validator).gcRound + 2) := fresh.2.2
      _ = Nat.max
          (((timed.execution.trace (time + 1)).validatorState
            validator).highestSignedRound + 1)
          (((timed.execution.trace (time + 1)).validatorState
            validator).gcRound + 2) := by
        rw [fresh.1.2.2.2.1]
  rw [← targetExact]
  exact broadcast

/-- An actual local commit record starts fresh normal work while the requester
is idle. A current quorum parent list then produces a post-install broadcast. -/
theorem record_commit_while_idle_with_ready_parents_produces_broadcast_with_exact_parents
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator targetRound : Time}
    {head : ValidatorCommitHead CommitId}
    {parents : List (ValidatorBlockRef BlockId)}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (idle : (needs.trace time validator).active = none)
    (recorded : ValidatorLocalActionOccurs (timed.execution.events time)
      validator (.recordCommit head))
    (epochActiveAfter : (timed.execution.trace (time + 1)).epochActive = true)
    (canonicalTarget :
      Nat.max
        (((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound + 1)
        (((timed.execution.trace (time + 1)).validatorState
          validator).gcRound + 2) = targetRound)
    (parentsReady : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace (time + 1)).validatorState validator)
      targetRound parents)
    (parentsInRange : ∀ parent, parent ∈ parents →
      parent.author < config.authorityCount)
    (parentsAreRepresentatives : ∀ parent, parent ∈ parents →
      ((timed.execution.trace (time + 1)).validatorState
        validator).acceptedRepresentative (targetRound - 1)
          parent.author = some parent) :
    Nonempty { production : ValidatorNormalProposalBroadcastProduction timed
        obligations (time + 1) validator targetRound //
      production.parents = parents } := by
  rcases needs.commit_install_while_idle_starts_fresh_normal_need idle
      (Or.inl recorded) epochActiveAfter with
    ⟨need, active, fresh⟩
  have freshTarget := fresh.2.2
  rw [fresh.1.2.2.2.1] at freshTarget
  have needTarget : need.targetRound = targetRound :=
    freshTarget.trans canonicalTarget
  have readyForNeed : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace (time + 1)).validatorState validator)
      need.targetRound parents := by
    simpa only [needTarget] using parentsReady
  have representativesForNeed : ∀ parent, parent ∈ parents →
      ((timed.execution.trace (time + 1)).validatorState
        validator).acceptedRepresentative (need.targetRound - 1)
          parent.author = some parent := by
    simpa only [needTarget] using parentsAreRepresentatives
  have production :=
    fresh_normal_parent_list_eventually_produces_broadcast_with_exact_parents
      needs latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable active fresh readyForNeed parentsInRange
          representativesForNeed
  rw [← needTarget]
  exact production

/-- Forget the equality to the supplied current parent list. -/
theorem record_commit_while_idle_with_ready_parents_produces_broadcast
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator targetRound : Time}
    {head : ValidatorCommitHead CommitId}
    {parents : List (ValidatorBlockRef BlockId)}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (idle : (needs.trace time validator).active = none)
    (recorded : ValidatorLocalActionOccurs (timed.execution.events time)
      validator (.recordCommit head))
    (epochActiveAfter : (timed.execution.trace (time + 1)).epochActive = true)
    (canonicalTarget :
      Nat.max
        (((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound + 1)
        (((timed.execution.trace (time + 1)).validatorState
          validator).gcRound + 2) = targetRound)
    (parentsReady : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace (time + 1)).validatorState validator)
      targetRound parents)
    (parentsInRange : ∀ parent, parent ∈ parents →
      parent.author < config.authorityCount)
    (parentsAreRepresentatives : ∀ parent, parent ∈ parents →
      ((timed.execution.trace (time + 1)).validatorState
        validator).acceptedRepresentative (targetRound - 1)
          parent.author = some parent) :
    Nonempty (ValidatorNormalProposalBroadcastProduction timed obligations
      (time + 1) validator targetRound) := by
  rcases
      record_commit_while_idle_with_ready_parents_produces_broadcast_with_exact_parents
        needs latchSource effects authorityCountAtLeastTwo validatorInRange
          validatorCorrectAvailable idle recorded epochActiveAfter
            canonicalTarget parentsReady parentsInRange parentsAreRepresentatives with
    ⟨⟨production, _parentsExact⟩⟩
  exact ⟨production⟩

/-- A complete parent list that is ready before a local record remains ready
after that record when its round is above the new GC boundary. The resulting
post-install proposal keeps that exact list. -/
theorem record_commit_while_idle_with_pre_record_parents_produces_broadcast
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (installedSource : ValidatorInstalledCommitParentSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator targetRound : Time}
    {head : ValidatorCommitHead CommitId}
    {parents : List (ValidatorBlockRef BlockId)}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (idle : (needs.trace time validator).active = none)
    (recorded : ValidatorLocalActionOccurs (timed.execution.events time)
      validator (.recordCommit head))
    (epochActiveAfter : (timed.execution.trace (time + 1)).epochActive = true)
    (canonicalTarget :
      Nat.max
        (((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound + 1)
        (((timed.execution.trace (time + 1)).validatorState
          validator).gcRound + 2) = targetRound)
    (parentsReadyBefore : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace time).validatorState validator)
      targetRound parents)
    (parentsInRange : ∀ parent, parent ∈ parents →
      parent.author < config.authorityCount)
    (parentsAreRepresentativesBefore : ∀ parent, parent ∈ parents →
      ((timed.execution.trace time).validatorState
        validator).acceptedRepresentative (targetRound - 1)
          parent.author = some parent) :
    Nonempty { production : ValidatorNormalProposalBroadcastProduction timed
        obligations (time + 1) validator targetRound //
      production.parents = parents } := by
  have gcBound :
      ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 2 ≤
        Nat.max
          (((timed.execution.trace (time + 1)).validatorState
            validator).highestSignedRound + 1)
          (((timed.execution.trace (time + 1)).validatorState
            validator).gcRound + 2) :=
    Nat.le_max_right _ _
  have targetParentsAboveGc :
      ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 1 <
        targetRound := by
    rw [canonicalTarget] at gcBound
    omega
  rcases record_commit_preserves_above_gc_normal_parent_list installedSource
      validatorInRange validatorCorrectAvailable recorded parentsReadyBefore
        parentsInRange parentsAreRepresentativesBefore targetParentsAboveGc with
    ⟨parentsReadyAfter, parentsAreRepresentativesAfter⟩
  exact
    record_commit_while_idle_with_ready_parents_produces_broadcast_with_exact_parents
      needs latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable idle recorded epochActiveAfter canonicalTarget
          parentsReadyAfter parentsInRange parentsAreRepresentativesAfter

/-- A local commit replaces unresolved recovery work with fresh normal work.
The replacement uses the same exact pre-record parent list when that list stays
above the new GC boundary. -/
theorem record_commit_rebases_active_recovery_with_pre_record_parents_produces_broadcast
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (installedSource : ValidatorInstalledCommitParentSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator : Time}
    {head : ValidatorCommitHead CommitId}
    {need : ValidatorRecoveryParentNeed BlockId CommitId config}
    {parents : List (ValidatorBlockRef BlockId)}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (needs.trace time validator).active = some need)
    (recoveryOrigin : need.proposalOrigin = .commitProgressRecovery)
    (recorded : ValidatorLocalActionOccurs (timed.execution.events time)
      validator (.recordCommit head))
    (targetNotReached :
      ((timed.execution.trace (time + 1)).validatorState
        validator).highestSignedRound < need.targetRound)
    (postGcAllowsTarget :
      ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 2 ≤
        need.targetRound)
    (parentsReadyBefore : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace time).validatorState validator)
      need.targetRound parents)
    (parentsInRange : ∀ parent, parent ∈ parents →
      parent.author < config.authorityCount)
    (parentsAreRepresentativesBefore : ∀ parent, parent ∈ parents →
      ((timed.execution.trace time).validatorState
        validator).acceptedRepresentative (need.targetRound - 1)
          parent.author = some parent) :
    Nonempty { production : ValidatorNormalProposalBroadcastProduction timed
        obligations (time + 1) validator need.targetRound //
      production.parents = parents } := by
  have mainFacts := needs.activeNeedMatchesMain time validator need active
  have recoveryFacts := mainFacts.2.2.2 recoveryOrigin
  have commitAdvancedFromRecord :=
    record_commit_occurrence_advances_commit_index timed recorded
  have commitAdvanced : need.baselineCommit.index <
      ((timed.execution.trace (time + 1)).validatorState
        validator).commitHead.index := by
    rw [recoveryFacts.1]
    exact commitAdvancedFromRecord
  rcases needs.commit_advance_rebases_recovery_need active recoveryOrigin
      commitAdvanced targetNotReached with
    ⟨rebased, activeAfter, _isRebase, fresh⟩
  have durable := timed.execution.durableStateMonotone validator time (time + 1)
    validatorInRange (Nat.le_succ _)
  have postFloorMatchesNeed :
      ((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound = need.signerFloor := by
    have needFloorAtMostPost : need.signerFloor ≤
        ((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound := by
      rw [mainFacts.2.1]
      exact durable.2.2.2.2.2.2.1
    have recoveryTarget := need.recoveryTargetIsExactNext recoveryOrigin
    omega
  have rebasedTarget : rebased.targetRound = need.targetRound := by
    calc
      rebased.targetRound = Nat.max (rebased.signerFloor + 1)
          (((timed.execution.trace (time + 1)).validatorState
            validator).gcRound + 2) := fresh.2.2
      _ = Nat.max need.targetRound
          (((timed.execution.trace (time + 1)).validatorState
            validator).gcRound + 2) := by
        rw [fresh.1.2.2.2.1, postFloorMatchesNeed,
          ← need.recoveryTargetIsExactNext recoveryOrigin]
      _ = need.targetRound := Nat.max_eq_left postGcAllowsTarget
  have targetParentsAboveGc :
      ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 1 <
        need.targetRound := by
    omega
  rcases record_commit_preserves_above_gc_normal_parent_list installedSource
      validatorInRange validatorCorrectAvailable recorded parentsReadyBefore
        parentsInRange parentsAreRepresentativesBefore targetParentsAboveGc with
    ⟨parentsReadyAfter, parentsAreRepresentativesAfter⟩
  have parentsReadyForRebased : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace (time + 1)).validatorState validator)
      rebased.targetRound parents := by
    simpa only [rebasedTarget] using parentsReadyAfter
  have representativesForRebased : ∀ parent, parent ∈ parents →
      ((timed.execution.trace (time + 1)).validatorState
        validator).acceptedRepresentative (rebased.targetRound - 1)
          parent.author = some parent := by
    simpa only [rebasedTarget] using parentsAreRepresentativesAfter
  have production :=
    fresh_normal_parent_list_eventually_produces_broadcast_with_exact_parents
      needs latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable activeAfter fresh parentsReadyForRebased
          parentsInRange representativesForRebased
  rw [← rebasedTarget]
  exact production

/-- A local commit preserves unresolved normal work. If the exact parent list
was already in that accumulator, the host proposes it only after the record
batch and keeps the exact list in the broadcast result. -/
theorem record_commit_preserves_active_normal_with_pre_record_parents_produces_broadcast
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (installedSource : ValidatorInstalledCommitParentSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator : Time}
    {head : ValidatorCommitHead CommitId}
    {need : ValidatorRecoveryParentNeed BlockId CommitId config}
    {parents : List (ValidatorBlockRef BlockId)}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (needs.trace time validator).active = some need)
    (normalOrigin : need.proposalOrigin = .normal)
    (recorded : ValidatorLocalActionOccurs (timed.execution.events time)
      validator (.recordCommit head))
    (epochActiveAfter : (timed.execution.trace (time + 1)).epochActive = true)
    (targetNotReached :
      ((timed.execution.trace (time + 1)).validatorState
        validator).highestSignedRound < need.targetRound)
    (postGcAllowsTarget :
      ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 2 ≤
        need.targetRound)
    (parentsReadyBefore : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace time).validatorState validator)
      need.targetRound parents)
    (parentsWithinNeed : ∀ parent, parent ∈ parents →
      parent ∈ need.candidateRefs)
    (parentsInRange : ∀ parent, parent ∈ parents →
      parent.author < config.authorityCount)
    (parentsAreRepresentativesBefore : ∀ parent, parent ∈ parents →
      ((timed.execution.trace time).validatorState
        validator).acceptedRepresentative (need.targetRound - 1)
          parent.author = some parent) :
    Nonempty { production : ValidatorNormalProposalBroadcastProduction timed
        obligations (time + 1) validator need.targetRound //
      production.parents = parents } := by
  rcases needs.normalNeedSurvivesCommit time validator need head active
      normalOrigin (Or.inl recorded) epochActiveAfter targetNotReached with
    ⟨afterNeed, activeAfter, accumulated⟩
  have afterNormal : afterNeed.proposalOrigin = .normal :=
    accumulated.1.trans normalOrigin
  have sameTarget : afterNeed.targetRound = need.targetRound :=
    accumulated.2.2.2.1
  have parentsWithinAfter : ∀ parent, parent ∈ parents →
      parent ∈ afterNeed.candidateRefs := by
    intro parent parentMember
    exact accumulated.2.2.2.2 parent
      (parentsWithinNeed parent parentMember)
  have targetParentsAboveGc :
      ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 1 <
        need.targetRound := by
    omega
  rcases record_commit_preserves_above_gc_normal_parent_list installedSource
      validatorInRange validatorCorrectAvailable recorded parentsReadyBefore
        parentsInRange parentsAreRepresentativesBefore targetParentsAboveGc with
    ⟨parentsReadyAfter, _parentsAreRepresentativesAfter⟩
  have parentsReadyForAfter : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace (time + 1)).validatorState validator)
      afterNeed.targetRound parents := by
    simpa only [sameTarget] using parentsReadyAfter
  have protectedBuild := needs.readyNormalNeedProtectsBuild (time + 1)
    validator afterNeed parents validatorInRange validatorCorrectAvailable
      activeAfter afterNormal parentsWithinAfter parentsReadyForAfter
  have production :=
    protected_normal_proposal_eventually_produces_broadcast_with_exact_parents
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable
        ({ targetRound := afterNeed.targetRound
           parents := parents
           actionProtected := protectedBuild } :
          ValidatorNormalProposalAt timed (time + 1) validator)
  rw [← sameTarget]
  exact production

/-- The exact post-record decision DAG supplies the current parent list for an
idle host's canonical normal target. The proposal action and every addressed
broadcast occur after the record batch. -/
theorem record_commit_while_idle_decision_dag_produces_broadcast
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (installedSource : ValidatorInstalledCommitParentSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator targetRound : Time}
    {head : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (idle : (needs.trace time validator).active = none)
    (recorded : ValidatorLocalActionOccurs (timed.execution.events time)
      validator (.recordCommit head))
    (epochActiveAfter : (timed.execution.trace (time + 1)).epochActive = true)
    (canonicalTarget :
      Nat.max
        (((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound + 1)
        (((timed.execution.trace (time + 1)).validatorState
          validator).gcRound + 2) = targetRound)
    (targetAtMostFrontier : targetRound ≤ head.round) :
    Nonempty (ValidatorNormalProposalBroadcastProduction timed obligations
      (time + 1) validator targetRound) := by
  have gcBound :
      ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 2 ≤
        Nat.max
          (((timed.execution.trace (time + 1)).validatorState
            validator).highestSignedRound + 1)
          (((timed.execution.trace (time + 1)).validatorState
            validator).gcRound + 2) :=
    Nat.le_max_right _ _
  have targetParentsAboveGc :
      ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 1 <
        targetRound := by
    rw [canonicalTarget] at gcBound
    omega
  rcases installed_commit_parent_window_gives_normal_parent_list
      installedSource validatorInRange validatorCorrectAvailable recorded
        targetParentsAboveGc targetAtMostFrontier with
    ⟨parents, parentsReady, parentsInRange, parentsAreRepresentatives,
      _parentsIncludeRepresentatives⟩
  exact record_commit_while_idle_with_ready_parents_produces_broadcast needs
    latchSource effects authorityCountAtLeastTwo validatorInRange
      validatorCorrectAvailable idle recorded epochActiveAfter canonicalTarget
        parentsReady parentsInRange parentsAreRepresentatives

/-- An exact parent-need extension event stores that extended need. -/
theorem parent_need_extension_event_sets_active
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {before after : ValidatorRecoveryParentNeedState
      BlockId CommitId config}
    {event : ValidatorRecoveryParentNeedEvent BlockId CommitId config}
    {mainAfter : ValidatorLocalState BlockId CommitId}
    {epochActiveAfter : Bool}
    {extended : ValidatorRecoveryParentNeed BlockId CommitId config}
    (transition : ValidatorRecoveryParentNeedTransition before event mainAfter
      epochActiveAfter after)
    (extension : event.extendNeed = some extended) :
    after.active = some extended := by
  cases transition <;> simp_all

/-- An accumulator extension relation includes an unchanged need. -/
theorem recovery_parent_need_accumulator_extends_refl
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (need : ValidatorRecoveryParentNeed BlockId CommitId config) :
    ValidatorRecoveryParentNeedAccumulatorExtends need need := by
  exact ⟨rfl, rfl, rfl, rfl, fun _ member => member⟩

/-- Accumulator extensions compose without changing the fixed normal target. -/
theorem recovery_parent_need_accumulator_extends_trans
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {first middle last :
      ValidatorRecoveryParentNeed BlockId CommitId config}
    (firstToMiddle :
      ValidatorRecoveryParentNeedAccumulatorExtends first middle)
    (middleToLast :
      ValidatorRecoveryParentNeedAccumulatorExtends middle last) :
    ValidatorRecoveryParentNeedAccumulatorExtends first last := by
  rcases firstToMiddle with
    ⟨middleOrigin, middleBaseline, middleFloor, middleTarget, middleContains⟩
  rcases middleToLast with
    ⟨lastOrigin, lastBaseline, lastFloor, lastTarget, lastContains⟩
  exact ⟨lastOrigin.trans middleOrigin, lastBaseline.trans middleBaseline,
    lastFloor.trans middleFloor, lastTarget.trans middleTarget,
    fun reference member => lastContains reference (middleContains reference member)⟩

/-- An active normal need survives one active host batch unless that batch has
already raised the signer floor to the need's fixed target. Commit installs do
not replace this work. -/
theorem active_normal_parent_need_survives_one_step_or_target_reached
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator : Time} {need}
    (active : (needs.trace time validator).active = some need)
    (normalOrigin : need.proposalOrigin = .normal)
    (epochActiveAfter :
      (timed.execution.trace (time + 1)).epochActive = true) :
    (∃ nextNeed,
      (needs.trace (time + 1) validator).active = some nextNeed ∧
        ValidatorRecoveryParentNeedAccumulatorExtends need nextNeed) ∨
      need.targetRound ≤
        ((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound := by
  have noRebase : (needs.event time validator).rebaseNeed = none := by
    cases rebase : (needs.event time validator).rebaseNeed with
    | none => rfl
    | some rebased =>
        rcases needs.rebasedNeedHasCommitSource time validator rebased rebase with
          ⟨oldNeed, _head, oldActive, oldRecovery, _install, _rebase, _source⟩
        have oldIsNeed : oldNeed = need := by
          rw [active] at oldActive
          exact (Option.some.inj oldActive).symm
        subst oldNeed
        simp [normalOrigin] at oldRecovery
  have transition := needs.transitionsFollowRules time validator
  rw [epochActiveAfter] at transition
  cases transition <;>
    simp_all [ValidatorRecoveryParentNeedAccumulatorExtends]

/-- A fixed normal target remains active for any finite number of active host
batches, unless an actual proposal persistence has already raised the signer
floor to that target. This induction permits any number of intervening commit
installs. -/
theorem active_normal_parent_need_extends_or_target_reached_after
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {start validator : Time} {need}
    (validatorInRange : validator < config.authorityCount)
    (active : (needs.trace start validator).active = some need)
    (normalOrigin : need.proposalOrigin = .normal)
    (activeSuffix : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ∀ offset,
      (∃ laterNeed,
        (needs.trace (start + offset) validator).active = some laterNeed ∧
          ValidatorRecoveryParentNeedAccumulatorExtends need laterNeed) ∨
        need.targetRound ≤
          ((timed.execution.trace (start + offset)).validatorState
            validator).highestSignedRound := by
  intro offset
  induction offset with
  | zero =>
      left
      exact ⟨need, by simpa using active,
        recovery_parent_need_accumulator_extends_refl need⟩
  | succ offset inductionHypothesis =>
      rcases inductionHypothesis with preserved | reached
      · rcases preserved with ⟨currentNeed, currentActive, currentExtends⟩
        have currentNormal : currentNeed.proposalOrigin = .normal :=
          currentExtends.1.trans normalOrigin
        have nextEpoch :
            (timed.execution.trace ((start + offset) + 1)).epochActive = true := by
          apply activeSuffix
          exact Nat.le_trans (Nat.le_add_right start offset) (Nat.le_succ _)
        rcases active_normal_parent_need_survives_one_step_or_target_reached
            needs currentActive currentNormal nextEpoch with
          nextPreserved | currentTargetReached
        · left
          rcases nextPreserved with ⟨nextNeed, nextActive, nextExtends⟩
          refine ⟨nextNeed, ?_,
            recovery_parent_need_accumulator_extends_trans currentExtends
              nextExtends⟩
          simpa [Nat.add_assoc] using nextActive
        · right
          have sameTarget : currentNeed.targetRound = need.targetRound :=
            currentExtends.2.2.2.1
          simpa [Nat.add_assoc, sameTarget] using currentTargetReached
      · right
        have durable := timed.execution.durableStateMonotone validator
          (start + offset) ((start + offset) + 1) validatorInRange
            (Nat.le_succ _)
        have floorMonotone := durable.2.2.2.2.2.2.1
        have reachedNext := Nat.le_trans reached floorMonotone
        simpa [Nat.add_assoc] using reachedNext

/-- Reaching one active normal need's target is not an abstract progress event.
It contains an actual proposal persistence, and the durable send worker sends
that exact own block. -/
theorem normal_parent_need_target_reached_eventually_sends_advancing_block
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (obligations : ValidatorProposalObligationExecution timed)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start offset validator : Time} {need}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (needs.trace start validator).active = some need)
    (reached : need.targetRound ≤
      ((timed.execution.trace (start + offset)).validatorState
        validator).highestSignedRound) :
    ∃ finish, ∃ block : ValidatorBlock BlockId,
      start ≤ finish ∧
        ((timed.execution.trace start).validatorState
            validator).highestSignedRound < block.reference.round ∧
        ((timed.execution.trace finish).validatorState validator).ownBlockAt
            block.reference.round = some block.reference ∧
        ((timed.execution.trace finish).validatorState
          validator).sentOwnBlockAt block.reference.round = true := by
  have needFacts := needs.activeNeedMatchesMain start validator need active
  have targetAboveStart :
      ((timed.execution.trace start).validatorState
          validator).highestSignedRound < need.targetRound := by
    rw [← needFacts.2.1]
    exact need.targetAboveSignerFloor
  rcases signer_floor_target_reached_has_persist_proposal_occurrence
      validatorInRange targetAboveStart offset reached with
    ⟨persistTime, block, startBeforePersist, _persistBeforeReached,
      persists, blockAboveStart⟩
  rcases persist_proposal_occurrence_eventually_sends_block obligations
      authorityCountAtLeastTwo validatorInRange validatorCorrectAvailable
      persists with
    ⟨finish, persistBeforeFinish, stored, sent, _floorAtFinish⟩
  exact ⟨finish, block,
    Nat.le_trans startBeforePersist (Nat.le_trans (Nat.le_succ _) persistBeforeFinish),
    blockAboveStart, stored, sent⟩

/-- One compatible current quorum source is added to a preserved normal
accumulator in one local batch. Extra equivocating candidates do not block the
quorum parent list. -/
theorem compatible_ready_source_closes_normal_parent_need
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator : Time} {need sourceNeed}
    (validatorInRange : validator < config.authorityCount)
    (active : (needs.trace time validator).active = some need)
    (normalOrigin : need.proposalOrigin = .normal)
    (notReady : ¬ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) need)
    (targetNotReached :
      ((timed.execution.trace (time + 1)).validatorState
        validator).highestSignedRound < need.targetRound)
    (sourceLocal : ValidatorRecoveryParentNeedLocalSourceAt pins time validator
      sourceNeed)
    (sameFloor : sourceNeed.signerFloor = need.signerFloor)
    (sameTarget : sourceNeed.targetRound = need.targetRound)
    (sourceReady : ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) sourceNeed) :
    ∃ extended,
      (needs.trace (time + 1) validator).active = some extended ∧
        extended.proposalOrigin = .normal ∧
        extended.signerFloor = need.signerFloor ∧
        extended.targetRound = need.targetRound ∧
          ValidatorRecoveryParentNeedReadyAt
            ((timed.execution.trace (time + 1)).validatorState validator)
            extended := by
  rcases sourceReady with
    ⟨parents, parentsFromSource, parentReadyAtTime⟩
  have parentsNonempty : parents ≠ [] :=
    validator_parent_list_ready_nonempty parentReadyAtTime.1
  have eachDiscovered : ∀ parent, parent ∈ parents →
      needs.discoveredCandidate time validator parent = true := by
    intro parent parentMember
    exact needs.compatibleSourceCandidateIsDiscovered time validator need
      sourceNeed parent active sourceLocal sameFloor sameTarget
      (parentsFromSource parent parentMember)
  rcases List.exists_mem_of_ne_nil parents parentsNonempty with
    ⟨firstParent, firstMember⟩
  have oneDiscovered : ∃ reference,
      needs.discoveredCandidate time validator reference = true :=
    ⟨firstParent, eachDiscovered firstParent firstMember⟩
  have timerNotLatched :
      (needs.event time validator).timerArmLatched = false := by
    cases timerValue : (needs.event time validator).timerArmLatched with
    | false => rfl
    | true =>
        rcases (needs.timerArmLatchIff time validator).1 timerValue with
          ⟨timerNeed, _goal, timerActive, _selected, _latched,
            recoveryOrigin, _target⟩
        have sameNeed : timerNeed = need := by
          rw [active] at timerActive
          cases timerActive
          rfl
        subst timerNeed
        simp [normalOrigin] at recoveryOrigin
  have noRebase : (needs.event time validator).rebaseNeed = none := by
    cases rebase : (needs.event time validator).rebaseNeed with
    | none => rfl
    | some rebased =>
        rcases needs.rebasedNeedHasCommitSource time validator rebased rebase with
          ⟨oldNeed, _head, oldActive, oldRecovery, _install, _isRebase,
            _fresh⟩
        have oldIsNeed : oldNeed = need := by
          rw [active] at oldActive
          exact (Option.some.inj oldActive).symm
        subst oldNeed
        simp [normalOrigin] at oldRecovery
  rcases needs.discoveredCandidatesExtendActiveNeed time validator need active
      noRebase timerNotLatched targetNotReached oneDiscovered with
    ⟨extended, extendEvent, exactExtension⟩
  have transition := needs.transitionsFollowRules time validator
  have activeNext :
      (needs.trace (time + 1) validator).active = some extended := by
    exact parent_need_extension_event_sets_active transition extendEvent
  unfold ValidatorRecoveryParentNeedIsExactExtension at exactExtension
  rcases exactExtension with
    ⟨sameOrigin, _sameDiscovery, _sameCapsule, _sameBaseline,
      _sameSignerFloor, targetPreserved, _sameSourceBlock,
      candidatesExact⟩
  have extendedNormal : extended.proposalOrigin = .normal := by
    exact sameOrigin.trans normalOrigin
  have parentsInExtended : ∀ parent, parent ∈ parents →
      parent ∈ extended.candidateRefs := by
    intro parent parentMember
    exact (candidatesExact parent).2
      (Or.inr (eachDiscovered parent parentMember))
  have durable := timed.execution.durableStateMonotone validator time
    (time + 1) validatorInRange (Nat.le_succ _)
  have parentReadyNext : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace (time + 1)).validatorState validator)
      extended.targetRound parents := by
    refine ⟨⟨parentReadyAtTime.1.1, ?_, parentReadyAtTime.1.2.2⟩, ?_⟩
    · intro parent parentMember
      have atTime := parentReadyAtTime.1.2.1 parent parentMember
      refine ⟨?_, timed.execution.accepted_block_persists
        validatorInRange (Nat.le_succ _) atTime.2⟩
      rw [targetPreserved, ← sameTarget]
      exact atTime.1
    · intro parent parentMember
      have acceptedNext := timed.execution.accepted_block_persists
        validatorInRange (Nat.le_succ _)
          (parentReadyAtTime.1.2.1 parent parentMember).2
      have targetFence := needs.activeNeedFencesTargetRound (time + 1)
        validator extended activeNext
      have parentTarget : parent.round + 1 = extended.targetRound := by
        calc
          parent.round + 1 = sourceNeed.targetRound :=
            (parentReadyAtTime.1.2.1 parent parentMember).1
          _ = need.targetRound := sameTarget
          _ = extended.targetRound := targetPreserved.symm
      rcases targetFence with targetOne | gcBelowTarget
      · have parentIsGenesis : parent.round = 0 := by omega
        have retainedNext := needs.acceptedCandidateIsPinned (time + 1)
          validator extended parent activeNext
            (parentsInExtended parent parentMember) (Or.inl parentIsGenesis)
              acceptedNext
        exact ⟨retainedNext, Or.inl parentIsGenesis⟩
      · have gcBelowParent :
            ((timed.execution.trace (time + 1)).validatorState
              validator).gcRound < parent.round := by omega
        have retainedNext := needs.acceptedCandidateIsPinned (time + 1)
          validator extended parent activeNext
            (parentsInExtended parent parentMember) (Or.inr gcBelowParent)
              acceptedNext
        exact ⟨retainedNext, Or.inr gcBelowParent⟩
  exact ⟨extended, activeNext, extendedNormal, _sameSignerFloor,
    targetPreserved,
    ⟨parents, parentsInExtended, by
      simpa only [extendedNormal] using parentReadyNext⟩⟩

/-- A preserved normal need with one compatible current quorum source produces
one own block, even if later commits continue to install. -/
theorem compatible_ready_source_eventually_produces_advancing_block
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator : Time} {need sourceNeed}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (needActive : (needs.trace time validator).active = some need)
    (normalOrigin : need.proposalOrigin = .normal)
    (notReady : ¬ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) need)
    (sourceLocal : ValidatorRecoveryParentNeedLocalSourceAt pins time validator
      sourceNeed)
    (sameFloor : sourceNeed.signerFloor = need.signerFloor)
    (sameTarget : sourceNeed.targetRound = need.targetRound)
    (sourceReady : ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) sourceNeed) :
    ∃ targetRound,
      ValidatorNormalProposalProduction timed time validator targetRound := by
  by_cases targetReached : need.targetRound ≤
      ((timed.execution.trace (time + 1)).validatorState
        validator).highestSignedRound
  · have needFacts := needs.activeNeedMatchesMain time validator need needActive
    have targetAboveStart :
        ((timed.execution.trace time).validatorState
            validator).highestSignedRound < need.targetRound := by
      rw [← needFacts.2.1]
      exact need.targetAboveSignerFloor
    rcases signer_floor_target_reached_has_persist_proposal_occurrence
        validatorInRange targetAboveStart 1 (by simpa using targetReached) with
      ⟨persistTime, block, startBeforePersist, _persistBeforeReached,
        persists, blockAboveStart⟩
    rcases persist_proposal_occurrence_eventually_sends_block obligations
        authorityCountAtLeastTwo validatorInRange validatorCorrectAvailable
        persists with
      ⟨finish, persistBeforeFinish, stored, sent, floorAtFinish⟩
    exact ⟨block.reference.round, persistTime, finish, block,
      startBeforePersist, persistBeforeFinish, rfl, blockAboveStart, persists,
      stored, sent, floorAtFinish⟩
  · have targetNotReached :
        ((timed.execution.trace (time + 1)).validatorState
            validator).highestSignedRound < need.targetRound := by
      omega
    rcases compatible_ready_source_closes_normal_parent_need needs
        validatorInRange needActive normalOrigin notReady targetNotReached
        sourceLocal sameFloor sameTarget sourceReady with
      ⟨extended, activeNext, extendedNormal, signerFloorPreserved,
        _targetPreserved, readyNext⟩
    have buildReady := needs.ready_normal_need_gives_protected_build
      validatorInRange validatorCorrectAvailable activeNext extendedNormal readyNext
    rcases normal_parent_build_ready_eventually_produces_advancing_block
        latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable buildReady with
      ⟨targetRound, persistTime, finish, block, nextBeforePersist,
        persistBeforeFinish, blockTarget, aboveNextFloor, persisted, stored, sent,
        floorAtFinish⟩
    have needFacts := needs.activeNeedMatchesMain time validator need needActive
    have extendedFacts := needs.activeNeedMatchesMain (time + 1) validator
      extended activeNext
    have sameMainFloor :
        ((timed.execution.trace time).validatorState
            validator).highestSignedRound =
          ((timed.execution.trace (time + 1)).validatorState
            validator).highestSignedRound := by
      calc
        ((timed.execution.trace time).validatorState
            validator).highestSignedRound = need.signerFloor :=
          needFacts.2.1.symm
        _ = extended.signerFloor := signerFloorPreserved.symm
        _ = ((timed.execution.trace (time + 1)).validatorState
            validator).highestSignedRound := extendedFacts.2.1
    refine ⟨targetRound, persistTime, finish, block,
      Nat.le_trans (Nat.le_succ _) nextBeforePersist, persistBeforeFinish,
      blockTarget, ?_, persisted, stored, sent, floorAtFinish⟩
    simpa only [sameMainFloor] using aboveNextFloor

/-- A compatible current parent source gives a concrete addressed broadcast.
If the target was already signed in the closing batch, the result keeps the
origin-neutral persisted proposal. Otherwise, it keeps the new normal proposal
and its exact parent list. -/
theorem compatible_ready_source_eventually_produces_broadcast
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator : Time} {need sourceNeed}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (needActive : (needs.trace time validator).active = some need)
    (normalOrigin : need.proposalOrigin = .normal)
    (notReady : ¬ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) need)
    (sourceLocal : ValidatorRecoveryParentNeedLocalSourceAt pins time validator
      sourceNeed)
    (sameFloor : sourceNeed.signerFloor = need.signerFloor)
    (sameTarget : sourceNeed.targetRound = need.targetRound)
    (sourceReady : ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) sourceNeed) :
    Nonempty (ValidatorPersistedProposalBroadcastProduction timed obligations
      time validator) ∨
      ∃ targetRound,
        Nonempty (ValidatorNormalProposalBroadcastProduction timed obligations
          (time + 1) validator targetRound) := by
  by_cases targetReached : need.targetRound ≤
      ((timed.execution.trace (time + 1)).validatorState
        validator).highestSignedRound
  · have needFacts := needs.activeNeedMatchesMain time validator need needActive
    have targetAboveStart :
        ((timed.execution.trace time).validatorState
            validator).highestSignedRound < need.targetRound := by
      rw [← needFacts.2.1]
      exact need.targetAboveSignerFloor
    rcases signer_floor_target_reached_has_persist_proposal_occurrence
        validatorInRange targetAboveStart 1 (by simpa using targetReached) with
      ⟨persistTime, block, startBeforePersist, _persistBeforeReached,
        persists, blockAboveStart⟩
    exact Or.inl
      (persist_proposal_occurrence_eventually_produces_broadcast latchSource
        effects authorityCountAtLeastTwo validatorInRange
          validatorCorrectAvailable startBeforePersist blockAboveStart persists)
  · have targetNotReached :
        ((timed.execution.trace (time + 1)).validatorState
            validator).highestSignedRound < need.targetRound := by
      omega
    rcases compatible_ready_source_closes_normal_parent_need needs
        validatorInRange needActive normalOrigin notReady targetNotReached
        sourceLocal sameFloor sameTarget sourceReady with
      ⟨extended, activeNext, extendedNormal, _signerFloorPreserved,
        _targetPreserved, readyNext⟩
    have buildReady := needs.ready_normal_need_gives_protected_build
      validatorInRange validatorCorrectAvailable activeNext extendedNormal
        readyNext
    rcases normal_parent_build_ready_eventually_produces_broadcast latchSource
        effects authorityCountAtLeastTwo validatorInRange
          validatorCorrectAvailable buildReady with
      ⟨targetRound, production⟩
    exact Or.inr ⟨targetRound, production⟩

/-- An actual commit install closes one preserved normal parent need from the
post-install local DAG, or the same batch has already signed its target.

The source is current host state. It does not state that a future quorum or a
remote holder exists. -/
theorem installed_commit_window_eventually_produces_advancing_block
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (installedSource : ValidatorInstalledCommitParentSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator : Time} {need}
    {head : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (needActive : (needs.trace time validator).active = some need)
    (normalOrigin : need.proposalOrigin = .normal)
    (installed : ValidatorLocalActionOccurs
      (timed.execution.events time) validator (.recordCommit head))
    (epochActiveAfter :
      (timed.execution.trace (time + 1)).epochActive = true)
    (targetParentsAboveGc :
      ((timed.execution.trace (time + 1)).validatorState validator).gcRound + 1 <
        need.targetRound)
    (targetAtMostFrontier : need.targetRound ≤ head.round) :
    ∃ targetRound,
      ValidatorNormalProposalProduction timed time validator targetRound := by
  rcases active_normal_parent_need_survives_one_step_or_target_reached needs
      needActive normalOrigin epochActiveAfter with preserved | reached
  · rcases preserved with ⟨afterNeed, activeAfter, accumulated⟩
    have afterNormal : afterNeed.proposalOrigin = .normal :=
      accumulated.1.trans normalOrigin
    have sameFloor : afterNeed.signerFloor = need.signerFloor :=
      accumulated.2.2.1
    have sameTarget : afterNeed.targetRound = need.targetRound :=
      accumulated.2.2.2.1
    have beforeFacts := needs.activeNeedMatchesMain time validator need needActive
    have afterFacts := needs.activeNeedMatchesMain (time + 1) validator
      afterNeed activeAfter
    have sameMainFloor :
        ((timed.execution.trace time).validatorState
            validator).highestSignedRound =
          ((timed.execution.trace (time + 1)).validatorState
            validator).highestSignedRound := by
      calc
        ((timed.execution.trace time).validatorState
            validator).highestSignedRound = need.signerFloor :=
          beforeFacts.2.1.symm
        _ = afterNeed.signerFloor := sameFloor.symm
        _ = ((timed.execution.trace (time + 1)).validatorState
            validator).highestSignedRound := afterFacts.2.1
    have afterTargetParentsAboveGc :
        ((timed.execution.trace (time + 1)).validatorState
            validator).gcRound + 1 < afterNeed.targetRound := by
      simpa only [sameTarget] using targetParentsAboveGc
    have afterTargetAtMostFrontier : afterNeed.targetRound ≤ head.round := by
      simpa only [sameTarget] using targetAtMostFrontier
    rcases installed_commit_parent_window_gives_normal_parent_list
        installedSource validatorInRange validatorCorrectAvailable installed
          afterTargetParentsAboveGc afterTargetAtMostFrontier with
      ⟨parents, parentsReady, _parentsInRange, parentsAreRepresentatives,
        parentsIncludeRepresentatives⟩
    rcases normal_parent_list_gives_compatible_local_source afterNormal
        afterFacts.2.1 parentsReady parentsAreRepresentatives
          parentsIncludeRepresentatives with
      ⟨sourceNeed, sourceLocal, sourceFloor, sourceTarget, sourceReady⟩
    by_cases afterReady : ValidatorRecoveryParentNeedReadyAt
        ((timed.execution.trace (time + 1)).validatorState validator) afterNeed
    · have buildReady := needs.ready_normal_need_gives_protected_build
        validatorInRange validatorCorrectAvailable activeAfter afterNormal
          afterReady
      rcases normal_parent_build_ready_eventually_produces_advancing_block
          latchSource effects authorityCountAtLeastTwo validatorInRange
            validatorCorrectAvailable buildReady with
        ⟨targetRound, production⟩
      exact ⟨targetRound,
        normal_proposal_production_starts_at_same_floor (Nat.le_succ _)
          sameMainFloor production⟩
    · rcases compatible_ready_source_eventually_produces_advancing_block
          needs latchSource effects authorityCountAtLeastTwo validatorInRange
            validatorCorrectAvailable activeAfter afterNormal afterReady
            (Or.inr (Or.inr sourceLocal)) sourceFloor sourceTarget sourceReady with
        ⟨targetRound, production⟩
      exact ⟨targetRound,
        normal_proposal_production_starts_at_same_floor (Nat.le_succ _)
          sameMainFloor production⟩
  · have needFacts := needs.activeNeedMatchesMain time validator need needActive
    have targetAboveStart :
        ((timed.execution.trace time).validatorState
            validator).highestSignedRound < need.targetRound := by
      rw [← needFacts.2.1]
      exact need.targetAboveSignerFloor
    rcases signer_floor_target_reached_has_persist_proposal_occurrence
        validatorInRange targetAboveStart 1 (by simpa using reached) with
      ⟨persistTime, block, startBeforePersist, _persistBeforeReached,
        persists, blockAboveStart⟩
    rcases persist_proposal_occurrence_eventually_sends_block obligations
        authorityCountAtLeastTwo validatorInRange validatorCorrectAvailable
          persists with
      ⟨finish, persistBeforeFinish, stored, sent, floorAtFinish⟩
    exact ⟨block.reference.round, persistTime, finish, block,
      startBeforePersist, persistBeforeFinish, rfl, blockAboveStart, persists,
      stored, sent, floorAtFinish⟩

/-- One exact-next normal proposal is the exact-next special case of normal
proposal production. -/
theorem protected_normal_exact_next_eventually_produces_exact_block
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
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (next : ValidatorNormalExactNextProposalAt timed start validator) :
    ValidatorExactNextProduction timed start validator := by
  have production := protected_normal_proposal_eventually_produces_advancing_block
    source effects authorityCountAtLeastTwo validatorInRange
      validatorCorrectAvailable
      ({ targetRound := next.targetRound
         parents := next.parents
         actionProtected := next.actionProtected } :
        ValidatorNormalProposalAt timed start validator)
  rcases production with
    ⟨persistTime, finish, block, startBeforePersist, persistBeforeFinish,
      blockTarget, _aboveStart, persisted, stored, sent, floorAtFinish⟩
  have blockRound : block.reference.round =
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound + 1 := by
    calc
      block.reference.round = next.targetRound := by simpa using blockTarget
      _ = ((timed.execution.trace start).validatorState
          validator).highestSignedRound + 1 := next.targetIsExactNext
  refine ⟨persistTime, finish, block, startBeforePersist, persistBeforeFinish,
    blockRound, persisted, ?_, ?_, ?_⟩
  · simpa [blockRound] using stored
  · simpa [blockRound] using sent
  · simpa [blockRound] using floorAtFinish

/-- Exact-next production is one strict phase production. -/
theorem exact_next_production_is_strict
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (production : ValidatorExactNextProduction timed start validator) :
    ValidatorStrictPhaseProduction timed start validator := by
  rcases production with
    ⟨persistTime, finish, block, startBeforePersist, persistBeforeFinish,
      blockRound, persisted, stored, sent, floorAtFinish⟩
  let target :=
    ((timed.execution.trace start).validatorState
      validator).highestSignedRound + 1
  have startBeforeFinish : start ≤ finish := Nat.le_trans startBeforePersist
    (Nat.le_trans (Nat.le_succ _) persistBeforeFinish)
  refine ⟨finish, target, block.reference, startBeforeFinish, ?_, ?_, stored,
    sent⟩
  · simp [target]
  · apply round_above_signer_floor_is_not_sent validatorInRange
    simp [target]

/-- An exact-next production also starts at an earlier state with the same
signer floor. -/
theorem exact_next_production_starts_at_same_floor
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {earlier later validator : Nat}
    (ordered : earlier ≤ later)
    (sameFloor :
      ((timed.execution.trace later).validatorState
        validator).highestSignedRound =
          ((timed.execution.trace earlier).validatorState
            validator).highestSignedRound)
    (production : ValidatorExactNextProduction timed later validator) :
    ValidatorExactNextProduction timed earlier validator := by
  rcases production with
    ⟨persistTime, finish, block, laterBeforePersist, persistBeforeFinish,
      blockRound, persisted, stored, sent, floorAtFinish⟩
  refine ⟨persistTime, finish, block, Nat.le_trans ordered laterBeforePersist,
    persistBeforeFinish, ?_, persisted, ?_, ?_, ?_⟩
  · simpa [sameFloor] using blockRound
  · simpa [sameFloor] using stored
  · simpa [sameFloor] using sent
  · simpa [sameFloor] using floorAtFinish

/-- One expired recovery timer supplies the only recovery proposal action used
by the exact-next path. -/
theorem timer_paced_ready_eventually_produces_exact_block_using_receiver
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
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {start validator : Nat}
    (receiver : Nat)
    (receiverInRange : receiver < config.authorityCount)
    (receiverIsOther : receiver ≠ validator)
    (ready : ValidatorOriginAwareRecoveryProposalReady faults timed waits
      ((timed.execution.trace start).validatorState validator).commitHead
      (((timed.execution.trace start).validatorState
        validator).highestSignedRound + 1) validator)
    (startBeforeTimer : start ≤ ready.ready.startedAt) :
    ValidatorExactNextProduction timed start validator := by
  let deadline := ready.ready.startedAt +
    waits.wait
      ((timed.execution.trace start).validatorState validator).commitHead
      (((timed.execution.trace start).validatorState
        validator).highestSignedRound + 1)
  have startBeforeDeadline : start ≤ deadline := by
    exact Nat.le_trans startBeforeTimer (Nat.le_add_right _ _)
  have enabled := timed.protectedActionIsEnabled deadline validator
    (.proposeNext ready.ready.parents) ready.ready.proposalProtected
  have targetAtDeadline :
      ((timed.execution.trace start).validatorState
          validator).highestSignedRound + 1 =
        ((timed.execution.trace deadline).validatorState
          validator).highestSignedRound + 1 := by
    rcases enabled.2.1 with
      ⟨recovery, recoveryAtDeadline, _noAlignment, recoveryTarget,
        _expired, _parents⟩
    have sameRecovery : recovery = ready.ready.recovery := by
      have stored := ready.ready.recoveryAtDeadline
      change ((timed.execution.trace deadline).validatorState
          validator).recovery = some ready.ready.recovery at stored
      rw [stored] at recoveryAtDeadline
      exact Option.some.inj recoveryAtDeadline.symm
    subst recovery
    rw [ready.ready.recoveryTarget] at recoveryTarget
    exact recoveryTarget
  have sameFloor :
      ((timed.execution.trace deadline).validatorState
          validator).highestSignedRound =
        ((timed.execution.trace start).validatorState
          validator).highestSignedRound := by
    omega
  have productionAtDeadline :=
    protected_exact_next_eventually_produces_exact_block_using_receiver source
      effects ready.ready.validatorInRange
      ready.ready.validatorCorrectAvailable
      receiver receiverInRange receiverIsOther
      ({ parents := ready.ready.parents
         actionProtected := ready.ready.proposalProtected } :
        ValidatorExactNextProposalAt timed deadline validator)
  exact exact_next_production_starts_at_same_floor startBeforeDeadline sameFloor
    productionAtDeadline

/-- Compatibility form for a proposal engine that supplies its receiver. -/
theorem timer_paced_ready_eventually_produces_exact_block
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {restartSnapshot : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (continuity : ValidatorStrictProposalRules syncRules obligations
      restartSnapshot)
    {start validator : Nat}
    (ready : ValidatorOriginAwareRecoveryProposalReady faults timed waits
      ((timed.execution.trace start).validatorState validator).commitHead
      (((timed.execution.trace start).validatorState
        validator).highestSignedRound + 1) validator)
    (startBeforeTimer : start ≤ ready.ready.startedAt) :
    ValidatorExactNextProduction timed start validator := by
  exact timer_paced_ready_eventually_produces_exact_block_using_receiver source
    effects (continuity.broadcastReceiver validator)
      (continuity.broadcastReceiverInRange validator
        ready.ready.validatorInRange)
      (continuity.broadcastReceiverIsOther validator
        ready.ready.validatorInRange)
      ready startBeforeTimer

/-- Current retained recovery parents start the durable timer. The timer gives
the exact next own block unless a newer local commit head wins first. -/
theorem timer_input_eventually_produces_exact_or_commit_advance
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {restartSnapshot : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (continuity : ValidatorStrictProposalRules syncRules obligations
      restartSnapshot)
    {start validator : Nat}
    (input : ValidatorRecoveryTimerArmInputAt timed start validator)
    (armEmpty : (arms.trace start validator).pending = none)
    (active : ∀ current, start ≤ current →
      (timed.execution.trace current).epochActive = true) :
    ValidatorExactNextOrCommitAdvance timed start validator := by
  by_cases stable : ∀ current, start ≤ current →
      ((timed.execution.trace current).validatorState validator).commitHead =
        ((timed.execution.trace start).validatorState validator).commitHead
  · rcases timerSource.ready_state_and_stable_head_derives_exact_next_ready
        arms start validator input armEmpty active stable with
      ⟨ready, startBeforeTimer, _timerBound⟩
    exact Or.inr
      (timer_paced_ready_eventually_produces_exact_block source effects
        continuity ready startBeforeTimer)
  · have changedExists : ∃ finish,
        start ≤ finish ∧
          ((timed.execution.trace finish).validatorState
              validator).commitHead ≠
            ((timed.execution.trace start).validatorState
              validator).commitHead := Classical.byContradiction (by
      intro noChange
      apply stable
      intro current startBeforeCurrent
      exact Classical.byContradiction (by
        intro changed
        apply noChange
        exact ⟨current, startBeforeCurrent, changed⟩))
    rcases changedExists with ⟨finish, startBeforeFinish, changed⟩
    have durable := timed.execution.durableStateMonotone validator start finish
      input.2.1 startBeforeFinish
    have differentIndex :
        ((timed.execution.trace start).validatorState
            validator).commitHead.index ≠
          ((timed.execution.trace finish).validatorState
            validator).commitHead.index := by
      intro sameIndex
      exact changed (durable.2.2.1 sameIndex).symm
    exact Or.inl ⟨finish, startBeforeFinish,
      Nat.lt_of_le_of_ne durable.1 differentIndex⟩

/-- Current recovery parents start or join the durable timer. The timer sends
the exact next own block, unless a newer local commit wins first. This form
uses only the validator count to choose a different receiver. -/
theorem timer_input_eventually_produces_exact_or_commit_advance_from_authority_count
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
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Nat}
    (input : ValidatorRecoveryTimerArmInputAt timed start validator)
    (active : ∀ current, start ≤ current →
      (timed.execution.trace current).epochActive = true) :
    ValidatorExactNextOrCommitAdvance timed start validator := by
  by_cases stable : ∀ current, start ≤ current →
      ((timed.execution.trace current).validatorState validator).commitHead =
        ((timed.execution.trace start).validatorState validator).commitHead
  · rcases timerSource.recovery_state_and_stable_head_derives_exact_next_ready
        arms start validator input active stable with
      ⟨ready, startBeforeTimer, _timerBound⟩
    exact Or.inr
      (timer_paced_ready_eventually_produces_exact_block_using_receiver source
        effects (validatorOtherReceiver validator)
        (validator_other_receiver_in_range authorityCountAtLeastTwo)
        validator_other_receiver_is_different ready startBeforeTimer)
  · have changedExists : ∃ finish,
        start ≤ finish ∧
          ((timed.execution.trace finish).validatorState
              validator).commitHead ≠
            ((timed.execution.trace start).validatorState
              validator).commitHead := Classical.byContradiction (by
      intro noChange
      apply stable
      intro current startBeforeCurrent
      exact Classical.byContradiction (by
        intro changed
        apply noChange
        exact ⟨current, startBeforeCurrent, changed⟩))
    rcases changedExists with ⟨finish, startBeforeFinish, changed⟩
    have durable := timed.execution.durableStateMonotone validator start finish
      input.2.1 startBeforeFinish
    have differentIndex :
        ((timed.execution.trace start).validatorState
            validator).commitHead.index ≠
          ((timed.execution.trace finish).validatorState
            validator).commitHead.index := by
      intro sameIndex
      exact changed (durable.2.2.1 sameIndex).symm
    exact Or.inl ⟨finish, startBeforeFinish,
      Nat.lt_of_le_of_ne durable.1 differentIndex⟩

/-- A ready recovery parent accumulator enters the paced exact-next path. -/
theorem ready_recovery_parent_need_eventually_produces_exact_or_commit_advance
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Nat} {need}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (needActive : (needs.trace start validator).active = some need)
    (recoveryOrigin : need.proposalOrigin = .commitProgressRecovery)
    (ready : ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace start).validatorState validator) need)
    (active : ∀ current, start ≤ current →
      (timed.execution.trace current).epochActive = true) :
    ValidatorExactNextOrCommitAdvance timed start validator := by
  have input := needs.ready_need_gives_timer_arm_input validatorInRange
    validatorCorrectAvailable needActive recoveryOrigin ready
  exact
    timer_input_eventually_produces_exact_or_commit_advance_from_authority_count
      timerSource arms source effects authorityCountAtLeastTwo
      input active

/-- Canonical genesis parents enter the same paced recovery path for round
one. -/
theorem canonical_genesis_eventually_produces_round_one_or_commit_advance
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
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ready : ValidatorCanonicalGenesisParentReadyAt timed start validator)
    (active : ∀ current, start ≤ current →
      (timed.execution.trace current).epochActive = true) :
    ValidatorExactNextOrCommitAdvance timed start validator := by
  have input :=
    ValidatorRecoveryParentNeedExecution.canonical_genesis_ready_gives_timer_arm_input
      ready validatorInRange validatorCorrectAvailable
  exact
    timer_input_eventually_produces_exact_or_commit_advance_from_authority_count
      timerSource arms source effects authorityCountAtLeastTwo
      input active

/-- One protected exact-next action completes one strict production phase. -/
theorem protected_exact_next_eventually_produces_sent_block
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {restartSnapshot : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (continuity : ValidatorStrictProposalRules syncRules obligations
      restartSnapshot)
    {start validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (next : ValidatorExactNextProposalAt timed start validator) :
    ValidatorStrictPhaseProduction timed start validator := by
  exact exact_next_production_is_strict validatorInRange
    (protected_exact_next_eventually_produces_exact_block source effects
      continuity validatorInRange validatorCorrectAvailable next)

/-- After the current signer-floor block is sent, one recovery-only phase
produces and sends the block at the exact next round. -/
theorem exact_recovery_phase_after_sent_produces_exact_next
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {restartSnapshot : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (needSources : ValidatorParentNeedSourceRules
      (timed := timed) restartSnapshot)
    (needProposals : ValidatorParentNeedProposalRules
      (timed := timed) restartSnapshot)
    (historyRules : ValidatorProducedCausalHistoryRules syncRules)
    (restartRules : ValidatorDurableRestartCausalSourceRules syncRules
      restartSnapshot)
    (continuity : ValidatorStrictProposalRules syncRules obligations
      restartSnapshot)
    {start validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ current, start ≤ current →
      (timed.execution.trace current).epochActive = true)
    (floorSent :
      ((timed.execution.trace start).validatorState validator).sentOwnBlockAt
        ((timed.execution.trace start).validatorState
          validator).highestSignedRound = true)
    (phase : ValidatorExactRecoveryPhaseAt syncRules obligations
      restartSnapshot start validator) :
    ValidatorExactNextProduction timed start validator := by
  cases phase with
  | ready ready recovery =>
      exact recovery_ready_proposal_eventually_produces_exact_block continuity
        validatorInRange validatorCorrectAvailable ready recovery
  | exactNext next =>
      exact protected_exact_next_eventually_produces_exact_block source
        effects continuity validatorInRange
        validatorCorrectAvailable next
  | persistedUnsent sameFloor _stored notSent _receiverInRange _receiverIsOther
      _sendGoal =>
      have sentAtRound := floorSent
      rw [sameFloor] at sentAtRound
      rw [sentAtRound] at notSent
      contradiction
  | parentSync pending =>
      rcases exact_next_parent_sync_eventually_supplies_proposal needSources
          needProposals historyRules pending validatorInRange
          validatorCorrectAvailable afterGst active with
        ⟨readyAt, next, startBeforeReady⟩
      have readyProduction :=
        protected_exact_next_eventually_produces_exact_block source
          effects continuity validatorInRange
          validatorCorrectAvailable next.proposal
      exact exact_next_production_starts_at_same_floor startBeforeReady
        next.sameSignerFloor readyProduction
  | restartParentSync pending =>
      rcases restart_parent_sync_eventually_supplies_proposal needSources
          needProposals historyRules restartRules pending validatorInRange
          validatorCorrectAvailable afterGst active with
        ⟨readyAt, next, startBeforeReady⟩
      have readyProduction :=
        protected_exact_next_eventually_produces_exact_block source
          effects continuity validatorInRange
          validatorCorrectAvailable next.proposal
      exact exact_next_production_starts_at_same_floor startBeforeReady
        next.sameSignerFloor readyProduction

/-- Recovery-only local continuation produces every finite exact-next offset.
The result keeps the persistence occurrence for the final block. -/
theorem exact_recovery_rules_iterate
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {restartSnapshot : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (needSources : ValidatorParentNeedSourceRules
      (timed := timed) restartSnapshot)
    (needProposals : ValidatorParentNeedProposalRules
      (timed := timed) restartSnapshot)
    (historyRules : ValidatorProducedCausalHistoryRules syncRules)
    (restartRules : ValidatorDurableRestartCausalSourceRules syncRules
      restartSnapshot)
    (continuity : ValidatorStrictProposalRules syncRules obligations
      restartSnapshot)
    {recoveryWait : Time}
    (recoveryRules : ValidatorExactRecoveryRules syncRules obligations
      restartSnapshot recoveryWait)
    {start validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ current, start ≤ current →
      (timed.execution.trace current).epochActive = true)
    (recovering : ∀ current, start ≤ current →
      ValidatorCommitProgressRecoveryModeAt timed recoveryWait current
        validator)
    (floorSent :
      ((timed.execution.trace start).validatorState validator).sentOwnBlockAt
        ((timed.execution.trace start).validatorState
          validator).highestSignedRound = true) :
    ∀ count,
      ValidatorExactOffsetProduction timed start validator (count + 1) := by
  intro count
  induction count generalizing start with
  | zero =>
      have phase := active_recovery_host_has_exact_phase recoveryRules
        validatorInRange validatorCorrectAvailable (active start (Nat.le_refl _))
        (recovering start (Nat.le_refl _))
      rcases exact_recovery_phase_after_sent_produces_exact_next source
          effects needSources needProposals historyRules
          restartRules continuity validatorInRange validatorCorrectAvailable
          afterGst active floorSent phase with
        ⟨persistTime, finish, block, startBeforePersist, persistBeforeFinish,
          blockRound, persisted, stored, sent, floorAtFinish⟩
      exact ⟨persistTime, finish, block, startBeforePersist,
        persistBeforeFinish, by simpa using blockRound, persisted, by simpa using
          stored, by simpa using sent, by simpa using floorAtFinish⟩
  | succ count inductionHypothesis =>
      have phase := active_recovery_host_has_exact_phase recoveryRules
        validatorInRange validatorCorrectAvailable (active start (Nat.le_refl _))
        (recovering start (Nat.le_refl _))
      rcases exact_recovery_phase_after_sent_produces_exact_next source
          effects needSources needProposals historyRules
          restartRules continuity validatorInRange validatorCorrectAvailable
          afterGst active floorSent phase with
        ⟨firstPersist, firstFinish, firstBlock, startBeforeFirstPersist,
          firstPersistBeforeFinish, firstRound, firstPersisted, firstStored,
          firstSent, firstFloor⟩
      have startBeforeFirstFinish : start ≤ firstFinish :=
        Nat.le_trans startBeforeFirstPersist
          (Nat.le_trans (Nat.le_succ _) firstPersistBeforeFinish)
      have afterGstAtFirst : network.gst ≤ firstFinish :=
        Nat.le_trans afterGst startBeforeFirstFinish
      have activeAfterFirst : ∀ current, firstFinish ≤ current →
          (timed.execution.trace current).epochActive = true := by
        intro current firstBeforeCurrent
        exact active current
          (Nat.le_trans startBeforeFirstFinish firstBeforeCurrent)
      have recoveringAfterFirst : ∀ current, firstFinish ≤ current →
          ValidatorCommitProgressRecoveryModeAt timed recoveryWait current
            validator := by
        intro current firstBeforeCurrent
        exact recovering current
          (Nat.le_trans startBeforeFirstFinish firstBeforeCurrent)
      have floorSentAtFirst :
          ((timed.execution.trace firstFinish).validatorState
            validator).sentOwnBlockAt
              ((timed.execution.trace firstFinish).validatorState
                validator).highestSignedRound = true := by
        rw [firstFloor]
        exact firstSent
      rcases inductionHypothesis afterGstAtFirst activeAfterFirst
          recoveringAfterFirst floorSentAtFirst with
        ⟨persistTime, finish, block, firstBeforePersist, persistBeforeFinish,
          blockRound, persisted, stored, sent, floorAtFinish⟩
      have targetRoundEquality :
          ((timed.execution.trace firstFinish).validatorState
              validator).highestSignedRound + (count + 1) =
            ((timed.execution.trace start).validatorState
              validator).highestSignedRound + (Nat.succ count + 1) := by
        rw [firstFloor]
        omega
      refine ⟨persistTime, finish, block,
        Nat.le_trans startBeforeFirstFinish firstBeforePersist,
        persistBeforeFinish, ?_, persisted, ?_, ?_, ?_⟩
      · rw [blockRound, firstFloor]
        omega
      · rw [← targetRoundEquality]
        exact stored
      · rw [← targetRoundEquality]
        exact sent
      · rw [floorAtFinish, firstFloor]
        omega
/-- Every current local phase completes one durable own-block send. Parent
synchronization uses only the source derived from a prior persistence action.
-/
theorem strict_proposal_phase_eventually_produces_sent_block
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {restartSnapshot : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (needSources : ValidatorParentNeedSourceRules
      (timed := timed) restartSnapshot)
    (needProposals : ValidatorParentNeedProposalRules
      (timed := timed) restartSnapshot)
    (historyRules : ValidatorProducedCausalHistoryRules syncRules)
    (restartRules : ValidatorDurableRestartCausalSourceRules syncRules
      restartSnapshot)
    (continuity : ValidatorStrictProposalRules syncRules obligations
      restartSnapshot)
    {start validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ current, start ≤ current →
      (timed.execution.trace current).epochActive = true)
    (phase : ValidatorStrictProposalPhaseAt syncRules obligations
      restartSnapshot start validator) :
    ValidatorStrictPhaseProduction timed start validator := by
  cases phase with
  | ready ready sameRound =>
      rename_i round proposal
      rcases proposal_work_eventually_produces_sent_block_with_peer
          validatorInRange validatorCorrectAvailable
          (continuity.broadcastReceiver validator)
          (continuity.broadcastReceiverInRange validator validatorInRange)
          (continuity.broadcastReceiverIsOther validator validatorInRange)
          (.ready proposal ready sameRound) with
        ⟨finish, reference, startBeforeFinish, stored, sent⟩
      have legal := obligations.readyProposalIsLegal start validator proposal
        ready
      have aboveFloor :
          ((timed.execution.trace start).validatorState
            validator).highestSignedRound < round := by
        simpa [sameRound] using legal.2.1
      exact ⟨finish, round, reference, startBeforeFinish,
        Nat.le_of_lt aboveFloor,
        round_above_signer_floor_is_not_sent validatorInRange aboveFloor,
        stored, sent⟩
  | exactNext next =>
      exact protected_exact_next_eventually_produces_sent_block source
        effects continuity validatorInRange
        validatorCorrectAvailable next
  | persistedUnsent sameFloor stored notSent receiverInRange receiverIsOther
      sendGoal =>
      rename_i round reference receiver
      rcases proposal_work_eventually_produces_sent_block_with_peer
          validatorInRange validatorCorrectAvailable receiver receiverInRange
          receiverIsOther
          (.persistedUnsent reference receiver sameFloor stored notSent
            receiverInRange receiverIsOther sendGoal) with
        ⟨finish, resultReference, startBeforeFinish, resultStored, resultSent⟩
      exact ⟨finish, round, resultReference, startBeforeFinish,
        Nat.le_of_eq sameFloor, notSent, resultStored, resultSent⟩
  | parentSync pending =>
      rcases exact_next_parent_sync_eventually_supplies_proposal needSources
          needProposals historyRules pending validatorInRange
          validatorCorrectAvailable afterGst active with
        ⟨readyAt, next, startBeforeReady⟩
      have readyProduction :=
        protected_exact_next_eventually_produces_sent_block source effects
          continuity validatorInRange validatorCorrectAvailable
          next.proposal
      exact strict_phase_production_starts_earlier validatorInRange
        startBeforeReady readyProduction
  | restartParentSync pending =>
      rcases restart_parent_sync_eventually_supplies_proposal needSources
          needProposals historyRules restartRules pending validatorInRange
          validatorCorrectAvailable afterGst active with
        ⟨readyAt, next, startBeforeReady⟩
      have readyProduction :=
        protected_exact_next_eventually_produces_sent_block source effects
          continuity validatorInRange validatorCorrectAvailable
          next.proposal
      exact strict_phase_production_starts_earlier validatorInRange
        startBeforeReady readyProduction

/-- A completed phase after one durable sent block must use a higher round. -/
theorem sent_block_precedes_strict_phase_production
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {sentAt phaseAt validator oldRound : Nat}
    {oldReference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (ordered : sentAt ≤ phaseAt)
    (oldStored :
      ((timed.execution.trace sentAt).validatorState validator).ownBlockAt
        oldRound = some oldReference)
    (oldSent :
      ((timed.execution.trace sentAt).validatorState validator).sentOwnBlockAt
        oldRound = true)
    (production : ValidatorStrictPhaseProduction timed phaseAt validator) :
    ∃ finish newRound newReference,
      phaseAt ≤ finish ∧
      oldRound < newRound ∧
      ((timed.execution.trace finish).validatorState validator).ownBlockAt
        newRound = some newReference ∧
      ((timed.execution.trace finish).validatorState validator).sentOwnBlockAt
        newRound = true := by
  rcases production with
    ⟨finish, newRound, newReference, phaseBeforeFinish, floorBelowRound,
      newNotSentAtPhase, newStored, newSent⟩
  have durable := timed.execution.durableStateMonotone validator sentAt phaseAt
    validatorInRange ordered
  have oldStoredAtPhase := durable.own_block_persists oldStored
  have oldSentAtPhase := durable.sent_own_block_persists oldSent
  have oldBelowFloor :=
    (timed.execution.statesWellFormed phaseAt validator validatorInRange)
      |>.ownBlockDoesNotExceedSignerFloor oldRound oldReference
        oldStoredAtPhase
  have differentRounds : oldRound ≠ newRound := by
    intro sameRound
    subst newRound
    rw [oldSentAtPhase] at newNotSentAtPhase
    contradiction
  exact ⟨finish, newRound, newReference, phaseBeforeFinish, by omega,
    newStored, newSent⟩

/-- Starting after one sent own block, the local phase invariant produces any
finite number of higher sent own blocks. -/
theorem strict_proposal_rules_iterate_after_sent
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {restartSnapshot : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (needSources : ValidatorParentNeedSourceRules
      (timed := timed) restartSnapshot)
    (needProposals : ValidatorParentNeedProposalRules
      (timed := timed) restartSnapshot)
    (historyRules : ValidatorProducedCausalHistoryRules syncRules)
    (restartRules : ValidatorDurableRestartCausalSourceRules syncRules
      restartSnapshot)
    (continuity : ValidatorStrictProposalRules syncRules obligations
      restartSnapshot)
    {start validator oldRound : Nat}
    {oldReference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ current, start ≤ current →
      (timed.execution.trace current).epochActive = true)
    (oldStored :
      ((timed.execution.trace start).validatorState validator).ownBlockAt
        oldRound = some oldReference)
    (oldSent :
      ((timed.execution.trace start).validatorState validator).sentOwnBlockAt
        oldRound = true) :
    ∀ count,
      ∃ finish round reference,
        start ≤ finish ∧
        oldRound + count + 1 ≤ round ∧
        ((timed.execution.trace finish).validatorState validator).ownBlockAt
          round = some reference ∧
        ((timed.execution.trace finish).validatorState validator).sentOwnBlockAt
          round = true := by
  intro count
  induction count generalizing start oldRound oldReference with
  | zero =>
      have phase := active_host_has_strict_proposal_phase continuity
        validatorInRange validatorCorrectAvailable (active start (Nat.le_refl _))
      have production := strict_proposal_phase_eventually_produces_sent_block
        source effects needSources needProposals historyRules
          restartRules continuity validatorInRange validatorCorrectAvailable
          afterGst active phase
      rcases sent_block_precedes_strict_phase_production validatorInRange
          (Nat.le_refl _) oldStored oldSent production with
        ⟨finish, round, reference, startBeforeFinish, higher, stored, sent⟩
      exact ⟨finish, round, reference, startBeforeFinish, by omega, stored,
        sent⟩
  | succ count inductionHypothesis =>
      have phase := active_host_has_strict_proposal_phase continuity
        validatorInRange validatorCorrectAvailable (active start (Nat.le_refl _))
      have production := strict_proposal_phase_eventually_produces_sent_block
        source effects needSources needProposals historyRules
          restartRules continuity validatorInRange validatorCorrectAvailable
          afterGst active phase
      rcases sent_block_precedes_strict_phase_production validatorInRange
          (Nat.le_refl _) oldStored oldSent production with
        ⟨middle, middleRound, middleReference, startBeforeMiddle, higher,
          middleStored, middleSent⟩
      have afterGstAtMiddle : network.gst ≤ middle :=
        Nat.le_trans afterGst startBeforeMiddle
      have activeAfterMiddle : ∀ current, middle ≤ current →
          (timed.execution.trace current).epochActive = true := by
        intro current middleBeforeCurrent
        exact active current (Nat.le_trans startBeforeMiddle middleBeforeCurrent)
      rcases inductionHypothesis afterGstAtMiddle activeAfterMiddle middleStored
          middleSent with
        ⟨finish, round, reference, middleBeforeFinish, roundBound, stored,
          sent⟩
      refine ⟨finish, round, reference,
        Nat.le_trans startBeforeMiddle middleBeforeFinish, ?_, stored, sent⟩
      omega

/-- The one-host persistent phase invariant gives unbounded block production
for every correct, available validator, even while commits continue. -/
theorem strict_proposal_rules_give_block_production_liveness
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {restartSnapshot : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (needSources : ValidatorParentNeedSourceRules
      (timed := timed) restartSnapshot)
    (needProposals : ValidatorParentNeedProposalRules
      (timed := timed) restartSnapshot)
    (historyRules : ValidatorProducedCausalHistoryRules syncRules)
    (restartRules : ValidatorDurableRestartCausalSourceRules syncRules
      restartSnapshot)
    (continuity : ValidatorStrictProposalRules syncRules obligations
      restartSnapshot) :
    BlockProductionLiveness config faults network timed.execution.trace := by
  intro validator start minimumRound validatorInRange validatorCorrectAvailable
    afterGst active
  have firstPhase := active_host_has_strict_proposal_phase continuity
    validatorInRange validatorCorrectAvailable (active start (Nat.le_refl _))
  rcases strict_proposal_phase_eventually_produces_sent_block source
      effects needSources needProposals historyRules restartRules
      continuity validatorInRange validatorCorrectAvailable afterGst active
      firstPhase with
    ⟨firstFinish, firstRound, firstReference, startBeforeFirst,
      startFloorBelowFirst, _firstNotSent, firstStored, firstSent⟩
  have afterGstAtFirst : network.gst ≤ firstFinish :=
    Nat.le_trans afterGst startBeforeFirst
  have activeAfterFirst : ∀ current, firstFinish ≤ current →
      (timed.execution.trace current).epochActive = true := by
    intro current firstBeforeCurrent
    exact active current (Nat.le_trans startBeforeFirst firstBeforeCurrent)
  rcases strict_proposal_rules_iterate_after_sent source effects
      needSources needProposals historyRules restartRules continuity
      validatorInRange validatorCorrectAvailable
      afterGstAtFirst activeAfterFirst firstStored firstSent minimumRound with
    ⟨finish, round, reference, firstBeforeFinish, roundBound, stored, sent⟩
  refine ⟨finish, round, Nat.le_trans startBeforeFirst firstBeforeFinish,
    ?_, ?_, ?_, sent⟩
  · omega
  · omega
  · simp [stored]

end Mysticeti
