/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorBlockSync

namespace Mysticeti

/-! Durable causal data for restart recovery.

A capsule contains block bodies in parent-first order. A correct holder protects
the complete finite history from GC. Block synchronization derives delivery and
ordered acceptance from local retry, service, and acceptance actions.

The full causal closure is a simple GC-safe contract. An implementation can use
a shorter closure only if a separate proof shows that the receiver has moved its
GC boundary far enough. This module does not assume that synchronization has
completed or that a parent layer is already installed.
-/

/-- All items in one finite history are accepted at one validator and time. -/
def AllHistoryItemsAcceptedAt {Item : Type}
    (trace : BlockSyncTrace Item) (validator : Nat)
    (history : List Item) (time : Time) : Prop :=
  ∀ item, item ∈ history → AcceptedAt trace validator item time

/-- All items in one finite history remain in local storage. -/
def AllHistoryItemsRetainedAt {Item : Type}
    (trace : BlockSyncTrace Item) (validator : Nat)
    (history : List Item) (time : Time) : Prop :=
  ∀ item, item ∈ history → RetainedAt trace validator item time

/-- Accepted recovery bodies are retained. This is the extra local storage rule
that ordinary accepted-reference monotonicity does not provide. -/
structure RecoveryBodyStorageRules
    {Item : Type} (trace : BlockSyncTrace Item) : Prop
    extends BlockSyncStorageRules trace where
  acceptedBodyIsRetained : ∀ validator item time,
    AcceptedAt trace validator item time →
      RetainedAt trace validator item time

