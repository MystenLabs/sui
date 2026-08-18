/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Lean.Elab.Tactic.Omega
import Mysticeti.ValidatorTimedExecutionLemmas

namespace Mysticeti

/-!
Commit-vote certification on the main validator execution.

This file starts after an earlier proof has shown that quorum correct stake
recorded one exact commit reference. It models the current block-carried vote
path:

1. A local commit record keeps one queued exact-reference vote.
2. A new persisted block carries the queued vote.
3. A protected send action creates one normal addressed block packet.
4. Delivery and local verification store one vote for the block author.
5. Quorum stored stake creates and retains one exact certified bundle.

The auxiliary evidence state is a ghost view of one host's commit evidence.
Its source map binds each used change to the same actions and packets as the
main `ValidatorExecution`. The final installed commit fields remain in the main
validator state.

This module proves certification and retention. Commit request, service, and
installation are the next propagation stage. It does not use a second abstract
network or assume that synchronization completed.

Missing Rust behavior:

* A fixed queued vote must reach a later block despite the per-block vote cap.
  Repeated FIFO drain, a reserved slot, or safe coalescing can give this rule.
* An uncarried queued vote must persist and recover after restart.
* A retained carrier that still needs dissemination must use block sync or a
  protected resend task. Rust does not repropose the old block.

Present Rust behavior still needs a source refinement proof:

* Local commit processing queues a vote, and proposal processing puts queued
  votes in a signed block.
* Verified received blocks add their commit votes to persistent storage.
* Stored commits, referenced blocks, and certifying blocks reconstruct the
  exact bundle checked by commit sync.

Certified commit sync is optional for the primary liveness proof. Exact local
DAG replay can install the same common reference without this optional path.
-/

abbrev CommonCommitRef (CommitId : Type) := ValidatorCommitHead CommitId

/-- One signed consensus block carries one author's exact commit vote. -/
structure CommitVoteCarrier (BlockId CommitId : Type) where
  block : ValidatorBlockRef BlockId
  author : Nat
  reference : CommonCommitRef CommitId

/-- Data returned by commit synchronization. -/
structure CommitSyncBundle (BlockId CommitId : Type) where
  commits : List (CommonCommitRef CommitId)
  blocks : List (ValidatorBlock BlockId)
  certifyingBlocks : List (CommitVoteCarrier BlockId CommitId)
  /-- This Boolean set counts each certifying author at most once. -/
  certifyingAuthors : VoterSet

/-- Adjacent commit entries increase the index by one. -/
def ConsecutiveCommitIndices {CommitId : Type} :
    List (CommonCommitRef CommitId) → Prop
  | [] => True
  | [_] => True
  | first :: second :: rest =>
      second.index = first.index + 1 ∧
        ConsecutiveCommitIndices (second :: rest)

/-- A returned bundle contains one exact certified chain. `validChain` models
the digest-link checks. `validBlocks` models block-reference and signature
checks. -/
def ExactCertifiedCommitBundle
    {BlockId CommitId : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake)
    (validChain : Nat → List (CommonCommitRef CommitId) → Prop)
    (validBlocks : CommitSyncBundle BlockId CommitId → Prop)
    (afterIndex : Nat) (reference : CommonCommitRef CommitId)
    (bundle : CommitSyncBundle BlockId CommitId) : Prop :=
  (bundle.commits.head?.map ValidatorCommitHead.index) =
      some (afterIndex + 1) ∧
    bundle.commits.getLast? = some reference ∧
    ConsecutiveCommitIndices bundle.commits ∧
    validChain afterIndex bundle.commits ∧
    validBlocks bundle ∧
    thresholds.quorum ≤
      weight authorityCount stake bundle.certifyingAuthors ∧
    ∀ author,
      author < authorityCount →
      bundle.certifyingAuthors author = true →
      ∃ carrier,
        carrier ∈ bundle.certifyingBlocks ∧
        carrier.author = author ∧
        carrier.block.author = author ∧
        carrier.reference = reference

/-! ## Auxiliary one-host evidence state -/

/-- Commit evidence owned by one validator.

The block body itself is in the main block catalog. `storedVote` records the
verified exact reference carried by one accepted signed block. `pendingVote`
can clear only after the invariant exposes a durable retained carrier.
-/
structure ValidatorCommitEvidenceLocalState (BlockId CommitId : Type) where
  recordedExact : CommonCommitRef CommitId → Bool
  /-- The live queue still has this vote obligation. A carrier can clear it. -/
  pendingVote : CommonCommitRef CommitId → Bool
  /-- One durable signed block has taken the vote from the live queue. -/
  retainedCarrier : ValidatorBlockRef BlockId →
    CommonCommitRef CommitId → Bool
  storedVote : Nat → CommonCommitRef CommitId → Bool
  retainedVerifiedBundle : CommitSyncBundle BlockId CommitId → Bool

