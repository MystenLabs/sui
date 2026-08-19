/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorCausalReadQuality

namespace Mysticeti

/-! Refinement of the Rust v3 commit materializer.

With `enable_v3`, `Core::try_commit_v3` builds each commit in
`FlexCommitter::build_commit`. That function starts from every committed leader
of the commit round, marks each one, and walks their causal history with an
explicit stack. It follows an ancestor reference only when the reference is
above the local GC round and is not already marked committed, and it marks each
followed reference before it pushes the body. `Linearizer::linearize_sub_dag`
runs the same shape for the pre-v3 path from a single leader.

This module models that loop and proves the conditional inclusion theorem: a
successful run materializes exactly the blocks that the eligible-ancestor
relation reaches from the committed leaders. "Conditional" has two parts. The
local view must hold the body of each eligible ancestor, which is the Rust
`get_block(..).unwrap()` inside the walk. The loop must also finish, which the
Rust finite store supplies.

The model uses a front stack instead of the Rust back stack. Both keep the same
last-in-first-out discipline and reach the same set. Rust sorts the result
before it builds the commit body, so the visit order of sibling ancestors does
not change the committed set that the sort receives.

Two modeling limits stay open in `REF-COMMIT-MATERIALIZER-WALK`. The loop takes
a step budget instead of deriving its bound from the finite store and the
committed marks. Rust also drops a repeated ancestor reference through
`if !set_committed(..) { continue; }`; the model relies on the verifier rule
that rejects two ancestors from one author, which `ancestorIdsNodup` records.
-/

/-- The local `DagState` view that one materializer run reads.

`committedBefore` is the set of block identifiers that `is_committed` already
accepts when the run starts. `catalog` is `get_blocks` on one identifier. -/
structure CommitMaterializerView (BlockId : Type) where
  gcRound : Nat
  committedBefore : List BlockId
  catalog : BlockId → Option (ValidatorBlock BlockId)
  /-- Rust looks up a full `BlockRef`. The identifier therefore names one exact
  reference, in the same shape that the rest of the model uses for
  `blockCatalog`. Read across two references that share an identifier, this
  field is a collision-freedom condition on the block digest. -/
  catalogNamesBlock : ∀ reference block, catalog reference.id = some block →
    block.reference = reference
  /-- The complete `block.ancestors()` list that the Rust walk follows.

  `ValidatorBlock.parents` is only the immediate-parent projection used for the
  quorum check. Block verification rejects an ancestor at or above the block
  round and requires quorum stake at exactly one round below, but it permits an
  older ancestor. The walk therefore needs the wider list. -/
  ancestorsOf : ValidatorBlock BlockId → List (ValidatorBlockRef BlockId)
  parentsAreAncestors : ∀ block reference, reference ∈ block.parents →
    reference ∈ ancestorsOf block
  /-- Block verification rejects two ancestors from one author, so the followed
  references are distinct. -/
  ancestorIdsNodup : ∀ block,
    ((ancestorsOf block).map ValidatorBlockRef.id).Nodup

namespace CommitMaterializerView

/-- The Rust ancestor filter: above the local GC round and not already
committed when the run starts. -/
def EligibleAncestor {BlockId : Type}
    (view : CommitMaterializerView BlockId)
    (reference : ValidatorBlockRef BlockId) : Prop :=
  view.gcRound < reference.round ∧ reference.id ∉ view.committedBefore

/-- Blocks that the eligible-ancestor walk reaches from one leader body.

This relation does not use the loop or its fuel. It is the specification of the
newly committed causal closure. -/
inductive Materialized {BlockId : Type}
    (view : CommitMaterializerView BlockId)
    (leaders : List (ValidatorBlock BlockId)) :
    ValidatorBlock BlockId → Prop where
  | leader {block : ValidatorBlock BlockId} :
      block ∈ leaders → Materialized view leaders block
  | ancestor {child parent : ValidatorBlock BlockId}
      {reference : ValidatorBlockRef BlockId} :
      Materialized view leaders child →
      reference ∈ view.ancestorsOf child →
      view.gcRound < reference.round →
      reference.id ∉ view.committedBefore →
      view.catalog reference.id = some parent →
      Materialized view leaders parent

