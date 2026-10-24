/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.Safety
import Mysticeti.ExactCommitPrefixSafety

namespace Mysticeti

/-! Full safety for Mysticeti v3, collected in one place.

Safety has two parts, and the specification proves both. They were stated in
separate modules, so this file names them together.

**Part one, per-slot exclusion.** `mysticeti_v3_safety` in `Safety` proves that
one selected leader slot cannot hold a commit result and a skip result at the
same time, and that one transaction cannot be accepted and rejected. Quorum
intersection at the checked thresholds gives this. An equivocating author counts
once on each side and never twice on one side.

**Part two, same-index commit agreement.** Two correct, available validators
that have installed a commit at the same index have installed the same commit.
`correct_validators_agree_on_commit_at_index` below is that result. The commit
head carries the index, the commit identifier, and the leader round, so equality
of heads is equality of the digest.

Part two is what a reader usually means by safety of the commit sequence. It is
not weaker than agreement on a sequence: the installed head at each index is
unique, so the installed prefixes agree entry by entry.

## What part two assumes

* `AuthenticatedFlexVoteSourceMap`. Authentication, non-equivocation, and the
  static fault bound. This derives the pairwise quorum-evidence adapter that the
  exact FlexCommitter theorem consumes, so the quorum-intersection content of
  part one is inside this input.
* `ExactCommitInstallProvenance`. Install provenance. Every installed head has
  an exact commit path from genesis, and a verified chain entry matches the
  durable prefix. Each field is a one-host storage or hash-chain rule. No field
  mentions two validators, and no field states agreement.
* Both validators are in range and are correct and available under the fixed
  fault model.

The proof is short because the work sits in those inputs. Each installed head
has an exact commit path from genesis. `ExactFlexSuccessor.unique` proves the
successor relation is functional, so two paths of the same length from one
genesis end at the same head.

`ASM-SAFE-COMMIT-STORE` and `ASM-SAFE-INSTALL-PROVENANCE` record what remains:
the one-host source refinement that instantiates these inputs from the running
Rust. `ASM-SAFE-DIGEST-IDENTITY` supplies the digest binding they rest on.
-/

namespace ExactCommitInstallProvenance

variable {BlockId CommitId History Encoding PacketId : Type}
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

/-- Full safety, part two.

Two correct, available validators that have installed a commit at one index have
installed the same commit. The head carries the index, the commit identifier,
and the leader round, so this is exact agreement on the digest.

This is `exactInstalledHeadsAtSameIndexAgree` under the name a reader looks
for. -/
theorem correct_validators_agree_on_commit_at_index
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {leftTime leftValidator rightTime rightValidator : Nat}
    {left right : ValidatorCommitHead CommitId}
    (leftValidatorInRange : leftValidator < config.authorityCount)
    (leftValidatorCorrect : faults.correctAvailable leftValidator = true)
    (rightValidatorInRange : rightValidator < config.authorityCount)
    (rightValidatorCorrect : faults.correctAvailable rightValidator = true)
    (leftInstalled : durable.exactInstalledHead leftTime leftValidator left)
    (rightInstalled : durable.exactInstalledHead rightTime rightValidator right)
    (sameIndex : left.index = right.index) :
    left = right :=
  provenance.exactInstalledHeadsAtSameIndexAgree authenticated
    leftValidatorInRange leftValidatorCorrect rightValidatorInRange
    rightValidatorCorrect leftInstalled rightInstalled sameIndex

end ExactCommitInstallProvenance

end Mysticeti