abbrev ValidatorCommitEvidenceTrace (BlockId CommitId : Type) :=
  Time → Nat → ValidatorCommitEvidenceLocalState BlockId CommitId

/-- Durable commit evidence only gains entries. The live pending bit is not
part of this monotonicity rule. -/
def ValidatorCommitEvidenceDurable
    {BlockId CommitId : Type}
    (before after : ValidatorCommitEvidenceLocalState BlockId CommitId) : Prop :=
  BoolMapMonotone before.recordedExact after.recordedExact ∧
    (∀ block, BoolMapMonotone (before.retainedCarrier block)
      (after.retainedCarrier block)) ∧
    (∀ author, BoolMapMonotone (before.storedVote author)
      (after.storedVote author)) ∧
    BoolMapMonotone before.retainedVerifiedBundle
      after.retainedVerifiedBundle

namespace ValidatorCommitEvidenceDurable

variable {BlockId CommitId : Type}
variable {before after : ValidatorCommitEvidenceLocalState BlockId CommitId}

theorem recorded_exact_persists
    (durable : ValidatorCommitEvidenceDurable before after)
    {reference : CommonCommitRef CommitId}
    (recorded : before.recordedExact reference = true) :
    after.recordedExact reference = true :=
  durable.1 reference recorded

theorem retained_carrier_persists
    (durable : ValidatorCommitEvidenceDurable before after)
    {block : ValidatorBlockRef BlockId}
    {reference : CommonCommitRef CommitId}
    (retained : before.retainedCarrier block reference = true) :
    after.retainedCarrier block reference = true :=
  durable.2.1 block reference retained

theorem stored_vote_persists
    (durable : ValidatorCommitEvidenceDurable before after)
    {author : Nat} {reference : CommonCommitRef CommitId}
    (stored : before.storedVote author reference = true) :
    after.storedVote author reference = true :=
  durable.2.2.1 author reference stored

theorem retained_bundle_persists
    (durable : ValidatorCommitEvidenceDurable before after)
    {bundle : CommitSyncBundle BlockId CommitId}
    (retained : before.retainedVerifiedBundle bundle = true) :
    after.retainedVerifiedBundle bundle = true :=
  durable.2.2.2 bundle retained

end ValidatorCommitEvidenceDurable

/-- A recorded vote is still pending or has moved to one durable carrier. -/
def HasLocalVoteEvidence
    {BlockId CommitId : Type}
    (state : ValidatorCommitEvidenceLocalState BlockId CommitId)
    (reference : CommonCommitRef CommitId) : Prop :=
  state.pendingVote reference = true ∨
    ∃ block, state.retainedCarrier block reference = true

/-- Correct, available validators that recorded one exact reference. -/
def recordedExactVoters
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (evidence : ValidatorCommitEvidenceTrace BlockId CommitId)
    (reference : CommonCommitRef CommitId) (time : Time) : VoterSet :=
  VoterSet.inter faults.correctAvailable
    (fun author => (evidence time author).recordedExact reference)

/-- The prior exact-commit stage supplies this internal condition. -/
def HasRecordedExactQuorum
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (evidence : ValidatorCommitEvidenceTrace BlockId CommitId)
    (reference : CommonCommitRef CommitId) (time : Time) : Prop :=
  config.thresholds.quorum ≤
    weight config.authorityCount config.stake
      (recordedExactVoters faults evidence reference time)

/-- Per-validator exact records and the fixed fault bounds give recorded quorum
stake. The quorum fact is derived; it is not an additional network input. -/
theorem all_correct_available_records_give_recorded_quorum
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (evidence : ValidatorCommitEvidenceTrace BlockId CommitId)
    (reference : CommonCommitRef CommitId) (time : Time)
    (allRecorded : ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      (evidence time author).recordedExact reference = true) :
    HasRecordedExactQuorum faults evidence reference time := by
  apply Nat.le_trans faults.correct_available_stake_is_quorum
  apply weight_mono config.stake
  intro author authorInRange authorCorrect
  simp only [recordedExactVoters, VoterSet.inter]
  rw [authorCorrect, allRecorded author authorInRange authorCorrect]
  rfl

