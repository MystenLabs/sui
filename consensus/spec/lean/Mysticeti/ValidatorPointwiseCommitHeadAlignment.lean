/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.CommonCommitStep

namespace Mysticeti

/-!
Start-head alignment for one pointwise common commit step.

The result is local to the start state. It does not use future progress, GST,
or epoch activity. An installed prior reference gives a lower bound on the
current commit index. If the current index is not ahead, the local exact-prefix
rule identifies the complete current head with the prior reference.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- One correct, available validator is ahead of the installed prior reference,
or its complete current head is that reference. -/
theorem correct_available_installed_prior_is_ahead_or_exact_head
    {trace : Trace (ValidatorWorldState BlockId CommitId PacketId)}
    (prefixMap : ValidatorCommitPrefixSourceMap faults trace)
    {start : Time} {prior : ValidatorCommitHead CommitId}
    (priorInstalled : AllCorrectAvailableInstalledExactAt faults trace start
      prior)
    {validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrect : faults.correctAvailable validator = true) :
    prior.index + 1 ≤ (trace start).localCommitIndex validator ∨
      ((trace start).validatorState validator).commitHead = prior := by
  have installed := priorInstalled validator validatorInRange validatorCorrect
  by_cases ahead :
      prior.index + 1 ≤ (trace start).localCommitIndex validator
  · exact Or.inl ahead
  · have sameIndex :
        (trace start).localCommitIndex validator = prior.index := by
      omega
    exact Or.inr (prefixMap.sameIndexInstalledHeadIsExact start validator prior
      validatorInRange validatorCorrect installed.1 sameIndex)

/-- At one start state, some correct, available validator is ahead, or all such
validators have the exact prior head. -/
theorem all_correct_available_installed_prior_gives_collective_head_split
    {trace : Trace (ValidatorWorldState BlockId CommitId PacketId)}
    (prefixMap : ValidatorCommitPrefixSourceMap faults trace)
    {start : Time} {prior : ValidatorCommitHead CommitId}
    (priorInstalled : AllCorrectAvailableInstalledExactAt faults trace start
      prior) :
    (∃ validator,
      validator < config.authorityCount ∧
        faults.correctAvailable validator = true ∧
        prior.index + 1 ≤ (trace start).localCommitIndex validator) ∨
      AllCorrectAvailableCommitHeadsEqual faults trace start prior := by
  classical
  by_cases ahead : ∃ validator,
      validator < config.authorityCount ∧
        faults.correctAvailable validator = true ∧
        prior.index + 1 ≤ (trace start).localCommitIndex validator
  · exact Or.inl ahead
  · apply Or.inr
    intro validator validatorInRange validatorCorrect
    rcases correct_available_installed_prior_is_ahead_or_exact_head prefixMap
        priorInstalled validatorInRange validatorCorrect with
      validatorAhead | exactHead
    · exact False.elim (ahead ⟨validator, validatorInRange,
          validatorCorrect, validatorAhead⟩)
    · exact exactHead

/-- The collective split at the start of one derived pointwise step.

The pointwise-step proposition is not needed to prove the split. It records the
future completion that uses this start state. -/
theorem derived_pointwise_common_commit_step_start_head_split
    {trace : Trace (ValidatorWorldState BlockId CommitId PacketId)}
    (_step : DerivedPointwiseCommonCommitStep faults network trace)
    (prefixMap : ValidatorCommitPrefixSourceMap faults trace)
    {start : Time} {prior : ValidatorCommitHead CommitId}
    (priorInstalled : AllCorrectAvailableInstalledExactAt faults trace start
      prior) :
    (∃ validator,
      validator < config.authorityCount ∧
        faults.correctAvailable validator = true ∧
        prior.index + 1 ≤ (trace start).localCommitIndex validator) ∨
      AllCorrectAvailableCommitHeadsEqual faults trace start prior := by
  exact all_correct_available_installed_prior_gives_collective_head_split
    prefixMap priorInstalled

end Mysticeti