/-! ### Facts that do not use the loop -/

section Relation

variable {BlockId : Type} {view : CommitMaterializerView BlockId}

/-- Plain inclusion form: a present eligible ancestor body of a materialized
block is itself materialized. This is the Rust walk invariant. -/
theorem materialized_contains_eligible_ancestor
    {leaders : List (ValidatorBlock BlockId)}
    {child ancestor : ValidatorBlock BlockId}
    {reference : ValidatorBlockRef BlockId}
    (childMaterialized : view.Materialized leaders child)
    (referenceMember : reference ∈ view.ancestorsOf child)
    (eligible : view.EligibleAncestor reference)
    (body : view.catalog reference.id = some ancestor) :
    view.Materialized leaders ancestor :=
  .ancestor childMaterialized referenceMember eligible.1 eligible.2 body

/-- Rust asserts that no committed block is at or below the GC round. Every
materialized block satisfies that assertion. -/
theorem materialized_above_gc {leaders : List (ValidatorBlock BlockId)}
    {block : ValidatorBlock BlockId}
    (leadersAboveGc : ∀ leader, leader ∈ leaders →
      view.gcRound < leader.reference.round)
    (materialized : view.Materialized leaders block) :
    view.gcRound < block.reference.round := by
  induction materialized with
  | leader member => exact leadersAboveGc _ member
  | @ancestor _ parent reference _ _ aboveGc _ body _ =>
      rw [view.catalogNamesBlock reference parent body]
      exact aboveGc

/-- Two local views that agree on the GC round, the earlier committed marks,
the walk edges, and the catalogued bodies reach the same blocks. -/
theorem materialized_congr {left right : CommitMaterializerView BlockId}
    (sameGcRound : left.gcRound = right.gcRound)
    (sameCommittedBefore : ∀ id,
      id ∈ left.committedBefore ↔ id ∈ right.committedBefore)
    (sameAncestors : ∀ block, left.ancestorsOf block = right.ancestorsOf block)
    (sameCatalog : ∀ id, left.catalog id = right.catalog id)
    {leaders : List (ValidatorBlock BlockId)} {block : ValidatorBlock BlockId}
    (materialized : left.Materialized leaders block) :
    right.Materialized leaders block := by
  induction materialized with
  | leader member => exact .leader member
  | @ancestor child _ reference _ referenceMember aboveGc fresh body
      childMaterialized =>
      exact .ancestor childMaterialized (sameAncestors child ▸ referenceMember)
        (sameGcRound ▸ aboveGc)
        (fun marked => fresh ((sameCommittedBefore reference.id).mpr marked))
        (sameCatalog reference.id ▸ body)

end Relation

/-! ### The executable Rust loop -/

/-- The references that one loop step follows from one popped block. -/
def eligibleAncestorRefs {BlockId : Type} [DecidableEq BlockId]
    (view : CommitMaterializerView BlockId) (committed : List BlockId)
    (block : ValidatorBlock BlockId) : List (ValidatorBlockRef BlockId) :=
  (view.ancestorsOf block).filter fun reference =>
    decide (view.gcRound < reference.round) && decide (reference.id ∉ committed)

/-- The bodies that one loop step pushes. A missing body is dropped here. The
inclusion theorem takes the present body as its condition, so the Rust `expect`
stays an explicit source obligation. -/
def eligibleAncestors {BlockId : Type} [DecidableEq BlockId]
    (view : CommitMaterializerView BlockId) (committed : List BlockId)
    (block : ValidatorBlock BlockId) : List (ValidatorBlock BlockId) :=
  (view.eligibleAncestorRefs committed block).filterMap fun reference =>
    view.catalog reference.id

/-- The identifiers that one loop step marks committed. -/
def eligibleAncestorIds {BlockId : Type} [DecidableEq BlockId]
    (view : CommitMaterializerView BlockId) (committed : List BlockId)
    (block : ValidatorBlock BlockId) : List BlockId :=
  (view.eligibleAncestors committed block).map fun ancestor =>
    ancestor.reference.id