/-- Authors whose verified signed blocks carry one exact reference. -/
def exactReferenceVoters
    {BlockId CommitId : Type}
    (evidence : ValidatorCommitEvidenceTrace BlockId CommitId)
    (receiver : Nat) (reference : CommonCommitRef CommitId)
    (time : Time) : VoterSet :=
  fun author => (evidence time receiver).storedVote author reference

/-- A certificate counts unique authors for one exact index and identifier. -/
def HasExactReferenceCertificate
    {BlockId CommitId : Type}
    (evidence : ValidatorCommitEvidenceTrace BlockId CommitId)
    {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake)
    (receiver : Nat) (reference : CommonCommitRef CommitId)
    (time : Time) : Prop :=
  thresholds.quorum ≤
    weight authorityCount stake
      (exactReferenceVoters evidence receiver reference time)

/-! ## Main-trace production evidence -/

/-- The next persisted blocks after one ready time.

The block-production proof supplies this finite result. This structure does not
contain a block send or a network-delivery result.
-/
structure NextPersistedBlockProduction
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (readyAt : Time) where
  deadline : Time
  block : Nat → ValidatorBlock BlockId
  persistedAt : Nat → Time
  readyBeforeDeadline : readyAt ≤ deadline
  persistedAfterReady : ∀ author,
    author < config.authorityCount →
    faults.correctAvailable author = true →
    readyAt ≤ persistedAt author
  persistedByDeadline : ∀ author,
    author < config.authorityCount →
    faults.correctAvailable author = true →
    persistedAt author ≤ deadline
  persistenceOccurs : ∀ author,
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ValidatorLocalActionOccurs (execution.events (persistedAt author)) author
      (.persistProposal (block author))
  noEarlierPersistence : ∀ author,
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ∀ time earlierBlock,
      readyAt ≤ time →
      time < persistedAt author →
      ¬ValidatorLocalActionOccurs (execution.events time) author
        (.persistProposal earlierBlock)

/-- One retained signed carrier has a later protected send in the main
execution. -/
structure SentSignedVoteBlock
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (evidence : ValidatorCommitEvidenceTrace BlockId CommitId)
    (localActionBound productionDeadline : Nat)
    (stageStart : Time) (author receiver : Nat)
    (reference : CommonCommitRef CommitId)
    where
  carrier : ValidatorBlockRef BlockId
  carrierAt : Time
  sendCompletedAt : Time
  packetId : PacketId
  packet : AddressedPacket (ValidatorMessage BlockId CommitId)
  carrierRetained :
    (evidence carrierAt author).retainedCarrier carrier reference = true
  carrierAfterStart : stageStart ≤ carrierAt
  carrierByDeadline : carrierAt ≤ productionDeadline + 1
  sendAfterCarrier : carrierAt ≤ sendCompletedAt
  sendWithinBound : sendCompletedAt ≤
    carrierAt + localActionBound
  sendOccurs : ValidatorLocalActionOccurs
    (execution.events sendCompletedAt) author
      (.sendBlock receiver carrier)
  packetInTrace :
    (execution.trace packet.sentAt).packets packetId = some packet
  packetIsProtocol : protocolPacket packet
  packetSender : packet.sender = author
  packetReceiver : packet.receiver = receiver
  packetPayload : ∃ sentBlock,
    packet.payload = .block sentBlock ∧ sentBlock.reference = carrier
  packetSentAfterAction : packet.sentAt = sendCompletedAt + 1

/-- A newly produced signed carrier has one persistence event after the pending
vote and one later send event. Draining the queue transfers the obligation to
the retained carrier before the send can run. -/
structure ProducedSignedBlock
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (evidence : ValidatorCommitEvidenceTrace BlockId CommitId)
    (localActionBound productionDeadline : Nat)
    (author receiver : Nat) (reference : CommonCommitRef CommitId)
    (pendingAt : Time) where
  block : ValidatorBlock BlockId
  persistedAt : Time
  pending : (evidence pendingAt author).pendingVote reference = true
  persistenceAfterPending : pendingAt ≤ persistedAt
  persistenceByDeadline : persistedAt ≤ productionDeadline
  persistenceOccurs : ValidatorLocalActionOccurs
    (execution.events persistedAt) author (.persistProposal block)
  carrierStored :
    (evidence (persistedAt + 1) author).retainedCarrier block.reference
      reference = true
  sent : SentSignedVoteBlock execution evidence localActionBound
    productionDeadline pendingAt author receiver reference
  sentCarrier : sent.carrier = block.reference
  sentFromStoredTime : sent.carrierAt = persistedAt + 1

/-! ## One-host source map -/

/-- Source-to-model rules for queued votes, block carriers, stored votes, and
retained certified bundles.

