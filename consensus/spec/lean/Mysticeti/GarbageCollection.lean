/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Lean.Elab.Tactic.Omega

namespace Mysticeti

/-! Garbage-collection boundaries used by the core and transaction proofs.

The Rust refinement obligations are `ASM-SAFE-GC`,
`ASM-SAFE-COMMITTED-PREFIX`, and `ASM-SAFE-EVIDENCE-REFINEMENT`.
-/

/-- Mysticeti v3 uses a depth-two indirect decision. -/
def indirectCommitDepth : Nat := 2

/-- The Rust `DagState::calculate_gc_round` rule. -/
def gcRound (lastCommitRound gcDepth : Nat) : Nat :=
  lastCommitRound - gcDepth

/-- A block round is retained only when it is above the GC boundary. -/
def Retained (boundary blockRound : Nat) : Prop :=
  boundary < blockRound

/-- Evidence that is still in the DAG and evidence that is already in the
pending committed prefix are different stores. -/
structure BlockEvidenceStore (Evidence : Type) where
  dag : Evidence
  committedPrefix : Evidence

namespace BlockEvidenceStore

/-- Block GC changes the DAG store. It does not change the pending committed
prefix that the modeled v3 transaction finalizer owns. -/
def collectDag {Evidence : Type} (store : BlockEvidenceStore Evidence)
    (remainingDag : Evidence) : BlockEvidenceStore Evidence :=
  { dag := remainingDag
    committedPrefix := store.committedPrefix }

@[simp]
theorem committed_prefix_unchanged {Evidence : Type}
    (store : BlockEvidenceStore Evidence) (remainingDag : Evidence) :
    (store.collectDag remainingDag).committedPrefix = store.committedPrefix := by
  rfl

end BlockEvidenceStore

/-- The GC state that Core reads before it makes the next leader decision.

`nextLeaderRound` maps to the v3 schedule rule that starts the next leader round
after the last committed leader round. `depthExceedsIndirect` maps to the v3
configuration check. -/
structure CoreGcState where
  gcDepth : Nat
  lastCommitRound : Nat
  decisionRound : Nat
  depthExceedsIndirect : indirectCommitDepth < gcDepth
  nextLeaderRound : lastCommitRound < decisionRound

namespace CoreGcState

def boundary (state : CoreGcState) : Nat :=
  gcRound state.lastCommitRound state.gcDepth

def votingRound (state : CoreGcState) : Nat :=
  state.decisionRound + 1

/-- Every round from the pending leader round through the anchor is above the
GC boundary that Core reads before it records the new commit. -/
def EvidenceRetained (state : CoreGcState) (anchorRound : Nat) : Prop :=
  state.decisionRound + indirectCommitDepth <= anchorRound ∧
    ∀ round, state.decisionRound <= round → round <= anchorRound →
      Retained state.boundary round

theorem round_from_decision_is_retained (state : CoreGcState) {round : Nat}
    (notEarlier : state.decisionRound <= round) :
    Retained state.boundary round := by
  unfold Retained boundary gcRound
  have lastBeforeRound : state.lastCommitRound < round :=
    Nat.lt_of_lt_of_le state.nextLeaderRound notEarlier
  exact Nat.lt_of_le_of_lt (Nat.sub_le _ _) lastBeforeRound

theorem direct_evidence_retained (state : CoreGcState) :
    Retained state.boundary state.decisionRound ∧
      Retained state.boundary state.votingRound := by
  constructor
  · exact state.round_from_decision_is_retained (by omega)
  · exact state.round_from_decision_is_retained (by
      simp [votingRound])

theorem evidence_retained (state : CoreGcState) {anchorRound : Nat}
    (depth : state.decisionRound + indirectCommitDepth <= anchorRound) :
    state.EvidenceRetained anchorRound := by
  constructor
  · exact depth
  · intro round notEarlier _
    exact state.round_from_decision_is_retained notEarlier

end CoreGcState

/-- GC data for one transaction target in the first pending commit.

If the first commit leader is already two rounds above the target, that commit
can preserve the voting-round evidence. Otherwise, the first finalizer trigger
preserves it. V3 sub-DAG construction uses the GC boundary from the preceding
commit. -/
structure TransactionGcWindow where
  gcDepth : Nat
  targetRound : Nat
  firstCommitLeaderRound : Nat
  /-- The GC boundary used when the first commit sub-DAG was built. For a commit
  installed through commit sync, this is a property of the explicit block list in
  `CertifiedCommit`. -/
  firstCommitGcRound : Nat
  depthExceedsIndirect : indirectCommitDepth < gcDepth
  targetInFirstCommit : targetRound <= firstCommitLeaderRound
  targetAboveFirstGc : Retained firstCommitGcRound targetRound

namespace TransactionGcWindow

def votingRound (window : TransactionGcWindow) : Nat :=
  window.targetRound + 1

def firstCommitIsDeep (window : TransactionGcWindow) : Prop :=
  window.targetRound + indirectCommitDepth <= window.firstCommitLeaderRound

/-- This is the GC boundary used to preserve the anchor voting blocks.

The first commit boundary is sufficient when that commit is already deep enough
for the transaction target. In the near case, use the boundary that the first
trigger reads from its preceding commit. -/
def evidenceGcRound (window : TransactionGcWindow)
    (triggerPreviousLeaderRound : Nat) : Nat :=
  if window.targetRound + indirectCommitDepth <= window.firstCommitLeaderRound then
    window.firstCommitGcRound
  else
    gcRound triggerPreviousLeaderRound window.gcDepth

/-- `gc_depth > 2` preserves the next-round votes in both target positions.

For a deep target, the first commit preserves the votes. For a target near the
first commit leader, the first trigger preserves them because its preceding
leader is still below the depth-two boundary. -/
theorem voting_round_retained (window : TransactionGcWindow)
    {triggerPreviousLeaderRound : Nat}
    (previousBeforeTrigger :
      triggerPreviousLeaderRound <
        window.firstCommitLeaderRound + indirectCommitDepth) :
    Retained (window.evidenceGcRound triggerPreviousLeaderRound)
      window.votingRound := by
  unfold evidenceGcRound
  split <;> rename_i deep
  · unfold Retained votingRound
    have targetAboveFirstGc := window.targetAboveFirstGc
    unfold Retained at targetAboveFirstGc
    omega
  · unfold Retained votingRound gcRound
    have targetInFirstCommit := window.targetInFirstCommit
    have depthExceedsIndirect := window.depthExceedsIndirect
    simp [indirectCommitDepth] at deep previousBeforeTrigger depthExceedsIndirect
    omega

end TransactionGcWindow

end Mysticeti