/-- One validator retains and accepts one complete finite history. -/
structure RetainedCorrectCausalHistory
    {Item CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (trace : BlockSyncTrace Item)
    (holder : Nat) (history : List Item) (start : Time) : Prop where
  holderInRange : holder < config.authorityCount
  holderCorrectAvailable : faults.correctAvailable holder = true
  retainedAtStart : AllHistoryItemsRetainedAt trace holder history start
  acceptedAtStart : AllHistoryItemsAcceptedAt trace holder history start

/-- A missing item from one protected history remains a local sync goal. This
rule is for one requester. It does not state that the request completes. -/
structure ProtectedCausalHistoryRequestRules
    {Item CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (trace : BlockSyncTrace Item)
    (holder : Nat) (history : List Item) : Prop where
  missingItemIsNeeded : ∀ time requester item,
    requester < config.authorityCount →
    faults.correctAvailable requester = true →
    item ∈ history →
    requester ≠ holder →
    ¬AcceptedAt trace requester item time →
    NeededAt trace requester item time

/-- One retained history item is eventually accepted after its earlier items. -/
theorem retained_causal_item_eventually_accepted
    {Item CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket : AddressedPacket (BlockSyncMessage Item) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {trace : BlockSyncTrace Item}
    (storage : BlockSyncStorageRules trace)
    (requests : FairProtectedRequestActions config faults protocolPacket trace)
    (serving : FairProtectedServeActions config faults protocolPacket trace)
    (accepting : FairProtectedAcceptActions config faults protocolPacket trace)
    {holder requester : Nat}
    {fullHistory earlierItems laterItems : List Item}
    {item : Item} {sourceStart current : Time}
    (source : RetainedCorrectCausalHistory config faults trace holder fullHistory
      sourceStart)
    (requestRules : ProtectedCausalHistoryRequestRules config faults trace holder
      fullHistory)
    (sourceBeforeCurrent : sourceStart ≤ current)
    (afterGst : network.gst ≤ current)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true)
    (historyOrder : fullHistory = earlierItems ++ item :: laterItems)
    (earlierAccepted :
      EarlierHistoryAccepted trace requester earlierItems current) :
    ∃ finish,
      current ≤ finish ∧ AcceptedAt trace requester item finish := by
  by_cases alreadyAccepted : AcceptedAt trace requester item current
  · exact ⟨current, Nat.le_refl _, alreadyAccepted⟩
  have itemInHistory : item ∈ fullHistory := by
    rw [historyOrder]
    simp
  have retainedAtCurrent : RetainedAt trace holder item current :=
    storage.retainedMonotone holder item sourceStart current sourceBeforeCurrent
      (source.retainedAtStart item itemInHistory)
  have acceptedAtCurrent : AcceptedAt trace holder item current :=
    storage.acceptedMonotone holder item sourceStart current sourceBeforeCurrent
      (source.acceptedAtStart item itemInHistory)
  have itemSource : RetainedCorrectOwnerItem config faults trace holder item
      current :=
    ⟨source.holderInRange, source.holderCorrectAvailable, retainedAtCurrent,
      acceptedAtCurrent⟩
  have requesterNeeds : requester ≠ holder →
      NeededAt trace requester item current := by
    intro requesterNotHolder
    exact requestRules.missingItemIsNeeded current requester item
      requesterInRange requesterCorrect itemInHistory requesterNotHolder
      alreadyAccepted
  exact retained_owner_history_item_eventually_accepted storage requests serving
    accepting itemSource requesterInRange requesterCorrect afterGst
    requesterNeeds historyOrder earlierAccepted

/-- A complete retained finite causal history is eventually accepted in order by
one correct requester. -/
theorem retained_causal_history_eventually_accepted
    {Item CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket : AddressedPacket (BlockSyncMessage Item) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {trace : BlockSyncTrace Item}
    (storage : BlockSyncStorageRules trace)
    (requests : FairProtectedRequestActions config faults protocolPacket trace)
    (serving : FairProtectedServeActions config faults protocolPacket trace)
    (accepting : FairProtectedAcceptActions config faults protocolPacket trace)
    {holder requester : Nat}
    {history : List Item} {start : Time}
    (source : RetainedCorrectCausalHistory config faults trace holder history start)
    (requestRules : ProtectedCausalHistoryRequestRules config faults trace holder
      history)
    (afterGst : network.gst ≤ start)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true) :
    ∃ finish,
      start ≤ finish ∧
        AllHistoryItemsAcceptedAt trace requester history finish := by
  have advance : ∀ (remaining acceptedItems : List Item) (current : Time),
      history = acceptedItems ++ remaining →
      start ≤ current →
      EarlierHistoryAccepted trace requester acceptedItems current →
      ∃ finish,
        current ≤ finish ∧
          AllHistoryItemsAcceptedAt trace requester history finish := by
    intro remaining
    induction remaining with
    | nil =>
        intro acceptedItems current historySplit startBeforeCurrent itemsAccepted
        refine ⟨current, Nat.le_refl _, ?_⟩
        intro item itemInHistory
        have itemInAccepted : item ∈ acceptedItems := by
          simpa [historySplit] using itemInHistory
        exact itemsAccepted item itemInAccepted
    | cons item tail inductionHypothesis =>
        intro acceptedItems current historySplit startBeforeCurrent itemsAccepted
        have currentAfterGst : network.gst ≤ current :=
          Nat.le_trans afterGst startBeforeCurrent
        rcases retained_causal_item_eventually_accepted storage requests serving
            accepting source requestRules startBeforeCurrent currentAfterGst
            requesterInRange requesterCorrect historySplit itemsAccepted with
          ⟨itemFinish, currentBeforeItem, itemAccepted⟩
        have acceptedItemsAtItem :
            EarlierHistoryAccepted trace requester acceptedItems itemFinish := by
          intro earlier earlierInAccepted
          exact storage.acceptedMonotone requester earlier current itemFinish
            currentBeforeItem (itemsAccepted earlier earlierInAccepted)
        have extendedAccepted : EarlierHistoryAccepted trace requester
            (acceptedItems ++ [item]) itemFinish := by
          intro earlier earlierInExtended
          rcases List.mem_append.mp earlierInExtended with
            earlierInAccepted | earlierIsItem
          · exact acceptedItemsAtItem earlier earlierInAccepted
          · have sameItem : earlier = item := by
              simpa using earlierIsItem
            subst earlier
            exact itemAccepted
        have nextSplit : history = (acceptedItems ++ [item]) ++ tail := by
          simpa [List.append_assoc] using historySplit
        rcases inductionHypothesis (acceptedItems ++ [item]) itemFinish nextSplit
            (Nat.le_trans startBeforeCurrent currentBeforeItem)
            extendedAccepted with ⟨finish, itemBeforeFinish, allAccepted⟩
        exact ⟨finish, Nat.le_trans currentBeforeItem itemBeforeFinish,
          allAccepted⟩
  have emptyAccepted : EarlierHistoryAccepted trace requester [] start := by
    intro item itemInEmpty
    simp at itemInEmpty
  exact advance history [] start (by simp) (Nat.le_refl _) emptyAccepted

/-- Accepted recovery bodies are retained at the same time. -/
theorem accepted_history_is_retained
    {Item : Type} {trace : BlockSyncTrace Item}
    (storage : RecoveryBodyStorageRules trace)
    {validator : Nat} {history : List Item} {time : Time}
    (accepted : AllHistoryItemsAcceptedAt trace validator history time) :
    AllHistoryItemsRetainedAt trace validator history time := by
  intro item itemMember
  exact storage.acceptedBodyIsRetained validator item time
    (accepted item itemMember)

/-- One protected causal bundle. The list contains block bodies. The target is
last. Each non-genesis parent body occurs before its child. -/
structure CausalRecoveryCapsule
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId) where
  targetBlock : ValidatorBlock BlockId
  targetPositive : 0 < targetBlock.reference.round
  targetValid : targetBlock.HasQuorumImmediateParents config
  /-- Exact static roots at which recursive synchronization can stop. A later
  composition theorem checks these references against the epoch genesis list. -/
  genesisParents : List (ValidatorBlockRef BlockId)
  genesisParentsNodup : genesisParents.Nodup
  genesisParentsAreRoundZero : ∀ parent,
    parent ∈ genesisParents → parent.round = 0
  history : List (ValidatorBlock BlockId)
  historyReferencesNodup :
    (history.map ValidatorBlock.reference).Nodup
  targetIsLast : ∃ earlier, history = earlier ++ [targetBlock]
  /-- Static genesis bodies are not copied into the recovery capsule. -/
  historyBlocksPositive : ∀ block, block ∈ history →
    0 < block.reference.round
  /-- Each non-target body is a direct parent of a later body. Together with
  the topological order, this makes the history the finite causal closure of
  the target. A requester can therefore discover every body by walking parent
  references from the delivered target. -/
  historyHasNoUnrelatedBlocks : ∀ block, block ∈ history →
    block = targetBlock ∨
      ∃ child,
        child ∈ history ∧ block.reference ∈ child.parents
  positiveHistoryBlocksValid : ∀ block, block ∈ history →
    0 < block.reference.round → block.HasQuorumImmediateParents config
  targetParentsInHistory : ∀ parent, parent ∈ targetBlock.parents →
    parent ∈ genesisParents ∨
      ∃ parentBlock,
        parentBlock ∈ history ∧ parentBlock.reference = parent
  historyClosed : ∀ block, block ∈ history →
    ∀ parent, parent ∈ block.parents →
      parent ∈ genesisParents ∨
        ∃ parentBlock,
          parentBlock ∈ history ∧ parentBlock.reference = parent
  historyTopological : ∀ block parentBlock,
    block ∈ history →
    parentBlock ∈ history →
    parentBlock.reference ∈ block.parents →
    ∃ before between after,
      history = before ++ parentBlock :: between ++ block :: after

namespace CausalRecoveryCapsule

variable {BlockId CommitId : Type}
variable {config : ValidatorEpochConfig CommitId}

/-- The capsule target round. -/
def targetRound
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config) : Nat :=
  capsule.targetBlock.reference.round