Each rule is local to one host or one delivered addressed packet. No field
states that a quorum, certificate, server, or later installation exists.
-/
structure ValidatorCommitEvidenceSourceMap
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (validChain : Nat → List (CommonCommitRef CommitId) → Prop)
    (validBlocks : CommitSyncBundle BlockId CommitId → Prop) where
  evidence : ValidatorCommitEvidenceTrace BlockId CommitId
  durable : ∀ validator earlier later,
    earlier ≤ later →
    ValidatorCommitEvidenceDurable (evidence earlier validator)
      (evidence later validator)
  effects : ValidatorExactExecutionEffects faults protocolPacket network
    program timed.execution

  /-- The local record and its queued vote are created by the same completed
  main-trace commit action. -/
  recordCommitAddsEvidence : ∀ time author reference,
    ValidatorLocalActionOccurs (timed.execution.events time) author
      (.recordCommit reference) →
    (evidence (time + 1) author).recordedExact reference = true ∧
      (evidence (time + 1) author).pendingVote reference = true
  recordedExactIsLocalInstall : ∀ time author reference,
    (evidence time author).recordedExact reference = true →
    ((timed.execution.trace time).validatorState author).installedCommitAt
        reference.index = some reference.id ∧
      ((timed.execution.trace time).validatorState author).commitInstallSourceAt
        reference.index =
        some CommitInstallSource.localExecution
  /-- After restart, a durable local record has either its pending obligation
  or a durable signed carrier. -/
  recordedExactHasVoteEvidence : ∀ time author reference,
    (evidence time author).recordedExact reference = true →
    HasLocalVoteEvidence (evidence time author) reference

  blockCarriesVote : ValidatorBlockRef BlockId →
    CommonCommitRef CommitId → Prop
  /-- The first proposal persisted after the queue was visible includes that
  vote. Later blocks do not have to repeat a vote that the live queue drained.

  Rust needs a matching queue rule. For example, it can reserve one block slot
  for the latest pending commit vote. The current fixed-size FIFO drain does not
  give this rule when more older votes are pending than fit in one block.
  -/
  nextPersistedBlockTransfersPendingVote : ∀ pendingAt persistedAt author block
      reference,
    pendingAt ≤ persistedAt →
    (evidence pendingAt author).pendingVote reference = true →
    ValidatorLocalActionOccurs (timed.execution.events persistedAt) author
      (.persistProposal block) →
    (∀ time earlierBlock,
      pendingAt ≤ time →
      time < persistedAt →
      ¬ValidatorLocalActionOccurs (timed.execution.events time) author
        (.persistProposal earlierBlock)) →
    (evidence (persistedAt + 1) author).retainedCarrier block.reference
      reference = true
  retainedCarrierIsExact : ∀ time author carrier reference,
    (evidence time author).retainedCarrier carrier reference = true →
    blockCarriesVote carrier reference
  /-- Every retained carrier comes from an earlier proposal-persistence event
  by the same host. This prevents the ghost map from inventing carriers. -/
  retainedCarrierHasPersistenceOrigin : ∀ time author carrier reference,
    (evidence time author).retainedCarrier carrier reference = true →
    ∃ persistedAt block,
      persistedAt + 1 ≤ time ∧
      block.reference = carrier ∧
      ValidatorLocalActionOccurs (timed.execution.events persistedAt) author
        (.persistProposal block)
  /-- One retained own carrier latches an exact send to each correct, available
  receiver. This optional certification rule needs a Rust resend task or a
  refinement to the existing protected block-sync request and service path. -/
  retainedCarrierProtectsSend : ∀ carrierAt author receiver carrier reference,
    author < config.authorityCount →
    faults.correctAvailable author = true →
    receiver < config.authorityCount →
    faults.correctAvailable receiver = true →
    (evidence carrierAt author).retainedCarrier carrier reference = true →
    timed.protectedAction carrierAt author (.sendBlock receiver carrier)

  /-- Delivery of one verified block stores the exact carried vote under its
  authenticated block author. -/
  deliveredVoteBlockStores : ∀ packetId packet block reference,
    (timed.execution.trace packet.sentAt).packets packetId = some packet →
    packet.payload = .block block →
    blockCarriesVote block.reference reference →
    ValidatorPacketDeliveryOccurs
      (timed.execution.events packet.deliveredAt) packetId →
    (evidence (packet.deliveredAt + 1) packet.receiver).storedVote
      block.reference.author reference = true

  bundleFor : Nat → Nat → CommonCommitRef CommitId → Time →
    CommitSyncBundle BlockId CommitId
  /-- Local exact-reference verification constructs and retains the same bundle
  that the commit-sync verifier accepts. This is an optional commit-sync source
  rule. Rust stores its parts separately, so the source refinement must show
  that the complete bundle can be reconstructed from retained storage. -/
  certificateCreatesRetainedBundle : ∀ server afterIndex reference time,
    afterIndex < reference.index →
    ((timed.execution.trace time).validatorState server).installedCommitAt
        reference.index = some reference.id →
    HasExactReferenceCertificate evidence config.thresholds server reference
        time →
    let bundle := bundleFor server afterIndex reference time
    ExactCertifiedCommitBundle config.thresholds validChain validBlocks
        afterIndex reference bundle ∧
      (evidence time server).retainedVerifiedBundle bundle = true

