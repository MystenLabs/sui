/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Lean.Elab.Tactic.Omega
import Mysticeti.CommonCommitStep
import Mysticeti.ExactCommitPrefixSafety
import Mysticeti.ValidatorLocalReplayStore
import Mysticeti.ValidatorManifestReplay

namespace Mysticeti

/-!
Internal replay bridge for one already installed exact commit reference.

The source keeps exact material from an actual successful local committer run.
It sends an authenticated manifest that names the exact prior, next head, and
decision-DAG references. The receiver creates durable local needs, synchronizes
the blocks parent-first, runs the dedicated manifest-scoped committer action,
and records its exact result.

The rules below are one-host source mappings and durable work rules. They do
not assume a common chain, a ready server, a certificate, or future success.
-/

/-- Package one exact stored entry as pointwise completion evidence. -/
private theorem exact_installation_gives_completion_from_source
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {start finish validator : Nat}
    {reference : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (startBeforeFinish : start ≤ finish)
    (positiveIndex : 0 < reference.index)
    (installed :
      ((execution.trace finish).validatorState validator).installedCommitAt
        reference.index = some reference.id) :
    Nonempty (ValidatorExactCommitCompletion execution.trace start validator
      reference) := by
  have permitted :=
    (execution.statesWellFormed finish validator validatorInRange)
      |>.installedCommitHasPermittedSource reference.index reference.id
        positiveIndex installed
  rcases permitted with localSource | syncSource
  · exact ⟨{
      finish
      kind := .localCommit
      finishAfterStart := startBeforeFinish
      installedAtFinish := installed
      sourceAtFinish := Or.inl localSource
      kindSound := localSource }⟩
  · exact ⟨{
      finish
      kind := .verifiedCommitSync
      finishAfterStart := startBeforeFinish
      installedAtFinish := installed
      sourceAtFinish := Or.inr syncSource
      kindSound := syncSource }⟩

/-- One exact replay material value retained by its source validator. -/
structure RetainedLocalCommitReplay
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    (store : ValidatorLocalReplayStoreExecution timed source runtime)
    (holder : Nat) (time : Time) where
  material : ValidatorExactReplayMaterial functions context
  holderInRange : holder < config.authorityCount
  holderCorrect : faults.correctAvailable holder = true
  retained : (store.trace time holder).retained material = true

/-- Simple local rules that connect replay retention, manifest processing,
block sync, the manifest-scoped committer, and exact record work. -/
structure ValidatorLocalCommitReplayRules
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (source : LocalFlexCommitterSourceMap config functions context program)
    (runtime : LocalFlexCommitterRuntime timed source)
    (replayStore : ValidatorLocalReplayStoreExecution timed source runtime)
    (blockSync : ValidatorBlockSyncExecutionRules timed)
    (manifests : ValidatorReplayManifestExecution timed)
    (manifestSource : ValidatorManifestReplaySourceMap (History := History)
      functions timed)
    (manifestRuntime : ValidatorManifestReplayRuntime manifestSource) where
  /-- A retained source keeps each block pinned while replay can still need it. -/
  replayHistoryIsProtected : ∀ holder start
      (replay : RetainedLocalCommitReplay replayStore holder start),
    ∀ block, block ∈ replay.material.blocks → ∀ time,
      start ≤ time →
      (timed.execution.trace time).epochActive = true →
      blockSync.sourceProtected holder block.reference time
  /-- An installed retained source has durable work to send this exact manifest
  to one addressed correct requester. -/
  replayManifestSendIsProtected : ∀ holder start
      (replay : RetainedLocalCommitReplay replayStore holder start) requester,
    requester < config.authorityCount →
    ((timed.execution.trace start).validatorState holder).installedCommitAt
        replay.material.output.reference.index =
      some replay.material.output.reference.digest →
    (replay.material.toReplayManifest).head.index =
      (replay.material.toReplayManifest).prior.index + 1 →
    timed.protectedAction start holder
      (.sendReplayManifest requester replay.material.toReplayManifest)
  /-- Canonical roots are deterministic for this epoch and exact prior. -/
  genesisRoots : List (ValidatorBlockRef BlockId)
  rootsForPrior : ValidatorCommitHead CommitId →
    List (ValidatorBlockRef BlockId)
  /-- Retained source material can only name those deterministic roots. -/
  replayGenesisRootsExact : ∀ holder start
      (replay : RetainedLocalCommitReplay replayStore holder start),
    replay.material.genesisRoots = genesisRoots
  replayExternalRootsFromPrior : ∀ holder start
      (replay : RetainedLocalCommitReplay replayStore holder start),
    ∀ reference, reference ∈ replay.material.externalRoots →
      reference ∈ rootsForPrior replay.material.sourceInput.commitHead
  /-- Each host obtains canonical genesis roots from its epoch state. -/
  genesisRootsAreAccepted : ∀ requester time,
    requester < config.authorityCount →
    ∀ reference, reference ∈ genesisRoots →
      ((timed.execution.trace time).validatorState requester).accepted
          reference = true
  /-- Each host obtains the deterministic external roots from its installed
  exact prior. This is a one-host lookup. -/
  installedPriorProvidesReplayRoots : ∀ requester time prior,
    requester < config.authorityCount →
    ((timed.execution.trace time).validatorState requester).installedCommitAt
        prior.index = some prior.id →
    ∀ reference, reference ∈ rootsForPrior prior →
      ((timed.execution.trace time).validatorState requester).accepted
          reference = true
  /-- A durable requester-local manifest need keeps the exact block goal live. -/
  manifestNeedPreventsObsolescence : ∀ requester manifest reference time,
    (manifests.trace time requester).neededBlock manifest reference = true →
    ((timed.execution.trace time).validatorState requester).accepted
        reference = false →
    ¬blockSync.goalObsolete requester reference time
  /-- Once the accepted manifest and all of its exact blocks are local, the
  dedicated replay action is durable work. -/
  acceptedManifestProtectsReplay : ∀ holder start
      (replay : RetainedLocalCommitReplay replayStore holder start)
      requester time,
    requester < config.authorityCount →
    (manifests.trace time requester).acceptedManifest
        replay.material.toReplayManifest = true →
    ((timed.execution.trace time).validatorState requester).installedCommitAt
        replay.material.sourceInput.commitHead.index =
      some replay.material.sourceInput.commitHead.id →
    (∀ block, block ∈ replay.material.blocks →
      ((timed.execution.trace time).validatorState requester).accepted
        block.reference = true) →
    timed.protectedAction time requester
      (.runReplayCommitter replay.material.toReplayManifest)
  /-- The dedicated action reconstructs only the exact manifest material.
  Extra live DAG blocks and later rounds cannot enter this input. -/
  manifestReplayReconstructsMaterial : ∀ holder start
      (replay : RetainedLocalCommitReplay replayStore holder start)
      (observation : ValidatorManifestReplayObservation
        BlockId CommitId PacketId),
    observation.manifest = replay.material.toReplayManifest →
    observation.OccursIn timed →
    manifestSource.decisionDag observation.validator observation.input
        observation.manifest = replay.material.blocks ∧
      manifestSource.replayContext observation.validator observation.input
          observation.manifest =
        context replay.material.sourceValidator replay.material.sourceInput ∧
      manifestSource.snapshot observation.validator observation.input
          observation.manifest = replay.material.snapshot ∧
      ∀ block, block ∈ replay.material.blocks →
        (observation.input.validatorState observation.validator).accepted
              block.reference = true ∧
          observation.input.blockCatalog block.reference.id = some block

namespace ValidatorLocalCommitReplayRules

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
variable {replayStore : ValidatorLocalReplayStoreExecution timed source runtime}
variable {blockSync : ValidatorBlockSyncExecutionRules timed}
variable {manifests : ValidatorReplayManifestExecution timed}
variable {manifestSource : ValidatorManifestReplaySourceMap (History := History)
  functions timed}
variable {manifestRuntime : ValidatorManifestReplayRuntime manifestSource}

/-- One retained exact local replay reaches and installs at one lagging correct
validator. -/
theorem retained_replay_installs_at_lagging_validator
    (rules : ValidatorLocalCommitReplayRules timed source runtime replayStore
      blockSync manifests manifestSource manifestRuntime)
    {genesis : ValidatorCommitHead CommitId}
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (durable : ExactCommitDurablePrefixSourceMap faults timed.execution.trace
      genesis)
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {holder client : Nat}
    {reference : ValidatorCommitHead CommitId}
    {start referenceTime : Time} {referenceValidator : Nat}
    (replay : RetainedLocalCommitReplay replayStore holder start)
    (replayReference : replay.material.output.toCommitHead = reference)
    (sourceInstalled :
      ((timed.execution.trace start).validatorState holder).installedCommitAt
          replay.material.output.reference.index =
        some replay.material.output.reference.digest)
    (referenceValidatorInRange :
      referenceValidator < config.authorityCount)
    (referenceValidatorCorrect :
      faults.correctAvailable referenceValidator = true)
    (referenceInstalled :
      durable.exactInstalledHead referenceTime referenceValidator reference)
    (nextIndex : reference.index =
      replay.material.sourceInput.commitHead.index + 1)
    (clientInRange : client < config.authorityCount)
    (clientCorrectAvailable : faults.correctAvailable client = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (priorInstalled :
      ((timed.execution.trace start).validatorState client).installedCommitAt
          replay.material.sourceInput.commitHead.index =
        some replay.material.sourceInput.commitHead.id) :
    Nonempty (ValidatorExactCommitCompletion timed.execution.trace start client
      reference) := by
  let manifest := replay.material.toReplayManifest
  have manifestHead : manifest.head = reference := by
    simpa [manifest, ValidatorExactReplayMaterial.toReplayManifest] using
      replayReference
  have manifestNextIndex : manifest.head.index = manifest.prior.index + 1 := by
    rw [manifestHead]
    simpa [manifest, ValidatorExactReplayMaterial.toReplayManifest] using
      nextIndex
  have sourceHistory := replayStore.retainedMaterialHasHistory start
    holder replay.material replay.holderInRange replay.holderCorrect
      replay.retained
  have sendProtected := rules.replayManifestSendIsProtected holder start replay
    client clientInRange sourceInstalled manifestNextIndex
  rcases manifests.protected_manifest_send_creates_receiver_needs
      sourceHistory.holderInRange sourceHistory.holderCorrectAvailable
      clientInRange clientCorrectAvailable afterGst manifestNextIndex (by
        simpa [manifest, ValidatorExactReplayMaterial.toReplayManifest] using
          priorInstalled) sendProtected with
    ⟨manifestReadyAt, startBeforeManifestReady, manifestAccepted,
      manifestNeeds⟩
  have retainedAtManifestReady := retained_validator_block_history_persists
    blockSync sourceHistory startBeforeManifestReady (by
      intro block member time timeAfterStart _timeBeforeReady
      exact rules.replayHistoryIsProtected holder start replay block member time
        timeAfterStart (active time timeAfterStart))
  have priorAtManifestReady :=
    (timed.execution.durable_fields_persist clientInRange
      startBeforeManifestReady).installed_commit_persists priorInstalled
  have rootsAccepted : ∀ blockReference,
      (blockReference ∈ replay.material.genesisRoots ∨
        blockReference ∈ replay.material.externalRoots) →
      ((timed.execution.trace manifestReadyAt).validatorState client).accepted
          blockReference = true := by
    intro blockReference root
    rcases root with genesisRoot | externalRoot
    · apply rules.genesisRootsAreAccepted client manifestReadyAt clientInRange
        blockReference
      rw [← rules.replayGenesisRootsExact holder start replay]
      exact genesisRoot
    · apply rules.installedPriorProvidesReplayRoots client manifestReadyAt
        replay.material.sourceInput.commitHead clientInRange priorAtManifestReady
        blockReference
      exact rules.replayExternalRootsFromPrior holder start replay
        blockReference externalRoot
  have parentFirst : ParentFirstValidatorBlockHistory
      (fun blockReference =>
        ((timed.execution.trace manifestReadyAt).validatorState client).accepted
          blockReference = true)
      replay.material.blocks :=
    replay_parent_first_to_block_sync rootsAccepted
      replay.material.parentFirstFromRoots
  rcases retained_parent_first_history_eventually_accepted blockSync
      retainedAtManifestReady clientInRange clientCorrectAvailable
      (Nat.le_trans afterGst startBeforeManifestReady) (by
        intro time readyBeforeTime
        exact active time (Nat.le_trans startBeforeManifestReady readyBeforeTime))
      (by
        intro time readyBeforeTime incomplete block member
        exact rules.replayHistoryIsProtected holder start replay block member
          time (Nat.le_trans startBeforeManifestReady readyBeforeTime)
          (active time
            (Nat.le_trans startBeforeManifestReady readyBeforeTime)))
      (by
        intro block member time readyBeforeTime notAccepted
        have referenceInManifest : block.reference ∈ manifest.blockReferences :=
          List.mem_map.mpr ⟨block, member, rfl⟩
        have neededAtReady := manifestNeeds block.reference referenceInManifest
        have neededAtTime := manifests.needed_block_persists
          readyBeforeTime neededAtReady
        exact rules.manifestNeedPreventsObsolescence client manifest
          block.reference time neededAtTime notAccepted)
      parentFirst with
    ⟨historyReadyAt, manifestBeforeHistory, historyAccepted⟩
  have startBeforeReady : start ≤ historyReadyAt :=
    Nat.le_trans startBeforeManifestReady manifestBeforeHistory
  by_cases alreadyAtOrAbove : reference.index ≤
      (timed.execution.trace historyReadyAt).localCommitIndex client
  · have installed := provenance.exactHeadAtOrBelowLocalHeadIsStored
      authenticated referenceValidatorInRange referenceValidatorCorrect
      referenceInstalled clientInRange clientCorrectAvailable alreadyAtOrAbove
    exact exact_installation_gives_completion_from_source timed.execution
      clientInRange
      startBeforeReady (by rw [nextIndex]; omega) installed
  · have manifestAcceptedAtHistory := manifests.accepted_manifest_persists
      manifestBeforeHistory manifestAccepted
    have priorAtHistory :=
      (timed.execution.durable_fields_persist clientInRange
        manifestBeforeHistory).installed_commit_persists priorAtManifestReady
    have runProtected := rules.acceptedManifestProtectsReplay holder start replay
      client historyReadyAt clientInRange (by
        simpa [manifest] using manifestAcceptedAtHistory) priorAtHistory
        historyAccepted
    rcases protected_validator_action_completes_within_bound timed clientInRange
        clientCorrectAvailable runProtected with
      ⟨runCompletion, readyBeforeRun, _runBound, runOccurs⟩
    rcases manifestRuntime.action_occurrence_returns_exact_result runOccurs
        with
      ⟨observation, observationTime, observationValidator,
        observationManifest, returned, _exactReturned⟩
    have observationOccurs :=
      manifestRuntime.everyReturnedObservationOccurs observation returned
    have reconstructed := rules.manifestReplayReconstructsMaterial holder start
      replay observation (by simpa [manifest] using observationManifest)
      observationOccurs
    have exactReplayResult : observation.result = some replay.material.output := by
      calc
        observation.result =
            tryReferenceFlexCommitWithContext functions
              (manifestSource.replayContext observation.validator
                observation.input observation.manifest)
              (manifestSource.snapshot observation.validator
                observation.input observation.manifest) :=
          manifestRuntime.everyReturnedResultIsExact observation returned
        _ = tryReferenceFlexCommitWithContext functions
              (context replay.material.sourceValidator
                replay.material.sourceInput)
          replay.material.snapshot := by rw [reconstructed.2.1,
                reconstructed.2.2.1]
        _ = some replay.material.output := replay.material.outputFromSnapshot
    have recordProtected :=
      manifestRuntime.successfulResultLatchesRecord observation
        replay.material.output returned exactReplayResult
    rw [observationValidator] at recordProtected
    rcases protected_local_flex_record_completes_and_persists_exact timed
        clientInRange clientCorrectAvailable recordProtected with
      ⟨recordTime, recordLatchBefore, _recordBound, outputInstalled,
        outputSource⟩
    let finish := recordTime + 1
    have outputIndex := congrArg ValidatorCommitHead.index replayReference
    have outputId := congrArg ValidatorCommitHead.id replayReference
    simp only [LocalFlexCommitOutput.toCommitHead] at outputIndex outputId
    have referenceInstalledAtFinish :
        ((timed.execution.trace finish).validatorState client).installedCommitAt
            reference.index = some reference.id := by
      rw [← outputIndex, ← outputId]
      exact outputInstalled
    have referenceSourceAtFinish :
        ((timed.execution.trace finish).validatorState client).commitInstallSourceAt
            reference.index = some .localExecution := by
      rw [← outputIndex]
      exact outputSource
    have readyBeforeObservation : historyReadyAt ≤ observation.time := by
      simpa [observationTime] using readyBeforeRun
    have startBeforeFinish : start ≤ finish := by
      exact Nat.le_trans startBeforeReady
        (Nat.le_trans readyBeforeObservation
          (Nat.le_trans (Nat.le_add_right observation.time 1)
            (Nat.le_trans recordLatchBefore (Nat.le_succ recordTime))))
    exact ⟨{
      finish
      kind := .localCommit
      finishAfterStart := startBeforeFinish
      installedAtFinish := referenceInstalledAtFinish
      sourceAtFinish := Or.inl referenceSourceAtFinish
      kindSound := referenceSourceAtFinish }⟩

end ValidatorLocalCommitReplayRules

/-! ## Installed witness to derived propagation -/

/-- One actual installed witness, exact prefix safety, and the local replay
rules derive the internal known-next-reference propagation result. -/
theorem installed_reference_replay_proves_known_next_propagation
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    (source : LocalFlexCommitterSourceMap config functions context program)
    (flexCommitterRuntime : LocalFlexCommitterRuntime timed source)
    (replayStore : ValidatorLocalReplayStoreExecution timed source
      flexCommitterRuntime)
    (blockSync : ValidatorBlockSyncExecutionRules timed)
    (replayManifest : ValidatorReplayManifestExecution timed)
    (manifestSource : ValidatorManifestReplaySourceMap (History := History)
      functions timed)
    (manifestRuntime : ValidatorManifestReplayRuntime manifestSource)
    (rules : ValidatorLocalCommitReplayRules timed source
      flexCommitterRuntime replayStore blockSync replayManifest manifestSource
      manifestRuntime)
    {genesis : ValidatorCommitHead CommitId}
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (durable : ExactCommitDurablePrefixSourceMap faults timed.execution.trace
      genesis)
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance flexCommitterRuntime durable
      validChain validBlocks) :
    KnownNextReferencePropagation faults network timed.execution.trace := by
  intro start prior witnessValidator witnessId afterGst activeFromStart
    priorInstalled witnessInRange witnessCorrect witnessInstalled
  rcases provenance.exactHeadForStoredId witnessInRange witnessCorrect
      witnessInstalled with
    ⟨next, nextInstalledExact, nextIndex, nextId⟩
  have nextPositive : 0 < next.index := by
    rw [nextIndex]
    omega
  rcases provenance.exactOrigin witnessInRange witnessCorrect nextPositive
      nextInstalledExact with ⟨origin⟩
  have originPriorIndex : origin.run.prior.index = prior.index := by
    have relation := ExactCommitInstallProvenance.originPriorIndex origin
    omega
  have witnessPrior := priorInstalled witnessValidator witnessInRange
    witnessCorrect
  rcases provenance.exactHeadForStoredId witnessInRange witnessCorrect
      witnessPrior.1 with
    ⟨storedPrior, storedPriorExact, storedPriorIndex, storedPriorId⟩
  have storedPriorIsOriginPrior :=
    provenance.exactInstalledHeadsAtSameIndexAgree authenticated witnessInRange
      witnessCorrect origin.run.validatorInRange origin.run.validatorCorrect
      storedPriorExact origin.priorInstalled (by
        rw [storedPriorIndex, originPriorIndex])
  have originPriorId : origin.run.prior.id = prior.id := by
    calc
      origin.run.prior.id = storedPrior.id :=
        congrArg ValidatorCommitHead.id storedPriorIsOriginPrior.symm
      _ = prior.id := storedPriorId
  rcases replayStore.successful_run_retains_exact_replay
      origin.run.validatorInRange origin.run.validatorCorrect
      origin.run.returned origin.run.successful with
    ⟨material, retainedAfterRun, materialFromRun, _historyAfterRun⟩
  have materialPrior :
      material.sourceInput.commitHead = origin.run.prior := by
    rw [materialFromRun.sourceInput]
    exact origin.run.priorAtInput
  have replayReference : material.output.toCommitHead = next := by
    rw [materialFromRun.outputExact]
    exact origin.runOutput
  have retainedBeforeInstall : origin.run.observation.time + 1 ≤
      origin.installTime := by
    exact Nat.le_trans (Nat.succ_le_iff.mpr origin.runBeforeRecord)
      (Nat.le_trans (Nat.le_add_right origin.recordTime 1)
        origin.recordVisibleByInstall)
  let replayStart := Nat.max start origin.installTime
  have startBeforeReplay : start ≤ replayStart := Nat.le_max_left _ _
  have installBeforeReplay : origin.installTime ≤ replayStart :=
    Nat.le_max_right _ _
  have retainedAtReplay := replayStore.retained_replay_persists
    (Nat.le_trans retainedBeforeInstall installBeforeReplay) retainedAfterRun
  let replay : RetainedLocalCommitReplay replayStore
      origin.run.observation.validator replayStart :=
    { material
      holderInRange := origin.run.validatorInRange
      holderCorrect := origin.run.validatorCorrect
      retained := retainedAtReplay }
  have nextStoredAtOrigin := durable.exactHeadHasStoredId origin.installTime
    origin.run.observation.validator next origin.installed
  have nextStoredAtReplay :=
    (timed.execution.durable_fields_persist origin.run.validatorInRange
      installBeforeReplay).installed_commit_persists nextStoredAtOrigin
  have replaySourceInstalled :
      ((timed.execution.trace replayStart).validatorState
        origin.run.observation.validator).installedCommitAt
          material.output.reference.index =
        some material.output.reference.digest := by
    have outputIndex := congrArg ValidatorCommitHead.index replayReference
    have outputId := congrArg ValidatorCommitHead.id replayReference
    simp only [LocalFlexCommitOutput.toCommitHead] at outputIndex outputId
    rw [outputIndex, outputId]
    exact nextStoredAtReplay
  have materialNextIndex : next.index =
      material.sourceInput.commitHead.index + 1 := by
    rw [materialPrior]
    exact (ExactCommitInstallProvenance.originPriorIndex origin).symm
  refine ⟨next, nextIndex, nextId, ?_⟩
  intro validator validatorInRange validatorCorrect
  have priorAtStart := priorInstalled validator validatorInRange validatorCorrect
  have priorAtReplay :
      ((timed.execution.trace replayStart).validatorState validator).installedCommitAt
          material.sourceInput.commitHead.index =
        some material.sourceInput.commitHead.id := by
    have persisted :=
      (timed.execution.durable_fields_persist validatorInRange
        startBeforeReplay).installed_commit_persists priorAtStart.1
    rw [materialPrior, originPriorIndex, originPriorId]
    exact persisted
  by_cases behind :
      (timed.execution.trace replayStart).localCommitIndex validator < next.index
  · rcases ValidatorLocalCommitReplayRules.retained_replay_installs_at_lagging_validator
        rules durable authenticated provenance replay replayReference
        replaySourceInstalled witnessInRange witnessCorrect nextInstalledExact
        materialNextIndex validatorInRange validatorCorrect
        (Nat.le_trans afterGst startBeforeReplay) (by
          intro time replayBeforeTime
          exact activeFromStart time
            (Nat.le_trans startBeforeReplay replayBeforeTime)) priorAtReplay with
      ⟨completion⟩
    have startBeforeFinish := Nat.le_trans startBeforeReplay
      completion.finishAfterStart
    rcases completion.sourceAtFinish with localSource | syncSource
    · exact ⟨{
        finish := completion.finish
        kind := .localCommit
        finishAfterStart := startBeforeFinish
        installedAtFinish := completion.installedAtFinish
        sourceAtFinish := Or.inl localSource
        kindSound := localSource }⟩
    · exact ⟨{
        finish := completion.finish
        kind := .verifiedCommitSync
        finishAfterStart := startBeforeFinish
        installedAtFinish := completion.installedAtFinish
        sourceAtFinish := Or.inr syncSource
        kindSound := syncSource }⟩
  · have installed := provenance.exactHeadAtOrBelowLocalHeadIsStored
        authenticated witnessInRange witnessCorrect nextInstalledExact
        validatorInRange validatorCorrect (Nat.le_of_not_gt behind)
    exact exact_installation_gives_completion_from_source timed.execution
      validatorInRange startBeforeReplay nextPositive installed

end Mysticeti