/-- The Rust stack loop. `fuel` bounds the number of iterations; the Rust loop
uses the finite store and the committed marks instead. The result pairs the
visited bodies with the final committed marks. -/
def linearizeFrom {BlockId : Type} [DecidableEq BlockId]
    (view : CommitMaterializerView BlockId) :
    Nat → List (ValidatorBlock BlockId) → List BlockId →
      Option (List (ValidatorBlock BlockId) × List BlockId)
  | 0, _, _ => none
  | _ + 1, [], committed => some ([], committed)
  | fuel + 1, block :: buffer, committed =>
      (view.linearizeFrom fuel
          (view.eligibleAncestors committed block ++ buffer)
          (view.eligibleAncestorIds committed block ++ committed)).map
        fun result => (block :: result.1, result.2)

/-- One complete materializer run for the committed leaders of one round.

`FlexCommitter::build_commit` marks every committed leader, then walks from all
of them. The pre-v3 `Linearizer` path is the one-leader case. -/
def buildCommit {BlockId : Type} [DecidableEq BlockId]
    (view : CommitMaterializerView BlockId) (fuel : Nat)
    (leaders : List (ValidatorBlock BlockId)) :
    Option (List (ValidatorBlock BlockId) × List BlockId) :=
  view.linearizeFrom fuel leaders
    (leaders.map (fun leader => leader.reference.id) ++ view.committedBefore)


/-! ### Reading one loop step -/

variable {BlockId : Type} [DecidableEq BlockId]
variable {view : CommitMaterializerView BlockId}

@[simp]
theorem mem_eligibleAncestors {committed : List BlockId}
    {block ancestor : ValidatorBlock BlockId} :
    ancestor ∈ view.eligibleAncestors committed block ↔
      ∃ reference, reference ∈ view.ancestorsOf block ∧
        view.gcRound < reference.round ∧ reference.id ∉ committed ∧
          view.catalog reference.id = some ancestor := by
  simp [eligibleAncestors, eligibleAncestorRefs, List.mem_filterMap,
    List.mem_filter, and_assoc]

theorem mem_eligibleAncestorIds {committed : List BlockId}
    {block : ValidatorBlock BlockId} {id : BlockId} :
    id ∈ view.eligibleAncestorIds committed block ↔
      ∃ ancestor, ancestor ∈ view.eligibleAncestors committed block ∧
        ancestor.reference.id = id := by
  simp [eligibleAncestorIds, List.mem_map, eq_comm]

/-- Each followed ancestor body is above the local GC round and was not
committed before this step. -/
theorem eligibleAncestor_facts {committed : List BlockId}
    {block ancestor : ValidatorBlock BlockId}
    (member : ancestor ∈ view.eligibleAncestors committed block) :
    view.gcRound < ancestor.reference.round ∧
      ancestor.reference.id ∉ committed := by
  rcases mem_eligibleAncestors.mp member with
    ⟨parent, _, aboveGc, notCommitted, body⟩
  rw [view.catalogNamesBlock parent ancestor body]
  exact ⟨aboveGc, notCommitted⟩

/-- A step follows only edges that the eligible-ancestor relation allows, as
long as the running marks contain the marks held before the run. -/
theorem eligibleAncestor_extends_materialized {committed : List BlockId}
    {leaders : List (ValidatorBlock BlockId)} {block ancestor : ValidatorBlock BlockId}
    (start : ∀ id, id ∈ view.committedBefore → id ∈ committed)
    (blockMaterialized : view.Materialized leaders block)
    (member : ancestor ∈ view.eligibleAncestors committed block) :
    view.Materialized leaders ancestor := by
  rcases mem_eligibleAncestors.mp member with
    ⟨reference, referenceMember, aboveGc, notCommitted, body⟩
  exact .ancestor blockMaterialized referenceMember aboveGc
    (fun before => notCommitted (start reference.id before)) body

/-! ### Reading one loop iteration -/