namespace ValidatorCommitEvidenceSourceMap

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
variable {validBlocks : CommitSyncBundle BlockId CommitId → Prop}

/-- A retained signed carrier remains retained at each later time. -/
theorem retained_carrier_persists
    (source : ValidatorCommitEvidenceSourceMap timed validChain validBlocks)
    {validator earlier later : Nat} {carrier : ValidatorBlockRef BlockId}
    {reference : CommonCommitRef CommitId}
    (ordered : earlier ≤ later)
    (retained :
      (source.evidence earlier validator).retainedCarrier carrier reference =
        true) :
    (source.evidence later validator).retainedCarrier carrier reference = true :=
  (source.durable validator earlier later ordered).retained_carrier_persists
    retained

/-- A stored exact vote remains stored at each later time. -/
theorem stored_vote_persists
    (source : ValidatorCommitEvidenceSourceMap timed validChain validBlocks)
    {receiver author earlier later : Nat}
    {reference : CommonCommitRef CommitId}
    (ordered : earlier ≤ later)
    (stored :
      (source.evidence earlier receiver).storedVote author reference = true) :
    (source.evidence later receiver).storedVote author reference = true :=
  (source.durable receiver earlier later ordered).stored_vote_persists stored

/-- A retained verified bundle remains retained at each later time. -/
theorem retained_bundle_persists
    (source : ValidatorCommitEvidenceSourceMap timed validChain validBlocks)
    {holder earlier later : Nat} {bundle : CommitSyncBundle BlockId CommitId}
    (ordered : earlier ≤ later)
    (retained :
      (source.evidence earlier holder).retainedVerifiedBundle bundle = true) :
    (source.evidence later holder).retainedVerifiedBundle bundle = true :=
  (source.durable holder earlier later ordered).retained_bundle_persists retained

/-- All stored votes have arrived by this common time. -/
def certificateDeadline
    (nextBlocks : NextPersistedBlockProduction timed.execution readyAt) : Time :=
  nextBlocks.deadline + timed.localActionBound + network.delta + 3

/-- A retained carrier uses one later protected send. -/
private theorem complete_retained_vote_send
    (source : ValidatorCommitEvidenceSourceMap timed validChain validBlocks)
    {author receiver : Nat} {reference : CommonCommitRef CommitId}
    {stageStart carrierAt productionDeadline : Time}
    {carrier : ValidatorBlockRef BlockId}
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (retained :
      (source.evidence carrierAt author).retainedCarrier carrier reference = true)
    (carrierAfterStart : stageStart ≤ carrierAt)
    (carrierByDeadline : carrierAt ≤ productionDeadline + 1) :
    ∃ sent : SentSignedVoteBlock timed.execution source.evidence
      timed.localActionBound productionDeadline stageStart author receiver
        reference,
      sent.carrier = carrier ∧ sent.carrierAt = carrierAt := by
  have sendProtected := source.retainedCarrierProtectsSend carrierAt author
    receiver carrier reference authorInRange authorCorrect receiverInRange
    receiverCorrect retained
  let completion := timed.completeProtectedAction author
    (.sendBlock receiver carrier) carrierAt authorInRange authorCorrect
      sendProtected
  have sendAfterCarrier : carrierAt ≤ completion.event.completedAt := by
    simpa [completion.sameEnableTime] using completion.enableBeforeCompletion
  have sendWithinBound : completion.event.completedAt ≤
      carrierAt + timed.localActionBound := by
    simpa [completion.sameEnableTime] using completion.completesWithinBound
  rcases send_block_occurrence_creates_addressed_packet source.effects
      completion.occurs with
    ⟨packetId, sentBlock, packet, packetAtNextTime, sentReference,
      packetProtocol, packetSender, packetReceiver, packetPayload,
      packetSentAt⟩
  have packetAtSentTime :
      (timed.execution.trace packet.sentAt).packets packetId = some packet := by
    rw [packetSentAt]
    exact packetAtNextTime
  refine ⟨
    { carrier := carrier
      carrierAt := carrierAt
      sendCompletedAt := completion.event.completedAt
      packetId := packetId
      packet := packet
      carrierRetained := retained
      carrierAfterStart := carrierAfterStart
      carrierByDeadline := carrierByDeadline
      sendAfterCarrier := sendAfterCarrier
      sendWithinBound := sendWithinBound
      sendOccurs := completion.occurs
      packetInTrace := packetAtSentTime
      packetIsProtocol := packetProtocol
      packetSender := packetSender
      packetReceiver := packetReceiver
      packetPayload := ⟨sentBlock, packetPayload, sentReference⟩
      packetSentAfterAction := packetSentAt }, rfl, rfl⟩