/-- The target and each immediate parent body occur in the history. -/
theorem target_and_parents_in_history
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config) :
    capsule.targetBlock ∈ capsule.history ∧
      ∀ parent, parent ∈ capsule.targetBlock.parents →
        parent.round = 0 ∨
          ∃ parentBlock,
            parentBlock ∈ capsule.history ∧
              parentBlock.reference = parent := by
  constructor
  · obtain ⟨earlier, historyShape⟩ := capsule.targetIsLast
    rw [historyShape]
    simp
  · intro parent parentMember
    rcases capsule.targetParentsInHistory parent parentMember with
      genesis | ⟨parentBlock, parentBlockMember, parentReference⟩
    · exact Or.inl (capsule.genesisParentsAreRoundZero parent genesis)
    · exact Or.inr ⟨parentBlock, parentBlockMember, parentReference⟩

/-- Each non-target history block has a later child in the same history. This
is the source-side reachability fact used by a recursive requester proof. -/
theorem non_target_block_has_later_child
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    {block : ValidatorBlock BlockId}
    (member : block ∈ capsule.history)
    (notTarget : block ≠ capsule.targetBlock) :
    ∃ child before between after,
      child ∈ capsule.history ∧
        block.reference ∈ child.parents ∧
        capsule.history =
          before ++ block :: between ++ child :: after := by
  rcases capsule.historyHasNoUnrelatedBlocks block member with
    sameTarget | ⟨child, childMember, parentMember⟩
  · exact False.elim (notTarget sameTarget)
  · rcases capsule.historyTopological child block childMember member
        parentMember with
      ⟨before, between, after, ordered⟩
    exact ⟨child, before, between, after, childMember, parentMember, ordered⟩