/-- A run that pops one block returns that block first and continues from the
extended stack and marks. -/
theorem linearizeFrom_cons {fuel : Nat} {block : ValidatorBlock BlockId}
    {buffer out : List (ValidatorBlock BlockId)}
    {committed finalCommitted : List BlockId}
    (run : view.linearizeFrom (fuel + 1) (block :: buffer) committed =
      some (out, finalCommitted)) :
    ∃ rest, out = block :: rest ∧
      view.linearizeFrom fuel
          (view.eligibleAncestors committed block ++ buffer)
          (view.eligibleAncestorIds committed block ++ committed) =
        some (rest, finalCommitted) := by
  simp only [linearizeFrom, Option.map_eq_some_iff] at run
  rcases run with ⟨⟨rest, restCommitted⟩, step, result⟩
  simp only [Prod.mk.injEq] at result
  exact ⟨rest, result.1.symm, result.2 ▸ step⟩

/-- A run on the empty stack returns no block and keeps its marks. -/
theorem linearizeFrom_nil {fuel : Nat} {out : List (ValidatorBlock BlockId)}
    {committed finalCommitted : List BlockId}
    (run : view.linearizeFrom (fuel + 1) [] committed =
      some (out, finalCommitted)) :
    out = [] ∧ finalCommitted = committed := by
  simp only [linearizeFrom, Option.some.injEq, Prod.mk.injEq] at run
  exact ⟨run.1.symm, run.2.symm⟩

/-- The whole run is driven by the stack, so an exhausted fuel gives no
result. -/
theorem linearizeFrom_zero {buffer : List (ValidatorBlock BlockId)}
    {committed : List BlockId} :
    view.linearizeFrom 0 buffer committed = none := rfl

/-! ### Loop invariants

Each result below is proved by induction on the fuel with the stack and the
running marks generalized. Together they give the conditional inclusion theorem
for one successful run.
-/

/-- Every block on the stack reaches the output. -/
theorem linearizeFrom_drains_buffer :
    ∀ (fuel : Nat) (buffer : List (ValidatorBlock BlockId))
      (committed : List BlockId) (out : List (ValidatorBlock BlockId))
      (finalCommitted : List BlockId),
      view.linearizeFrom fuel buffer committed = some (out, finalCommitted) →
      ∀ block, block ∈ buffer → block ∈ out := by
  intro fuel
  induction fuel with
  | zero => intro _ _ _ _ run; rw [linearizeFrom_zero] at run; exact absurd run (by simp)
  | succ fuel ih =>
      intro buffer committed out finalCommitted run
      cases buffer with
      | nil => intro block member; exact absurd member (by simp)
      | cons block tail =>
          obtain ⟨rest, outShape, step⟩ := linearizeFrom_cons run
          subst outShape
          intro other member
          rcases List.mem_cons.mp member with rfl | tailMember
          · exact List.mem_cons_self
          · exact List.mem_cons_of_mem _
              (ih _ _ _ _ step other (List.mem_append_right _ tailMember))

/-- Conditional inclusion for one run: an ancestor reference that is above the
GC round, is not already marked, and has a local body is materialized. -/
theorem linearizeFrom_includes_eligible_ancestor :
    ∀ (fuel : Nat) (buffer : List (ValidatorBlock BlockId))
      (committed : List BlockId) (out : List (ValidatorBlock BlockId))
      (finalCommitted : List BlockId),
      view.linearizeFrom fuel buffer committed = some (out, finalCommitted) →
      ∀ block, block ∈ out →
        ∀ reference, reference ∈ view.ancestorsOf block →
          view.gcRound < reference.round →
          reference.id ∉ committed →
          ∀ ancestor, view.catalog reference.id = some ancestor →
            ancestor ∈ out := by
  intro fuel
  induction fuel with
  | zero => intro _ _ _ _ run; rw [linearizeFrom_zero] at run; exact absurd run (by simp)
  | succ fuel ih =>
      intro buffer committed out finalCommitted run
      cases buffer with
      | nil =>
          obtain ⟨outShape, _⟩ := linearizeFrom_nil run
          subst outShape
          intro block member
          exact absurd member (by simp)
      | cons block tail =>
          obtain ⟨rest, outShape, step⟩ := linearizeFrom_cons run
          subst outShape
          intro other otherMember reference referenceMember aboveGc notMarked
            ancestor body
          have ancestorEligible :
              ancestor ∈ view.eligibleAncestors committed block →
                ancestor ∈ block :: rest := fun member =>
            List.mem_cons_of_mem _
              (linearizeFrom_drains_buffer _ _ _ _ _ step ancestor
                (List.mem_append_left _ member))
          rcases List.mem_cons.mp otherMember with rfl | restMember
          · exact ancestorEligible
              (mem_eligibleAncestors.mpr
                ⟨reference, referenceMember, aboveGc, notMarked, body⟩)
          · by_cases stepMarked :
                reference.id ∈ view.eligibleAncestorIds committed block
            · rcases mem_eligibleAncestorIds.mp stepMarked with
                ⟨earlier, earlierMember, earlierId⟩
              rcases mem_eligibleAncestors.mp earlierMember with
                ⟨parent, _, _, _, earlierBody⟩
              rw [view.catalogNamesBlock parent earlier earlierBody] at earlierId
              rw [earlierId] at earlierBody
              rw [body] at earlierBody
              exact ancestorEligible (Option.some.inj earlierBody ▸ earlierMember)
            · exact List.mem_cons_of_mem _
                (ih _ _ _ _ step other restMember reference referenceMember
                  aboveGc
                  (fun member =>
                    (List.mem_append.mp member).elim stepMarked notMarked)
                  ancestor body)

