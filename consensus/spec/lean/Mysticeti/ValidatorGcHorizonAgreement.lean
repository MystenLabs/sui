/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ExactCommitPrefixSafety

namespace Mysticeti

/-! Agreement of durable commit prefixes across different block-GC horizons.

Block garbage collection can remove different DAG bodies at different
validators. It does not change an installed commit entry. These theorems compare
absolute durable commit indexes and do not require equal block-GC rounds.
-/

namespace ExactCommitInstallProvenance

variable {BlockId CommitId History Encoding PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {functions : CommitReferenceFunctions
  CommitId (LeaderBlockRef BlockId) Encoding}
variable {context : ValidatorFlexContextAt BlockId CommitId History}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) -> Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {source : LocalFlexCommitterSourceMap config functions context program}
variable {runtime : LocalFlexCommitterRuntime timed source}
variable {genesis : ValidatorCommitHead CommitId}
variable {durable : ExactCommitDurablePrefixSourceMap faults
  timed.execution.trace genesis}
variable {validChain : Nat -> List (CommonCommitRef CommitId) -> Prop}
variable {validBlocks : CommitSyncBundle BlockId CommitId -> Prop}

/-- Two correct validators that cover `cut` have the same exact durable entry
at every absolute commit index through `cut`. -/
theorem exactInstalledPrefixesAgreeThrough
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {leftTime leftValidator rightTime rightValidator cut : Nat}
    (leftValidatorInRange : leftValidator < config.authorityCount)
    (leftValidatorCorrect : faults.correctAvailable leftValidator = true)
    (rightValidatorInRange : rightValidator < config.authorityCount)
    (rightValidatorCorrect : faults.correctAvailable rightValidator = true)
    (leftCovers :
      cut <= (timed.execution.trace leftTime).localCommitIndex leftValidator)
    (rightCovers :
      cut <= (timed.execution.trace rightTime).localCommitIndex rightValidator) :
    forall index, index <= cut ->
      exists head : ValidatorCommitHead CommitId,
        head.index = index /\
        durable.exactInstalledHead leftTime leftValidator head /\
        durable.exactInstalledHead rightTime rightValidator head /\
        ((timed.execution.trace leftTime).validatorState leftValidator).installedCommitAt
            index = some head.id /\
        ((timed.execution.trace rightTime).validatorState rightValidator).installedCommitAt
            index = some head.id := by
  intro index indexInPrefix
  have leftIndexCovered :
      index <= (timed.execution.trace leftTime).localCommitIndex leftValidator :=
    Nat.le_trans indexInPrefix leftCovers
  have rightIndexCovered :
      index <=
        (timed.execution.trace rightTime).localCommitIndex rightValidator :=
    Nat.le_trans indexInPrefix rightCovers
  rcases durable.installedAtOrBelowHead leftTime leftValidator index
      leftValidatorInRange leftValidatorCorrect leftIndexCovered with
    ⟨leftId, leftStored⟩
  rcases durable.installedAtOrBelowHead rightTime rightValidator index
      rightValidatorInRange rightValidatorCorrect rightIndexCovered with
    ⟨rightId, rightStored⟩
  rcases durable.storedIdHasExactHead leftTime leftValidator index leftId
      leftValidatorInRange leftValidatorCorrect leftStored with
    ⟨leftHead, leftExact, leftIndex, leftIdBinding⟩
  rcases durable.storedIdHasExactHead rightTime rightValidator index rightId
      rightValidatorInRange rightValidatorCorrect rightStored with
    ⟨rightHead, rightExact, rightIndex, rightIdBinding⟩
  have sameHead : leftHead = rightHead :=
    provenance.exactInstalledHeadsAtSameIndexAgree authenticated
      leftValidatorInRange leftValidatorCorrect rightValidatorInRange
      rightValidatorCorrect leftExact rightExact
      (leftIndex.trans rightIndex.symm)
  subst rightHead
  rw [<- leftIdBinding] at leftStored
  rw [<- rightIdBinding] at rightStored
  exact ⟨leftHead, leftIndex, leftExact, rightExact, leftStored, rightStored⟩

/-- The prefix result does not depend on the two local block-GC rounds. The GC
facts only identify the horizons of the compared states. -/
theorem exactInstalledPrefixesAgreeAcrossGcHorizons
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {leftTime leftValidator rightTime rightValidator cut : Nat}
    {leftGcRound rightGcRound : Nat}
    (leftValidatorInRange : leftValidator < config.authorityCount)
    (leftValidatorCorrect : faults.correctAvailable leftValidator = true)
    (rightValidatorInRange : rightValidator < config.authorityCount)
    (rightValidatorCorrect : faults.correctAvailable rightValidator = true)
    (_leftGcHorizon :
      ((timed.execution.trace leftTime).validatorState leftValidator).gcRound =
        leftGcRound)
    (_rightGcHorizon :
      ((timed.execution.trace rightTime).validatorState rightValidator).gcRound =
        rightGcRound)
    (leftCovers :
      cut <= (timed.execution.trace leftTime).localCommitIndex leftValidator)
    (rightCovers :
      cut <= (timed.execution.trace rightTime).localCommitIndex rightValidator) :
    forall index, index <= cut ->
      exists head : ValidatorCommitHead CommitId,
        head.index = index /\
        durable.exactInstalledHead leftTime leftValidator head /\
        durable.exactInstalledHead rightTime rightValidator head /\
        ((timed.execution.trace leftTime).validatorState leftValidator).installedCommitAt
            index = some head.id /\
        ((timed.execution.trace rightTime).validatorState rightValidator).installedCommitAt
            index = some head.id := by
  exact provenance.exactInstalledPrefixesAgreeThrough authenticated
    leftValidatorInRange leftValidatorCorrect rightValidatorInRange
    rightValidatorCorrect leftCovers rightCovers

end ExactCommitInstallProvenance

end Mysticeti