/-- A pending vote uses a new persisted block and a later protected send. -/
private theorem complete_signed_vote_send
    (source : ValidatorCommitEvidenceSourceMap timed validChain validBlocks)
    {author receiver : Nat} {reference : CommonCommitRef CommitId}
    {readyAt : Time}
    (nextBlocks : NextPersistedBlockProduction timed.execution readyAt)
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (pending :
      (source.evidence readyAt author).pendingVote reference = true) :
    Nonempty (ProducedSignedBlock timed.execution source.evidence
      timed.localActionBound nextBlocks.deadline author receiver reference
        readyAt) := by
  let block := nextBlocks.block author
  let persistedAt := nextBlocks.persistedAt author
  have persistenceAfterPending :=
    nextBlocks.persistedAfterReady author authorInRange authorCorrect
  have persistence :=
    nextBlocks.persistenceOccurs author authorInRange authorCorrect
  have carrierStored := source.nextPersistedBlockTransfersPendingVote readyAt
    persistedAt author block reference persistenceAfterPending pending persistence
      (nextBlocks.noEarlierPersistence author authorInRange authorCorrect)
  have carrierByDeadline : persistedAt + 1 ≤ nextBlocks.deadline + 1 :=
    Nat.add_le_add_right
      (nextBlocks.persistedByDeadline author authorInRange authorCorrect) 1
  rcases complete_retained_vote_send source authorInRange authorCorrect
      receiverInRange receiverCorrect carrierStored
      (Nat.le_trans persistenceAfterPending (Nat.le_add_right persistedAt 1))
      carrierByDeadline with
    ⟨sent, sentCarrier, sentCarrierAt⟩
  exact ⟨
    { block := block
      persistedAt := persistedAt
      pending := pending
      persistenceAfterPending := persistenceAfterPending
      persistenceByDeadline :=
        nextBlocks.persistedByDeadline author authorInRange authorCorrect
      persistenceOccurs := persistence
      carrierStored := carrierStored
      sent := sent
      sentCarrier := sentCarrier
      sentFromStoredTime := sentCarrierAt }⟩