/-- A run follows only eligible-ancestor edges, so every output block is
materialized. -/
theorem linearizeFrom_output_materialized
    {leaders : List (ValidatorBlock BlockId)} :
    ∀ (fuel : Nat) (buffer : List (ValidatorBlock BlockId))
      (committed : List BlockId) (out : List (ValidatorBlock BlockId))
      (finalCommitted : List BlockId),
      view.linearizeFrom fuel buffer committed = some (out, finalCommitted) →
      (∀ id, id ∈ view.committedBefore → id ∈ committed) →
      (∀ block, block ∈ buffer → view.Materialized leaders block) →
      ∀ block, block ∈ out → view.Materialized leaders block := by
  intro fuel
  induction fuel with
  | zero => intro _ _ _ _ run; rw [linearizeFrom_zero] at run; exact absurd run (by simp)
  | succ fuel ih =>
      intro buffer committed out finalCommitted run start bufferMaterialized
      cases buffer with
      | nil =>
          obtain ⟨outShape, _⟩ := linearizeFrom_nil run
          subst outShape
          intro block member
          exact absurd member (by simp)
      | cons block tail =>
          obtain ⟨rest, outShape, step⟩ := linearizeFrom_cons run
          subst outShape
          have blockMaterialized := bufferMaterialized block List.mem_cons_self
          intro other member
          rcases List.mem_cons.mp member with rfl | restMember
          · exact blockMaterialized
          · refine ih _ _ _ _ step
              (fun id before => List.mem_append_right _ (start id before))
              (fun candidate candidateMember => ?_) other restMember
            rcases List.mem_append.mp candidateMember with ancestor | old
            · exact eligibleAncestor_extends_materialized start blockMaterialized
                ancestor
            · exact bufferMaterialized candidate (List.mem_cons_of_mem _ old)

/-! ### The materializer inclusion theorem -/

/-- A successful Rust materializer run returns exactly the blocks that the
eligible-ancestor relation reaches from the committed leaders.

The conditions are the ones the Rust loop already relies on. Each leader body is
in the local view, no leader is already committed, and the run finishes. Every
followed edge needs its body in the same local view, which is the
`get_block(..).unwrap()` obligation inside the walk. -/
theorem buildCommit_materializes_exactly {fuel : Nat}
    {leaders out : List (ValidatorBlock BlockId)} {finalCommitted : List BlockId}
    (run : view.buildCommit fuel leaders = some (out, finalCommitted))
    (leadersCatalogued : ∀ leader, leader ∈ leaders →
      view.catalog leader.reference.id = some leader) :
    ∀ block, block ∈ out ↔ view.Materialized leaders block := by
  have leadersMember : ∀ leader, leader ∈ leaders → leader ∈ out := fun leader
    member => linearizeFrom_drains_buffer _ _ _ _ _ run leader member
  intro block
  constructor
  · exact fun member =>
      linearizeFrom_output_materialized _ _ _ _ _ run
        (fun _ before => List.mem_append_right _ before)
        (fun candidate candidateMember => .leader candidateMember)
        block member
  · intro materialized
    induction materialized with
    | leader member => exact leadersMember _ member
    | @ancestor child parent reference _ referenceMember aboveGc fresh body
        childMember =>
        by_cases sameAsLeader :
            ∃ leader, leader ∈ leaders ∧ leader.reference.id = reference.id
        · rcases sameAsLeader with ⟨leader, leaderMember, leaderId⟩
          rw [← leaderId, leadersCatalogued leader leaderMember] at body
          exact Option.some.inj body ▸ leadersMember leader leaderMember
        · refine linearizeFrom_includes_eligible_ancestor _ _ _ _ _ run child
            childMember reference referenceMember aboveGc (fun marked => ?_)
            parent body
          rcases List.mem_append.mp marked with leaderMark | before
          · rcases List.mem_map.mp leaderMark with ⟨leader, leaderMember, leaderId⟩
            exact sameAsLeader ⟨leader, leaderMember, leaderId⟩
          · exact fresh before

