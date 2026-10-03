/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Std

namespace Mysticeti

/-!
Exact commit-reference construction.

The encoded record has the same fields as the Rust `CommitV1` body. Selected
leader skip decisions are not in this record. The ordered committed leaders are
builder input because Rust uses their order to make the block-sort seed. They
are not encoded as a separate commit field.

The digest operation is abstract. The agreement proof needs only deterministic
encoding and hashing. A larger safety proof must state collision resistance when
it uses a digest as a unique name for earlier commit contents.
-/

/-- An exact commit reference contains the index and digest used by commit votes. -/
structure ExactCommitReference (Digest : Type) where
  index : Nat
  digest : Digest

/-- The local data that Rust calculates from the ordered committed leaders and
the stored DAG before it makes a commit body. -/
structure ExactCommitBuildMaterial (BlockId : Type) where
  timestamp : Nat
  namedLeader : BlockId
  sortedCommittedBlocks : List BlockId

/-- Extra local data that Rust uses before it constructs a `CommitV1` body.

`orderedCommittedLeaders` determines the sort seed. `namedLeader` is the last
leader after the final committed-block sort. `sortedCommittedBlocks` is the
complete newly committed causal closure above the GC boundary.
-/
structure ExactCommitBuilderInput (Digest BlockId : Type) where
  prior : ExactCommitReference Digest
  index : Nat
  timestamp : Nat
  orderedCommittedLeaders : List BlockId
  namedLeader : BlockId
  sortedCommittedBlocks : List BlockId

/-- The exact data serialized by Rust `CommitV1`.

The field order follows the Rust structure: index, previous digest, timestamp,
named leader, and the fully sorted committed block references.
-/
structure ExactCommitRecord (Digest BlockId : Type) where
  index : Nat
  previousDigest : Digest
  timestamp : Nat
  leader : BlockId
  blocks : List BlockId

namespace ExactCommitBuilderInput

/-- Remove the builder-only leader list and make the exact serialized body. -/
def toCommitRecord {Digest BlockId : Type}
    (input : ExactCommitBuilderInput Digest BlockId) :
    ExactCommitRecord Digest BlockId :=
  { index := input.index
    previousDigest := input.prior.digest
    timestamp := input.timestamp
    leader := input.namedLeader
    blocks := input.sortedCommittedBlocks }

/-- A valid local builder uses the next index after its prior reference. -/
def UsesNextIndex {Digest BlockId : Type}
    (input : ExactCommitBuilderInput Digest BlockId) : Prop :=
  input.index = input.prior.index + 1

end ExactCommitBuilderInput

/-- Abstract deterministic operations used by Rust to construct a commit digest.

`encode` models BCS serialization of the `Commit::V1` enum and its `CommitV1`
body. The enum tag is therefore a fixed part of this function. `hash` models the
commit digest. The commit reference is not signed as a separate object. Commit
votes are later carried inside signed blocks.
-/
structure CommitReferenceFunctions
    (Digest BlockId Encoding : Type) where
  encode : ExactCommitRecord Digest BlockId → Encoding
  hash : Encoding → Digest

/-- Build an exact reference from the body that Rust serializes and hashes. -/
def constructExactCommitReference
    {Digest BlockId Encoding : Type}
    (functions : CommitReferenceFunctions Digest BlockId Encoding)
    (record : ExactCommitRecord Digest BlockId) :
    ExactCommitReference Digest :=
  { index := record.index
    digest := functions.hash (functions.encode record) }

/-- Build an exact reference from the complete local builder input. -/
def constructExactCommitReferenceFromInput
    {Digest BlockId Encoding : Type}
    (functions : CommitReferenceFunctions Digest BlockId Encoding)
    (input : ExactCommitBuilderInput Digest BlockId) :
    ExactCommitReference Digest :=
  constructExactCommitReference functions input.toCommitRecord

/-- Equal exact Rust commit bodies produce equal commit references. -/
theorem exact_commit_reference_builder_congruence
    {Digest BlockId Encoding : Type}
    (functions : CommitReferenceFunctions Digest BlockId Encoding)
    {left right : ExactCommitRecord Digest BlockId}
    (sameRecord : left = right) :
    constructExactCommitReference functions left =
      constructExactCommitReference functions right := by
  rw [sameRecord]

/-- Equal complete local builder inputs produce equal commit references. -/
theorem exact_commit_reference_input_congruence
    {Digest BlockId Encoding : Type}
    (functions : CommitReferenceFunctions Digest BlockId Encoding)
    {left right : ExactCommitBuilderInput Digest BlockId}
    (sameInput : left = right) :
    constructExactCommitReferenceFromInput functions left =
      constructExactCommitReferenceFromInput functions right := by
  rw [sameInput]

/-- A valid builder input produces the next commit index. -/
theorem constructed_reference_uses_next_index
    {Digest BlockId Encoding : Type}
    (functions : CommitReferenceFunctions Digest BlockId Encoding)
    {input : ExactCommitBuilderInput Digest BlockId}
    (next : input.UsesNextIndex) :
    (constructExactCommitReferenceFromInput functions input).index =
      input.prior.index + 1 := by
  exact next

end Mysticeti