/-- Ordered acceptance gives accepted target and parent bodies. -/
theorem accepted_history_gives_target_and_parents
    {trace : BlockSyncTrace (ValidatorBlock BlockId)}
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    {validator time : Nat}
    (accepted : AllHistoryItemsAcceptedAt trace validator capsule.history time) :
    AcceptedAt trace validator capsule.targetBlock time ∧
      ∀ parent, parent ∈ capsule.targetBlock.parents →
        parent.round = 0 ∨
          ∃ parentBlock,
            parentBlock.reference = parent ∧
              AcceptedAt trace validator parentBlock time := by
  refine ⟨accepted capsule.targetBlock
      capsule.target_and_parents_in_history.1, ?_⟩
  intro parent parentMember
  rcases capsule.target_and_parents_in_history.2 parent parentMember with
    parentGenesis | ⟨parentBlock, parentInHistory, parentReference⟩
  · exact Or.inl parentGenesis
  · exact Or.inr ⟨parentBlock, parentReference,
      accepted parentBlock parentInHistory⟩

end CausalRecoveryCapsule

/-- One validator's durable state at the start of recovery.

At signer floor zero, the validator has no authored round-zero block. It uses
the canonical genesis base. A positive signer floor has one durable own block.
A validator with no positive capsule must be at the genesis base. -/
structure InitialRecoverySource
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (holder : Nat) where
  signerFloor : Nat
  gcRound : Nat
  durableOwnBlock : Option (ValidatorBlock BlockId)
  ownBlockAtPositiveFloor : 0 < signerFloor →
    ∃ block,
      durableOwnBlock = some block ∧
        block.reference.author = holder ∧
        block.reference.round = signerFloor
  noAuthoredGenesisBlock : signerFloor = 0 → durableOwnBlock = none
  capsule : Option (CausalRecoveryCapsule (BlockId := BlockId) config)
  noCapsuleOnlyAtGenesis : capsule = none →
    signerFloor = 0 ∧ gcRound = 0
  capsuleAtOrAboveFloor : ∀ positiveCapsule,
    capsule = some positiveCapsule →
      signerFloor ≤ positiveCapsule.targetRound
  localGcWindow : ∀ positiveCapsule,
    capsule = some positiveCapsule →
      (positiveCapsule.targetRound = 1 ∧ gcRound = 0) ∨
        gcRound + 1 < positiveCapsule.targetRound

namespace InitialRecoverySource