/-! ### Duplicate-free output

Rust asserts `set_committed` on the leaders and drops a repeated ancestor
reference with `continue`. Block verification also rejects two ancestors from
one author. The results below reproduce the resulting duplicate-free commit
vector, which the deterministic sort needs.
-/

omit [DecidableEq BlockId] in
/-- Distinct followed references give distinct bodies. -/
theorem filterMap_catalog_ids_nodup :
    ∀ references : List (ValidatorBlockRef BlockId),
      (references.map ValidatorBlockRef.id).Nodup →
      (((references.filterMap fun reference => view.catalog reference.id)).map
        fun block => block.reference.id).Nodup := by
  intro references
  induction references with
  | nil => intro _; simp
  | cons reference tail ih =>
      intro nodup
      simp only [List.map_cons, List.nodup_cons] at nodup
      cases body : view.catalog reference.id with
      | none => simpa [List.filterMap_cons, body] using ih nodup.2
      | some block =>
          have named := view.catalogNamesBlock reference block body
          simp only [List.filterMap_cons, body, List.map_cons, List.nodup_cons]
          refine ⟨fun clash => ?_, ih nodup.2⟩
          rcases List.mem_map.mp clash with ⟨other, otherMember, otherId⟩
          rcases List.mem_filterMap.mp otherMember with
            ⟨otherReference, otherReferenceMember, otherBody⟩
          rw [view.catalogNamesBlock otherReference other otherBody] at otherId
          rw [named] at otherId
          exact nodup.1 (otherId ▸ List.mem_map_of_mem otherReferenceMember)

theorem eligibleAncestors_ids_nodup {committed : List BlockId}
    {block : ValidatorBlock BlockId} :
    ((view.eligibleAncestors committed block).map
      fun ancestor => ancestor.reference.id).Nodup :=
  filterMap_catalog_ids_nodup _
    (List.Nodup.sublist (List.Sublist.map _ List.filter_sublist)
      (view.ancestorIdsNodup block))

/-- An output block was already on the stack, or its mark is new. -/
theorem linearizeFrom_output_is_fresh :
    ∀ (fuel : Nat) (buffer : List (ValidatorBlock BlockId))
      (committed : List BlockId) (out : List (ValidatorBlock BlockId))
      (finalCommitted : List BlockId),
      view.linearizeFrom fuel buffer committed = some (out, finalCommitted) →
      ∀ block, block ∈ out →
        block.reference.id ∈ buffer.map (fun other => other.reference.id) ∨
          block.reference.id ∉ committed := by
  intro fuel
  induction fuel with
  | zero => intro _ _ _ _ run; rw [linearizeFrom_zero] at run; exact absurd run (by simp)
  | succ fuel ih =>
      intro buffer committed out finalCommitted run
      cases buffer with
      | nil =>
          obtain ⟨outShape, _⟩ := linearizeFrom_nil run
          subst outShape
          intro block member
          exact absurd member (by simp)
      | cons block tail =>
          obtain ⟨rest, outShape, step⟩ := linearizeFrom_cons run
          subst outShape
          intro other member
          rcases List.mem_cons.mp member with rfl | restMember
          · exact Or.inl List.mem_cons_self
          · rcases ih _ _ _ _ step other restMember with stepBuffer | stepFresh
            · rcases List.mem_map.mp stepBuffer with
                ⟨candidate, candidateMember, candidateId⟩
              rcases List.mem_append.mp candidateMember with ancestor | old
              · exact Or.inr (candidateId ▸
                  (eligibleAncestor_facts ancestor).2)
              · exact Or.inl (candidateId ▸
                  List.mem_cons_of_mem _ (List.mem_map_of_mem old))
            · exact Or.inr fun marked =>
                stepFresh (List.mem_append_right _ marked)