/-- One recorded validator's queued vote reaches one correct observer in a new
signed block. -/
private theorem observer_stores_recorded_vote
    (source : ValidatorCommitEvidenceSourceMap timed validChain validBlocks)
    {author receiver : Nat} {reference : CommonCommitRef CommitId}
    {start : Time}
    (nextBlocks : NextPersistedBlockProduction timed.execution start)
    (afterGst : network.gst ≤ start)
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true)
    (recorded :
      (source.evidence start author).recordedExact reference = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true) :
    (source.evidence (certificateDeadline nextBlocks) receiver).storedVote
      author reference = true := by
  have localEvidence :=
    source.recordedExactHasVoteEvidence start author reference recorded
  have sentExists : Nonempty (SentSignedVoteBlock timed.execution
      source.evidence timed.localActionBound nextBlocks.deadline start author
        receiver reference) := by
    rcases localEvidence with pending | ⟨carrier, retained⟩
    · rcases complete_signed_vote_send source nextBlocks authorInRange
          authorCorrect receiverInRange receiverCorrect pending with
        ⟨produced⟩
      exact ⟨produced.sent⟩
    · have carrierByDeadline : start ≤ nextBlocks.deadline + 1 :=
        Nat.le_trans nextBlocks.readyBeforeDeadline
          (Nat.le_add_right nextBlocks.deadline 1)
      rcases complete_retained_vote_send source authorInRange authorCorrect
          receiverInRange receiverCorrect retained (Nat.le_refl start)
          carrierByDeadline with
        ⟨sent, _, _⟩
      exact ⟨sent⟩
  rcases sentExists with ⟨sent⟩
  rcases sent.packetPayload with ⟨sentBlock, payload, sentReference⟩
  have packetSenderInRange : sent.packet.sender < config.authorityCount := by
    rw [sent.packetSender]
    exact authorInRange
  have packetReceiverInRange :
      sent.packet.receiver < config.authorityCount := by
    rw [sent.packetReceiver]
    exact receiverInRange
  have packetSenderCorrect :
      faults.correctAvailable sent.packet.sender = true := by
    rw [sent.packetSender]
    exact authorCorrect
  have packetReceiverCorrect :
      faults.correctAvailable sent.packet.receiver = true := by
    rw [sent.packetReceiver]
    exact receiverCorrect
  have packetAfterGst : network.gst ≤ sent.packet.sentAt := by
    calc
      network.gst ≤ start := afterGst
      _ ≤ sent.carrierAt := sent.carrierAfterStart
      _ ≤ sent.sendCompletedAt := sent.sendAfterCarrier
      _ ≤ sent.sendCompletedAt + 1 := Nat.le_add_right _ _
      _ = sent.packet.sentAt := sent.packetSentAfterAction.symm
  have deliveryBounds := network.postGstDelivery sent.packet
    sent.packetIsProtocol packetSenderInRange packetReceiverInRange
    packetSenderCorrect packetReceiverCorrect packetAfterGst
  have delivered := timed.execution.protocolPacketsAreDelivered
    sent.packetId sent.packet sent.packetInTrace
    sent.packetIsProtocol packetSenderInRange packetReceiverInRange
    packetSenderCorrect packetReceiverCorrect packetAfterGst
  have carriedBySentBlock :
      source.blockCarriesVote sentBlock.reference reference := by
    rw [sentReference]
    exact source.retainedCarrierIsExact sent.carrierAt author sent.carrier
      reference sent.carrierRetained
  have storedAtDelivery := source.deliveredVoteBlockStores sent.packetId
    sent.packet sentBlock reference sent.packetInTrace payload
    carriedBySentBlock delivered
  have carrierAuthor : sent.carrier.author = author := by
    rcases source.retainedCarrierHasPersistenceOrigin sent.carrierAt author
        sent.carrier reference sent.carrierRetained with
      ⟨persistedAt, carrierBlock, _, carrierReference, persistence⟩
    rcases validator_world_step_local_action_with_suffix
        (timed.execution.stepsFollowRules persistedAt) persistence with
      ⟨_, _, _, actionStep, _⟩
    have guard := validator_atomic_local_action_has_basic_guard actionStep
    rw [← carrierReference]
    exact guard.1
  have storedForAuthor :
      (source.evidence (sent.packet.deliveredAt + 1) receiver).storedVote
        author reference = true := by
    simpa [sent.packetReceiver, sentReference, carrierAuthor] using
      storedAtDelivery
  apply source.stored_vote_persists
    (earlier := sent.packet.deliveredAt + 1)
    (later := certificateDeadline nextBlocks)
    (receiver := receiver) (author := author) (reference := reference)
  · dsimp [certificateDeadline]
    have sendBound : sent.sendCompletedAt ≤
        sent.carrierAt + timed.localActionBound := sent.sendWithinBound
    have carrierBound : sent.carrierAt ≤ nextBlocks.deadline + 1 :=
      sent.carrierByDeadline
    calc
      sent.packet.deliveredAt + 1 ≤
          (sent.packet.sentAt + network.delta) + 1 :=
        Nat.add_le_add_right deliveryBounds.2 1
      _ = (sent.sendCompletedAt + 1 + network.delta) + 1 := by
        rw [sent.packetSentAfterAction]
      _ ≤
          (sent.carrierAt + timed.localActionBound + 1 +
            network.delta) + 1 := by
        exact Nat.add_le_add_right
          (Nat.add_le_add_right (Nat.add_le_add_right sendBound 1)
            network.delta) 1
      _ ≤
          (nextBlocks.deadline + 1 + timed.localActionBound + 1 +
            network.delta) + 1 := by
        exact Nat.add_le_add_right
          (Nat.add_le_add_right
            (Nat.add_le_add_right
              (Nat.add_le_add_right carrierBound timed.localActionBound)
              1)
            network.delta)
          1
      _ = nextBlocks.deadline + timed.localActionBound + network.delta + 3 := by
        simp only [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm]
  · exact storedForAuthor