variable {BlockId CommitId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {holder : Nat}

/-- The recovery base round. Canonical genesis starts round one. -/
def recoveryRound
    (source : InitialRecoverySource (BlockId := BlockId) config holder) : Nat :=
  match source.capsule with
  | none => 1
  | some capsule => capsule.targetRound

theorem recovery_round_positive
    (source : InitialRecoverySource (BlockId := BlockId) config holder) :
    0 < source.recoveryRound := by
  unfold recoveryRound
  split
  · omega
  · rename_i capsule capsulePresent
    exact capsule.targetPositive

theorem signer_floor_le_recovery_round
    (source : InitialRecoverySource (BlockId := BlockId) config holder) :
    source.signerFloor ≤ source.recoveryRound := by
  unfold recoveryRound
  split
  · rename_i noCapsule
    have atGenesis := (source.noCapsuleOnlyAtGenesis noCapsule).1
    omega
  · rename_i capsule capsulePresent
    exact source.capsuleAtOrAboveFloor capsule capsulePresent

/-- The GC boundary is below the parent round of the local recovery base, or the
base is round one and uses canonical genesis. -/
theorem recovery_gc_window
    (source : InitialRecoverySource (BlockId := BlockId) config holder) :
    (source.recoveryRound = 1 ∧ source.gcRound = 0) ∨
      source.gcRound + 1 < source.recoveryRound := by
  unfold recoveryRound
  split
  · rename_i noCapsule
    exact Or.inl ⟨rfl, (source.noCapsuleOnlyAtGenesis noCapsule).2⟩
  · rename_i capsule capsulePresent
    exact source.localGcWindow capsule capsulePresent

end InitialRecoverySource

/-- A finite fixed correct-validator set and one recovery source per member. -/
structure FixedCausalRecoverySources
    (BlockId CommitId : Type)
    (config : ValidatorEpochConfig CommitId) where
  validators : List Nat
  validatorsNodup : validators.Nodup
  validatorsNonempty : validators ≠ []
  source : (validator : Nat) →
    InitialRecoverySource (BlockId := BlockId) config validator

namespace FixedCausalRecoverySources

variable {BlockId CommitId : Type}
variable {config : ValidatorEpochConfig CommitId}

def sourceRound
    (sources : FixedCausalRecoverySources BlockId CommitId config)
    (validator : Nat) : Nat :=
  (sources.source validator).recoveryRound

def roundMaximum : List Nat → Nat
  | [] => 0
  | round :: rounds => Nat.max round (roundMaximum rounds)

def maximumRound
    (sources : FixedCausalRecoverySources BlockId CommitId config) : Nat :=
  roundMaximum (sources.validators.map sources.sourceRound)

theorem round_le_roundMaximum
    {round : Nat} {rounds : List Nat} (member : round ∈ rounds) :
    round ≤ roundMaximum rounds := by
  induction rounds with
  | nil => simp at member
  | cons head tail inductionHypothesis =>
      simp only [List.mem_cons] at member
      simp only [roundMaximum]
      rcases member with rfl | inTail
      · exact Nat.le_max_left _ _
      · exact Nat.le_trans (inductionHypothesis inTail)
          (Nat.le_max_right _ _)

theorem roundMaximum_mem
    {rounds : List Nat} (nonempty : rounds ≠ []) :
    roundMaximum rounds ∈ rounds := by
  induction rounds with
  | nil => contradiction
  | cons head tail inductionHypothesis =>
      cases tail with
      | nil => simp [roundMaximum]
      | cons next rest =>
          have tailNonempty : next :: rest ≠ [] := by simp
          have tailMember := inductionHypothesis tailNonempty
          change Nat.max head (roundMaximum (next :: rest)) ∈
            head :: next :: rest
          by_cases tailLeHead : roundMaximum (next :: rest) ≤ head
          · simpa only [Nat.max_eq_left tailLeHead] using
              (List.mem_cons_self : head ∈ head :: next :: rest)
          · have headLeTail : head ≤ roundMaximum (next :: rest) :=
              Nat.le_of_lt (Nat.lt_of_not_ge tailLeHead)
            simpa only [Nat.max_eq_right headLeTail] using
              (List.mem_cons_of_mem head tailMember)

theorem member_round_le_maximum
    (sources : FixedCausalRecoverySources BlockId CommitId config)
    {validator : Nat} (member : validator ∈ sources.validators) :
    sources.sourceRound validator ≤ sources.maximumRound := by
  apply round_le_roundMaximum
  exact List.mem_map.mpr ⟨validator, member, rfl⟩

theorem maximum_round_has_owner
    (sources : FixedCausalRecoverySources BlockId CommitId config) :
    ∃ owner ∈ sources.validators,
      sources.sourceRound owner = sources.maximumRound := by
  have mappedNonempty : sources.validators.map sources.sourceRound ≠ [] := by
    simpa using sources.validatorsNonempty
  have maximumInMap : roundMaximum
      (sources.validators.map sources.sourceRound) ∈
      sources.validators.map sources.sourceRound :=
    roundMaximum_mem mappedNonempty
  unfold maximumRound
  rw [List.mem_map] at maximumInMap
  obtain ⟨owner, ownerMember, ownerRound⟩ := maximumInMap
  exact ⟨owner, ownerMember, ownerRound⟩

/-- The maximum recovery-source round is positive because each source round is
positive and the finite source set has an owner of the maximum. -/
theorem maximum_round_positive
    (sources : FixedCausalRecoverySources BlockId CommitId config) :
    0 < sources.maximumRound := by
  obtain ⟨owner, ownerMember, ownerMaximum⟩ :=
    sources.maximum_round_has_owner
  rw [← ownerMaximum]
  exact (sources.source owner).recovery_round_positive

end FixedCausalRecoverySources

end Mysticeti