/-- No block is committed twice in one run. -/
theorem linearizeFrom_output_nodup :
    ∀ (fuel : Nat) (buffer : List (ValidatorBlock BlockId))
      (committed : List BlockId) (out : List (ValidatorBlock BlockId))
      (finalCommitted : List BlockId),
      view.linearizeFrom fuel buffer committed = some (out, finalCommitted) →
      (buffer.map fun block => block.reference.id).Nodup →
      (∀ block, block ∈ buffer → block.reference.id ∈ committed) →
      (out.map fun block => block.reference.id).Nodup := by
  intro fuel
  induction fuel with
  | zero => intro _ _ _ _ run; rw [linearizeFrom_zero] at run; exact absurd run (by simp)
  | succ fuel ih =>
      intro buffer committed out finalCommitted run bufferNodup bufferMarked
      cases buffer with
      | nil =>
          obtain ⟨outShape, _⟩ := linearizeFrom_nil run
          subst outShape
          simp
      | cons block tail =>
          obtain ⟨rest, outShape, step⟩ := linearizeFrom_cons run
          subst outShape
          simp only [List.map_cons, List.nodup_cons] at bufferNodup
          have blockMarked := bufferMarked block List.mem_cons_self
          have ancestorsFresh : ∀ ancestor,
              ancestor ∈ view.eligibleAncestors committed block →
                ancestor.reference.id ∉ committed := fun _ member =>
            (eligibleAncestor_facts member).2
          have stepNodup :
              ((view.eligibleAncestors committed block ++ tail).map
                fun other => other.reference.id).Nodup := by
            rw [List.map_append]
            refine List.nodup_append.mpr
              ⟨eligibleAncestors_ids_nodup, bufferNodup.2, ?_⟩
            intro ancestorId ancestorMemberId tailId tailMemberId sameId
            rcases List.mem_map.mp ancestorMemberId with
              ⟨ancestor, ancestorMember, ancestorEq⟩
            rcases List.mem_map.mp tailMemberId with
              ⟨other, otherMember, otherEq⟩
            exact ancestorsFresh ancestor ancestorMember
              (ancestorEq ▸ sameId ▸ otherEq ▸ bufferMarked other
                (List.mem_cons_of_mem _ otherMember))
          have stepMarked : ∀ candidate,
              candidate ∈ view.eligibleAncestors committed block ++ tail →
                candidate.reference.id ∈
                  view.eligibleAncestorIds committed block ++ committed := by
            intro candidate candidateMember
            rcases List.mem_append.mp candidateMember with ancestor | old
            · exact List.mem_append_left _
                (mem_eligibleAncestorIds.mpr ⟨candidate, ancestor, rfl⟩)
            · exact List.mem_append_right _
                (bufferMarked candidate (List.mem_cons_of_mem _ old))
          refine List.nodup_cons.mpr ⟨fun clash => ?_,
            ih _ _ _ _ step stepNodup stepMarked⟩
          rcases List.mem_map.mp clash with ⟨other, otherMember, otherId⟩
          rcases linearizeFrom_output_is_fresh _ _ _ _ _ step other otherMember with
            stepBuffer | stepFresh
          · rw [otherId] at stepBuffer
            rw [List.map_append] at stepBuffer
            rcases List.mem_append.mp stepBuffer with ancestorId | tailId
            · rcases List.mem_map.mp ancestorId with
                ⟨ancestor, ancestorMember, ancestorEq⟩
              exact ancestorsFresh ancestor ancestorMember
                (ancestorEq ▸ blockMarked)
            · exact bufferNodup.1 tailId
          · exact stepFresh (otherId ▸ List.mem_append_right _ blockMarked)