/-- Signed vote blocks from the recorded quorum give one exact certificate at
each correct observer. -/
theorem observer_has_exact_certificate
    (source : ValidatorCommitEvidenceSourceMap timed validChain validBlocks)
    {receiver : Nat} {reference : CommonCommitRef CommitId} {start : Time}
    (nextBlocks : NextPersistedBlockProduction timed.execution start)
    (afterGst : network.gst ≤ start)
    (recordedQuorum :
      HasRecordedExactQuorum faults source.evidence reference start)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true) :
    HasExactReferenceCertificate source.evidence config.thresholds receiver
      reference (certificateDeadline nextBlocks) := by
  unfold HasExactReferenceCertificate
  apply Nat.le_trans recordedQuorum
  apply weight_mono config.stake
  intro author authorInRange isRecorder
  have membership :
      faults.correctAvailable author = true ∧
        (source.evidence start author).recordedExact reference = true := by
    simpa [recordedExactVoters, VoterSet.inter] using isRecorder
  exact observer_stores_recorded_vote source nextBlocks afterGst authorInRange
    membership.1 membership.2 receiverInRange receiverCorrect

/-- A recorded quorum produces one retained exact certified bundle on one
correct, available server.

This is the conditional boundary for commit-vote certification. The result is
ready for `RetainedCertifiedCommitBundle` in the known-reference propagation
stage. It does not assume a certificate, server, retained bundle, or completed
commit synchronization.
-/
theorem recorded_quorum_produces_retained_certificate
    (source : ValidatorCommitEvidenceSourceMap timed validChain validBlocks)
    {afterIndex : Nat} {reference : CommonCommitRef CommitId} {start : Time}
    (nextBlocks : NextPersistedBlockProduction timed.execution start)
    (afterGst : network.gst ≤ start)
    (recordedQuorum :
      HasRecordedExactQuorum faults source.evidence reference start)
    (afterIndexBeforeReference : afterIndex < reference.index) :
    ∃ server bundle,
      server < config.authorityCount ∧
      faults.correctAvailable server = true ∧
      ((timed.execution.trace (certificateDeadline nextBlocks)).validatorState
          server).installedCommitAt reference.index =
        some reference.id ∧
      HasExactReferenceCertificate source.evidence config.thresholds server
        reference (certificateDeadline nextBlocks) ∧
      ExactCertifiedCommitBundle config.thresholds validChain validBlocks
        afterIndex reference bundle ∧
      ValidatorCommitEvidenceLocalState.retainedVerifiedBundle
        (source.evidence (certificateDeadline nextBlocks) server) bundle =
          true := by
  let recorders := recordedExactVoters faults source.evidence reference start
  have recorderWeightPositive :
      0 < weight config.authorityCount config.stake recorders := by
    have quorumPositive := config.thresholds.quorum_positive
    have recordedQuorum' : config.thresholds.quorum ≤
        weight config.authorityCount config.stake recorders := by
      simpa [recorders, HasRecordedExactQuorum] using recordedQuorum
    omega
  rcases positive_weight_has_member recorderWeightPositive with
    ⟨server, serverInRange, inRecorders, _positiveStake⟩
  have recorderMembership :
      faults.correctAvailable server = true ∧
        (source.evidence start server).recordedExact reference = true := by
    simpa [recorders, recordedExactVoters, VoterSet.inter] using inRecorders
  have installedAtStart :=
    (source.recordedExactIsLocalInstall start server reference
      recorderMembership.2).1
  have startBeforeCertificate :
      start ≤ certificateDeadline nextBlocks := by
    dsimp [certificateDeadline]
    calc
      start ≤ nextBlocks.deadline := nextBlocks.readyBeforeDeadline
      _ ≤ nextBlocks.deadline + timed.localActionBound :=
        Nat.le_add_right _ _
      _ ≤ nextBlocks.deadline + timed.localActionBound + network.delta :=
        Nat.le_add_right _ _
      _ ≤ nextBlocks.deadline + timed.localActionBound + network.delta + 3 :=
        Nat.le_add_right _ _
  have installedAtCertificate := timed.execution.installed_commit_persists
    serverInRange startBeforeCertificate installedAtStart
  have certificate := source.observer_has_exact_certificate nextBlocks afterGst
    recordedQuorum serverInRange recorderMembership.1
  let bundle := source.bundleFor server afterIndex reference
    (certificateDeadline nextBlocks)
  have retained := source.certificateCreatesRetainedBundle server afterIndex
    reference (certificateDeadline nextBlocks)
    afterIndexBeforeReference installedAtCertificate certificate
  exact ⟨server, bundle, serverInRange, recorderMembership.1,
    installedAtCertificate, certificate, retained.1, retained.2⟩

end ValidatorCommitEvidenceSourceMap

end Mysticeti