/-- A complete run commits each block once. -/
theorem buildCommit_output_nodup {fuel : Nat}
    {leaders out : List (ValidatorBlock BlockId)} {finalCommitted : List BlockId}
    (run : view.buildCommit fuel leaders = some (out, finalCommitted))
    (leadersNodup : (leaders.map fun leader => leader.reference.id).Nodup) :
    (out.map fun block => block.reference.id).Nodup :=
  linearizeFrom_output_nodup _ _ _ _ _ run leadersNodup
    (fun _leader member => List.mem_append_left _ (List.mem_map_of_mem member))

/-! ### From one run to the exact commit body -/

/-- Two local views make the same walk reads. -/
structure ViewAgreement (left right : CommitMaterializerView BlockId) : Prop where
  gcRound : left.gcRound = right.gcRound
  committedBefore : ∀ id,
    id ∈ left.committedBefore ↔ id ∈ right.committedBefore
  ancestors : ∀ block, left.ancestorsOf block = right.ancestorsOf block
  catalog : ∀ id, left.catalog id = right.catalog id

namespace ViewAgreement

omit [DecidableEq BlockId] in
theorem symm {left right : CommitMaterializerView BlockId}
    (agreement : ViewAgreement left right) : ViewAgreement right left :=
  { gcRound := agreement.gcRound.symm
    committedBefore := fun id => (agreement.committedBefore id).symm
    ancestors := fun block => (agreement.ancestors block).symm
    catalog := fun id => (agreement.catalog id).symm }

end ViewAgreement

/-- Two successful runs over agreeing local views commit the same blocks. -/
theorem buildCommit_outputs_agree
    {left right : CommitMaterializerView BlockId}
    (agreement : ViewAgreement left right)
    {leftFuel rightFuel : Nat} {leaders : List (ValidatorBlock BlockId)}
    {leftOut rightOut : List (ValidatorBlock BlockId)}
    {leftMarks rightMarks : List BlockId}
    (leftRun : left.buildCommit leftFuel leaders = some (leftOut, leftMarks))
    (rightRun : right.buildCommit rightFuel leaders = some (rightOut, rightMarks))
    (leadersCatalogued : ∀ leader, leader ∈ leaders →
      left.catalog leader.reference.id = some leader) :
    ∀ block, block ∈ leftOut ↔ block ∈ rightOut := by
  have leftExact := buildCommit_materializes_exactly leftRun leadersCatalogued
  have rightExact := buildCommit_materializes_exactly rightRun
    (fun leader member =>
      agreement.catalog leader.reference.id ▸ leadersCatalogued leader member)
  intro block
  rw [leftExact block, rightExact block]
  exact ⟨materialized_congr agreement.gcRound agreement.committedBefore
      agreement.ancestors agreement.catalog,
    materialized_congr agreement.symm.gcRound agreement.symm.committedBefore
      agreement.symm.ancestors agreement.symm.catalog⟩

end CommitMaterializerView

/-- The deterministic committed-block sort that `build_commit` runs after the
walk.

V3 keys on the block round and on `hash(seed || digest)`, with a seed over the
ordered committed leaders. Distinct committed references therefore get distinct
keys unless that hash collides, so `determined` is discharged by the code for a
duplicate-free vector. The pre-v3 `sort_sub_dag_blocks` keys only on round and
author and has no such tie-break. -/
structure CommitBlockSort (BlockId : Type) where
  sort : List (ValidatorBlock BlockId) → List (ValidatorBlock BlockId) →
    List (ValidatorBlock BlockId)
  keepsBlocks : ∀ leaders blocks block,
    block ∈ sort leaders blocks ↔ block ∈ blocks
  keepsNodup : ∀ leaders blocks,
    (blocks.map fun block => block.reference.id).Nodup →
      ((sort leaders blocks).map fun block => block.reference.id).Nodup
  determined : ∀ leaders left right,
    (left.map fun block => block.reference.id).Nodup →
    (right.map fun block => block.reference.id).Nodup →
    (∀ block, block ∈ left ↔ block ∈ right) →
      sort leaders left = sort leaders right


end Mysticeti
