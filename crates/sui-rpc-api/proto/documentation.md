# Protocol Documentation
<a name="top"></a>

## Table of Contents

- [sui.node.v2.proto](#sui-node-v2-proto)
    - [BalanceChange](#sui-node-v2-BalanceChange)
    - [BalanceChanges](#sui-node-v2-BalanceChanges)
    - [EffectsFinality](#sui-node-v2-EffectsFinality)
    - [ExecuteTransactionOptions](#sui-node-v2-ExecuteTransactionOptions)
    - [ExecuteTransactionRequest](#sui-node-v2-ExecuteTransactionRequest)
    - [ExecuteTransactionResponse](#sui-node-v2-ExecuteTransactionResponse)
    - [FullCheckpointObject](#sui-node-v2-FullCheckpointObject)
    - [FullCheckpointObjects](#sui-node-v2-FullCheckpointObjects)
    - [FullCheckpointTransaction](#sui-node-v2-FullCheckpointTransaction)
    - [GetCheckpointOptions](#sui-node-v2-GetCheckpointOptions)
    - [GetCheckpointRequest](#sui-node-v2-GetCheckpointRequest)
    - [GetCheckpointResponse](#sui-node-v2-GetCheckpointResponse)
    - [GetCommitteeRequest](#sui-node-v2-GetCommitteeRequest)
    - [GetCommitteeResponse](#sui-node-v2-GetCommitteeResponse)
    - [GetFullCheckpointOptions](#sui-node-v2-GetFullCheckpointOptions)
    - [GetFullCheckpointRequest](#sui-node-v2-GetFullCheckpointRequest)
    - [GetFullCheckpointResponse](#sui-node-v2-GetFullCheckpointResponse)
    - [GetNodeInfoRequest](#sui-node-v2-GetNodeInfoRequest)
    - [GetNodeInfoResponse](#sui-node-v2-GetNodeInfoResponse)
    - [GetObjectOptions](#sui-node-v2-GetObjectOptions)
    - [GetObjectRequest](#sui-node-v2-GetObjectRequest)
    - [GetObjectResponse](#sui-node-v2-GetObjectResponse)
    - [GetTransactionOptions](#sui-node-v2-GetTransactionOptions)
    - [GetTransactionRequest](#sui-node-v2-GetTransactionRequest)
    - [GetTransactionResponse](#sui-node-v2-GetTransactionResponse)
    - [UserSignatures](#sui-node-v2-UserSignatures)
    - [UserSignaturesBytes](#sui-node-v2-UserSignaturesBytes)
  
    - [NodeService](#sui-node-v2-NodeService)
  
- [sui.types.proto](#sui-types-proto)
    - [ActiveJwk](#sui-types-ActiveJwk)
    - [Address](#sui-types-Address)
    - [AddressDeniedForCoinError](#sui-types-AddressDeniedForCoinError)
    - [Argument](#sui-types-Argument)
    - [AuthenticatorStateExpire](#sui-types-AuthenticatorStateExpire)
    - [AuthenticatorStateUpdate](#sui-types-AuthenticatorStateUpdate)
    - [Bcs](#sui-types-Bcs)
    - [Bn254FieldElement](#sui-types-Bn254FieldElement)
    - [CancelledTransaction](#sui-types-CancelledTransaction)
    - [CancelledTransactions](#sui-types-CancelledTransactions)
    - [ChangeEpoch](#sui-types-ChangeEpoch)
    - [ChangedObject](#sui-types-ChangedObject)
    - [CheckpointCommitment](#sui-types-CheckpointCommitment)
    - [CheckpointContents](#sui-types-CheckpointContents)
    - [CheckpointContents.V1](#sui-types-CheckpointContents-V1)
    - [CheckpointSummary](#sui-types-CheckpointSummary)
    - [CheckpointedTransactionInfo](#sui-types-CheckpointedTransactionInfo)
    - [CircomG1](#sui-types-CircomG1)
    - [CircomG2](#sui-types-CircomG2)
    - [Command](#sui-types-Command)
    - [CommandArgumentError](#sui-types-CommandArgumentError)
    - [CongestedObjectsError](#sui-types-CongestedObjectsError)
    - [ConsensusCommitPrologue](#sui-types-ConsensusCommitPrologue)
    - [ConsensusDeterminedVersionAssignments](#sui-types-ConsensusDeterminedVersionAssignments)
    - [Digest](#sui-types-Digest)
    - [EndOfEpochData](#sui-types-EndOfEpochData)
    - [EndOfEpochTransaction](#sui-types-EndOfEpochTransaction)
    - [EndOfEpochTransactionKind](#sui-types-EndOfEpochTransactionKind)
    - [Event](#sui-types-Event)
    - [ExecutionStatus](#sui-types-ExecutionStatus)
    - [FailureStatus](#sui-types-FailureStatus)
    - [GasCostSummary](#sui-types-GasCostSummary)
    - [GasPayment](#sui-types-GasPayment)
    - [GenesisObject](#sui-types-GenesisObject)
    - [GenesisTransaction](#sui-types-GenesisTransaction)
    - [I128](#sui-types-I128)
    - [Identifier](#sui-types-Identifier)
    - [Input](#sui-types-Input)
    - [Jwk](#sui-types-Jwk)
    - [JwkId](#sui-types-JwkId)
    - [MakeMoveVector](#sui-types-MakeMoveVector)
    - [MergeCoins](#sui-types-MergeCoins)
    - [ModifiedAtVersion](#sui-types-ModifiedAtVersion)
    - [MoveCall](#sui-types-MoveCall)
    - [MoveError](#sui-types-MoveError)
    - [MoveField](#sui-types-MoveField)
    - [MoveLocation](#sui-types-MoveLocation)
    - [MoveModule](#sui-types-MoveModule)
    - [MovePackage](#sui-types-MovePackage)
    - [MoveStruct](#sui-types-MoveStruct)
    - [MoveStructValue](#sui-types-MoveStructValue)
    - [MoveValue](#sui-types-MoveValue)
    - [MoveVariant](#sui-types-MoveVariant)
    - [MoveVector](#sui-types-MoveVector)
    - [MultisigAggregatedSignature](#sui-types-MultisigAggregatedSignature)
    - [MultisigCommittee](#sui-types-MultisigCommittee)
    - [MultisigMember](#sui-types-MultisigMember)
    - [MultisigMemberPublicKey](#sui-types-MultisigMemberPublicKey)
    - [MultisigMemberSignature](#sui-types-MultisigMemberSignature)
    - [NestedResult](#sui-types-NestedResult)
    - [Object](#sui-types-Object)
    - [ObjectData](#sui-types-ObjectData)
    - [ObjectExist](#sui-types-ObjectExist)
    - [ObjectId](#sui-types-ObjectId)
    - [ObjectReference](#sui-types-ObjectReference)
    - [ObjectReferenceWithOwner](#sui-types-ObjectReferenceWithOwner)
    - [ObjectWrite](#sui-types-ObjectWrite)
    - [Owner](#sui-types-Owner)
    - [PackageIdDoesNotMatch](#sui-types-PackageIdDoesNotMatch)
    - [PackageUpgradeError](#sui-types-PackageUpgradeError)
    - [PackageWrite](#sui-types-PackageWrite)
    - [PasskeyAuthenticator](#sui-types-PasskeyAuthenticator)
    - [ProgrammableTransaction](#sui-types-ProgrammableTransaction)
    - [Publish](#sui-types-Publish)
    - [RandomnessStateUpdate](#sui-types-RandomnessStateUpdate)
    - [ReadOnlyRoot](#sui-types-ReadOnlyRoot)
    - [RoaringBitmap](#sui-types-RoaringBitmap)
    - [SharedObjectInput](#sui-types-SharedObjectInput)
    - [SimpleSignature](#sui-types-SimpleSignature)
    - [SizeError](#sui-types-SizeError)
    - [SplitCoins](#sui-types-SplitCoins)
    - [StructTag](#sui-types-StructTag)
    - [SystemPackage](#sui-types-SystemPackage)
    - [Transaction](#sui-types-Transaction)
    - [Transaction.TransactionV1](#sui-types-Transaction-TransactionV1)
    - [TransactionEffects](#sui-types-TransactionEffects)
    - [TransactionEffectsV1](#sui-types-TransactionEffectsV1)
    - [TransactionEffectsV2](#sui-types-TransactionEffectsV2)
    - [TransactionEvents](#sui-types-TransactionEvents)
    - [TransactionExpiration](#sui-types-TransactionExpiration)
    - [TransactionKind](#sui-types-TransactionKind)
    - [TransferObjects](#sui-types-TransferObjects)
    - [TypeArgumentError](#sui-types-TypeArgumentError)
    - [TypeOrigin](#sui-types-TypeOrigin)
    - [TypeTag](#sui-types-TypeTag)
    - [U128](#sui-types-U128)
    - [U256](#sui-types-U256)
    - [UnchangedSharedObject](#sui-types-UnchangedSharedObject)
    - [Upgrade](#sui-types-Upgrade)
    - [UpgradeInfo](#sui-types-UpgradeInfo)
    - [UserSignature](#sui-types-UserSignature)
    - [ValidatorAggregatedSignature](#sui-types-ValidatorAggregatedSignature)
    - [ValidatorCommittee](#sui-types-ValidatorCommittee)
    - [ValidatorCommitteeMember](#sui-types-ValidatorCommitteeMember)
    - [VersionAssignment](#sui-types-VersionAssignment)
    - [ZkLoginAuthenticator](#sui-types-ZkLoginAuthenticator)
    - [ZkLoginClaim](#sui-types-ZkLoginClaim)
    - [ZkLoginInputs](#sui-types-ZkLoginInputs)
    - [ZkLoginProof](#sui-types-ZkLoginProof)
    - [ZkLoginPublicIdentifier](#sui-types-ZkLoginPublicIdentifier)
  
    - [SignatureScheme](#sui-types-SignatureScheme)
  
- [google/protobuf/empty.proto](#google_protobuf_empty-proto)
    - [Empty](#google-protobuf-Empty)
  
- [google/protobuf/timestamp.proto](#google_protobuf_timestamp-proto)
    - [Timestamp](#google-protobuf-Timestamp)
  
- [Scalar Value Types](#scalar-value-types)



<a name="sui-node-v2-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## sui.node.v2.proto



<a name="sui-node-v2-BalanceChange"></a>

### BalanceChange



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| address | [sui.types.Address](#sui-types-Address) | optional |  |
| coin_type | [sui.types.TypeTag](#sui-types-TypeTag) | optional |  |
| amount | [sui.types.I128](#sui-types-I128) | optional |  |






<a name="sui-node-v2-BalanceChanges"></a>

### BalanceChanges



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| balance_changes | [BalanceChange](#sui-node-v2-BalanceChange) | repeated |  |






<a name="sui-node-v2-EffectsFinality"></a>

### EffectsFinality



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| certified | [sui.types.ValidatorAggregatedSignature](#sui-types-ValidatorAggregatedSignature) |  |  |
| checkpointed | [uint64](#uint64) |  |  |
| quorum_executed | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |






<a name="sui-node-v2-ExecuteTransactionOptions"></a>

### ExecuteTransactionOptions



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| effects | [bool](#bool) | optional | Include the sui.types.TransactionEffects message in the response.

Defaults to `false` if not included |
| effects_bcs | [bool](#bool) | optional | Include the TransactionEffects formatted as BCS in the response.

Defaults to `false` if not included |
| events | [bool](#bool) | optional | Include the sui.types.TransactionEvents message in the response.

Defaults to `false` if not included |
| events_bcs | [bool](#bool) | optional | Include the TransactionEvents formatted as BCS in the response.

Defaults to `false` if not included |
| balance_changes | [bool](#bool) | optional | Include the BalanceChanges in the response.

Defaults to `false` if not included |






<a name="sui-node-v2-ExecuteTransactionRequest"></a>

### ExecuteTransactionRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| transaction | [sui.types.Transaction](#sui-types-Transaction) | optional |  |
| transaction_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |
| signatures | [UserSignatures](#sui-node-v2-UserSignatures) | optional |  |
| signatures_bytes | [UserSignaturesBytes](#sui-node-v2-UserSignaturesBytes) | optional |  |
| options | [ExecuteTransactionOptions](#sui-node-v2-ExecuteTransactionOptions) | optional |  |






<a name="sui-node-v2-ExecuteTransactionResponse"></a>

### ExecuteTransactionResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| finality | [EffectsFinality](#sui-node-v2-EffectsFinality) | optional |  |
| effects | [sui.types.TransactionEffects](#sui-types-TransactionEffects) | optional |  |
| effects_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |
| events | [sui.types.TransactionEvents](#sui-types-TransactionEvents) | optional |  |
| events_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |
| balance_changes | [BalanceChanges](#sui-node-v2-BalanceChanges) | optional |  |






<a name="sui-node-v2-FullCheckpointObject"></a>

### FullCheckpointObject



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| object_id | [sui.types.ObjectId](#sui-types-ObjectId) | optional |  |
| version | [uint64](#uint64) | optional |  |
| digest | [sui.types.Digest](#sui-types-Digest) | optional | The digest of this object |
| object | [sui.types.Object](#sui-types-Object) | optional |  |
| object_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |






<a name="sui-node-v2-FullCheckpointObjects"></a>

### FullCheckpointObjects



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| objects | [FullCheckpointObject](#sui-node-v2-FullCheckpointObject) | repeated |  |






<a name="sui-node-v2-FullCheckpointTransaction"></a>

### FullCheckpointTransaction



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| digest | [sui.types.Digest](#sui-types-Digest) | optional | The digest of this transaction |
| transaction | [sui.types.Transaction](#sui-types-Transaction) | optional |  |
| transaction_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |
| effects | [sui.types.TransactionEffects](#sui-types-TransactionEffects) | optional |  |
| effects_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |
| events | [sui.types.TransactionEvents](#sui-types-TransactionEvents) | optional |  |
| events_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |
| input_objects | [FullCheckpointObjects](#sui-node-v2-FullCheckpointObjects) | optional |  |
| output_objects | [FullCheckpointObjects](#sui-node-v2-FullCheckpointObjects) | optional |  |






<a name="sui-node-v2-GetCheckpointOptions"></a>

### GetCheckpointOptions



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| summary | [bool](#bool) | optional | Include the sui.types.CheckpointSummary in the response.

Defaults to `false` if not included |
| summary_bcs | [bool](#bool) | optional | Include the CheckpointSummary formatted as BCS in the response.

Defaults to `false` if not included |
| signature | [bool](#bool) | optional | Include the sui.types.ValidatorAggregatedSignature in the response.

Defaults to `false` if not included |
| contents | [bool](#bool) | optional | Include the sui.types.CheckpointContents message in the response.

Defaults to `false` if not included |
| contents_bcs | [bool](#bool) | optional | Include the CheckpointContents formatted as BCS in the response.

Defaults to `false` if not included |






<a name="sui-node-v2-GetCheckpointRequest"></a>

### GetCheckpointRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| sequence_number | [uint64](#uint64) | optional |  |
| digest | [sui.types.Digest](#sui-types-Digest) | optional |  |
| options | [GetCheckpointOptions](#sui-node-v2-GetCheckpointOptions) | optional |  |






<a name="sui-node-v2-GetCheckpointResponse"></a>

### GetCheckpointResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| sequence_number | [uint64](#uint64) | optional | The sequence number of this Checkpoint |
| digest | [sui.types.Digest](#sui-types-Digest) | optional | The digest of this Checkpoint&#39;s CheckpointSummary |
| summary | [sui.types.CheckpointSummary](#sui-types-CheckpointSummary) | optional |  |
| summary_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |
| signature | [sui.types.ValidatorAggregatedSignature](#sui-types-ValidatorAggregatedSignature) | optional |  |
| contents | [sui.types.CheckpointContents](#sui-types-CheckpointContents) | optional |  |
| contents_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |






<a name="sui-node-v2-GetCommitteeRequest"></a>

### GetCommitteeRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| epoch | [uint64](#uint64) | optional |  |






<a name="sui-node-v2-GetCommitteeResponse"></a>

### GetCommitteeResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| committee | [sui.types.ValidatorCommittee](#sui-types-ValidatorCommittee) | optional |  |






<a name="sui-node-v2-GetFullCheckpointOptions"></a>

### GetFullCheckpointOptions



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| summary | [bool](#bool) | optional | Include the sui.types.CheckpointSummary in the response.

Defaults to `false` if not included |
| summary_bcs | [bool](#bool) | optional | Include the CheckpointSummary formatted as BCS in the response.

Defaults to `false` if not included |
| signature | [bool](#bool) | optional | Include the sui.types.ValidatorAggregatedSignature in the response.

Defaults to `false` if not included |
| contents | [bool](#bool) | optional | Include the sui.types.CheckpointContents message in the response.

Defaults to `false` if not included |
| contents_bcs | [bool](#bool) | optional | Include the CheckpointContents formatted as BCS in the response.

Defaults to `false` if not included |
| transaction | [bool](#bool) | optional | Include the sui.types.Transaction message in the response.

Defaults to `false` if not included |
| transaction_bcs | [bool](#bool) | optional | Include the Transaction formatted as BCS in the response.

Defaults to `false` if not included |
| effects | [bool](#bool) | optional | Include the sui.types.TransactionEffects message in the response.

Defaults to `false` if not included |
| effects_bcs | [bool](#bool) | optional | Include the TransactionEffects formatted as BCS in the response.

Defaults to `false` if not included |
| events | [bool](#bool) | optional | Include the sui.types.TransactionEvents message in the response.

Defaults to `false` if not included |
| events_bcs | [bool](#bool) | optional | Include the TransactionEvents formatted as BCS in the response.

Defaults to `false` if not included |
| input_objects | [bool](#bool) | optional | Include the input objects for transactions in the response.

Defaults to `false` if not included |
| output_objects | [bool](#bool) | optional | Include the output objects for transactions in the response.

Defaults to `false` if not included |
| object | [bool](#bool) | optional | Include the sui.types.Object message in the response.

Defaults to `false` if not included |
| object_bcs | [bool](#bool) | optional | Include the Object formatted as BCS in the response.

Defaults to `false` if not included |






<a name="sui-node-v2-GetFullCheckpointRequest"></a>

### GetFullCheckpointRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| sequence_number | [uint64](#uint64) | optional |  |
| digest | [sui.types.Digest](#sui-types-Digest) | optional |  |
| options | [GetFullCheckpointOptions](#sui-node-v2-GetFullCheckpointOptions) | optional |  |






<a name="sui-node-v2-GetFullCheckpointResponse"></a>

### GetFullCheckpointResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| sequence_number | [uint64](#uint64) | optional | The sequence number of this Checkpoint |
| digest | [sui.types.Digest](#sui-types-Digest) | optional | The digest of this Checkpoint&#39;s CheckpointSummary |
| summary | [sui.types.CheckpointSummary](#sui-types-CheckpointSummary) | optional |  |
| summary_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |
| signature | [sui.types.ValidatorAggregatedSignature](#sui-types-ValidatorAggregatedSignature) | optional |  |
| contents | [sui.types.CheckpointContents](#sui-types-CheckpointContents) | optional |  |
| contents_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |
| transactions | [FullCheckpointTransaction](#sui-node-v2-FullCheckpointTransaction) | repeated |  |






<a name="sui-node-v2-GetNodeInfoRequest"></a>

### GetNodeInfoRequest







<a name="sui-node-v2-GetNodeInfoResponse"></a>

### GetNodeInfoResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| chain_id | [sui.types.Digest](#sui-types-Digest) | optional | The chain identifier of the chain that this Node is on |
| chain | [string](#string) | optional | Human readable name of the chain that this Node is on |
| epoch | [uint64](#uint64) | optional | Current epoch of the Node based on its highest executed checkpoint |
| checkpoint_height | [uint64](#uint64) | optional | Checkpoint height of the most recently executed checkpoint |
| timestamp | [google.protobuf.Timestamp](#google-protobuf-Timestamp) | optional | Unix timestamp of the most recently executed checkpoint |
| lowest_available_checkpoint | [uint64](#uint64) | optional | The lowest checkpoint for which checkpoints and transaction data is available |
| lowest_available_checkpoint_objects | [uint64](#uint64) | optional | The lowest checkpoint for which object data is available |
| software_version | [string](#string) | optional |  |






<a name="sui-node-v2-GetObjectOptions"></a>

### GetObjectOptions



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| object | [bool](#bool) | optional | Include the sui.types.Object message in the response.

Defaults to `false` if not included |
| object_bcs | [bool](#bool) | optional | Include the Object formatted as BCS in the response.

Defaults to `false` if not included |






<a name="sui-node-v2-GetObjectRequest"></a>

### GetObjectRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| object_id | [sui.types.ObjectId](#sui-types-ObjectId) | optional |  |
| version | [uint64](#uint64) | optional |  |
| options | [GetObjectOptions](#sui-node-v2-GetObjectOptions) | optional |  |






<a name="sui-node-v2-GetObjectResponse"></a>

### GetObjectResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| object_id | [sui.types.ObjectId](#sui-types-ObjectId) | optional |  |
| version | [uint64](#uint64) | optional |  |
| digest | [sui.types.Digest](#sui-types-Digest) | optional | The digest of this object |
| object | [sui.types.Object](#sui-types-Object) | optional |  |
| object_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |






<a name="sui-node-v2-GetTransactionOptions"></a>

### GetTransactionOptions



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| transaction | [bool](#bool) | optional | Include the sui.types.Transaction message in the response.

Defaults to `false` if not included |
| transaction_bcs | [bool](#bool) | optional | Include the Transaction formatted as BCS in the response.

Defaults to `false` if not included |
| signatures | [bool](#bool) | optional | Include the set of sui.types.UserSignature&#39;s in the response.

Defaults to `false` if not included |
| signatures_bytes | [bool](#bool) | optional | Include the set of UserSignature&#39;s encoded as bytes in the response.

Defaults to `false` if not included |
| effects | [bool](#bool) | optional | Include the sui.types.TransactionEffects message in the response.

Defaults to `false` if not included |
| effects_bcs | [bool](#bool) | optional | Include the TransactionEffects formatted as BCS in the response.

Defaults to `false` if not included |
| events | [bool](#bool) | optional | Include the sui.types.TransactionEvents message in the response.

Defaults to `false` if not included |
| events_bcs | [bool](#bool) | optional | Include the TransactionEvents formatted as BCS in the response.

Defaults to `false` if not included |






<a name="sui-node-v2-GetTransactionRequest"></a>

### GetTransactionRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| digest | [sui.types.Digest](#sui-types-Digest) | optional |  |
| options | [GetTransactionOptions](#sui-node-v2-GetTransactionOptions) | optional |  |






<a name="sui-node-v2-GetTransactionResponse"></a>

### GetTransactionResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| digest | [sui.types.Digest](#sui-types-Digest) | optional | The digest of this transaction |
| transaction | [sui.types.Transaction](#sui-types-Transaction) | optional |  |
| transaction_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |
| signatures | [UserSignatures](#sui-node-v2-UserSignatures) | optional |  |
| signatures_bytes | [UserSignaturesBytes](#sui-node-v2-UserSignaturesBytes) | optional |  |
| effects | [sui.types.TransactionEffects](#sui-types-TransactionEffects) | optional |  |
| effects_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |
| events | [sui.types.TransactionEvents](#sui-types-TransactionEvents) | optional |  |
| events_bcs | [sui.types.Bcs](#sui-types-Bcs) | optional |  |
| checkpoint | [uint64](#uint64) | optional |  |
| timestamp | [google.protobuf.Timestamp](#google-protobuf-Timestamp) | optional |  |






<a name="sui-node-v2-UserSignatures"></a>

### UserSignatures



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| signatures | [sui.types.UserSignature](#sui-types-UserSignature) | repeated |  |






<a name="sui-node-v2-UserSignaturesBytes"></a>

### UserSignaturesBytes



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| signatures | [bytes](#bytes) | repeated |  |





 

 

 


<a name="sui-node-v2-NodeService"></a>

### NodeService


| Method Name | Request Type | Response Type | Description |
| ----------- | ------------ | ------------- | ------------|
| GetNodeInfo | [GetNodeInfoRequest](#sui-node-v2-GetNodeInfoRequest) | [GetNodeInfoResponse](#sui-node-v2-GetNodeInfoResponse) |  |
| GetCommittee | [GetCommitteeRequest](#sui-node-v2-GetCommitteeRequest) | [GetCommitteeResponse](#sui-node-v2-GetCommitteeResponse) |  |
| GetObject | [GetObjectRequest](#sui-node-v2-GetObjectRequest) | [GetObjectResponse](#sui-node-v2-GetObjectResponse) |  |
| GetTransaction | [GetTransactionRequest](#sui-node-v2-GetTransactionRequest) | [GetTransactionResponse](#sui-node-v2-GetTransactionResponse) |  |
| GetCheckpoint | [GetCheckpointRequest](#sui-node-v2-GetCheckpointRequest) | [GetCheckpointResponse](#sui-node-v2-GetCheckpointResponse) |  |
| GetFullCheckpoint | [GetFullCheckpointRequest](#sui-node-v2-GetFullCheckpointRequest) | [GetFullCheckpointResponse](#sui-node-v2-GetFullCheckpointResponse) |  |
| ExecuteTransaction | [ExecuteTransactionRequest](#sui-node-v2-ExecuteTransactionRequest) | [ExecuteTransactionResponse](#sui-node-v2-ExecuteTransactionResponse) |  |

 



<a name="sui-types-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## sui.types.proto



<a name="sui-types-ActiveJwk"></a>

### ActiveJwk



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| id | [JwkId](#sui-types-JwkId) | optional |  |
| jwk | [Jwk](#sui-types-Jwk) | optional |  |
| epoch | [uint64](#uint64) | optional |  |






<a name="sui-types-Address"></a>

### Address



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| address | [bytes](#bytes) | optional |  |






<a name="sui-types-AddressDeniedForCoinError"></a>

### AddressDeniedForCoinError



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| address | [Address](#sui-types-Address) | optional |  |
| coin_type | [string](#string) | optional |  |






<a name="sui-types-Argument"></a>

### Argument



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| gas | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| input | [uint32](#uint32) |  |  |
| result | [uint32](#uint32) |  |  |
| nested_result | [NestedResult](#sui-types-NestedResult) |  |  |






<a name="sui-types-AuthenticatorStateExpire"></a>

### AuthenticatorStateExpire



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| min_epoch | [uint64](#uint64) | optional |  |
| authenticator_object_initial_shared_version | [uint64](#uint64) | optional |  |






<a name="sui-types-AuthenticatorStateUpdate"></a>

### AuthenticatorStateUpdate



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| epoch | [uint64](#uint64) | optional |  |
| round | [uint64](#uint64) | optional |  |
| new_active_jwks | [ActiveJwk](#sui-types-ActiveJwk) | repeated |  |
| authenticator_object_initial_shared_version | [uint64](#uint64) | optional |  |






<a name="sui-types-Bcs"></a>

### Bcs



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| bcs | [bytes](#bytes) | optional |  |






<a name="sui-types-Bn254FieldElement"></a>

### Bn254FieldElement



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| element | [bytes](#bytes) | optional |  |






<a name="sui-types-CancelledTransaction"></a>

### CancelledTransaction



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| digest | [Digest](#sui-types-Digest) | optional |  |
| version_assignments | [VersionAssignment](#sui-types-VersionAssignment) | repeated |  |






<a name="sui-types-CancelledTransactions"></a>

### CancelledTransactions



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| cancelled_transactions | [CancelledTransaction](#sui-types-CancelledTransaction) | repeated |  |






<a name="sui-types-ChangeEpoch"></a>

### ChangeEpoch



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| epoch | [uint64](#uint64) | optional | The next (to become) epoch ID. |
| protocol_version | [uint64](#uint64) | optional | The protocol version in effect in the new epoch. |
| storage_charge | [uint64](#uint64) | optional | The total amount of gas charged for storage during the epoch. |
| computation_charge | [uint64](#uint64) | optional | The total amount of gas charged for computation during the epoch. |
| storage_rebate | [uint64](#uint64) | optional | The amount of storage rebate refunded to the txn senders. |
| non_refundable_storage_fee | [uint64](#uint64) | optional | The non-refundable storage fee. |
| epoch_start_timestamp_ms | [uint64](#uint64) | optional | Unix timestamp when epoch started |
| system_packages | [SystemPackage](#sui-types-SystemPackage) | repeated | System packages (specifically framework and move stdlib) that are written before the new epoch starts. This tracks framework upgrades on chain. When executing the ChangeEpoch txn, the validator must write out the modules below. Modules are provided with the version they will be upgraded to, their modules in serialized form (which include their package ID), and a list of their transitive dependencies. |






<a name="sui-types-ChangedObject"></a>

### ChangedObject



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| object_id | [ObjectId](#sui-types-ObjectId) | optional |  |
| not_exist | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| exist | [ObjectExist](#sui-types-ObjectExist) |  |  |
| removed | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| object_write | [ObjectWrite](#sui-types-ObjectWrite) |  |  |
| package_write | [PackageWrite](#sui-types-PackageWrite) |  |  |
| none | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| created | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| deleted | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |






<a name="sui-types-CheckpointCommitment"></a>

### CheckpointCommitment



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| ecmh_live_object_set | [Digest](#sui-types-Digest) |  |  |






<a name="sui-types-CheckpointContents"></a>

### CheckpointContents



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| v1 | [CheckpointContents.V1](#sui-types-CheckpointContents-V1) |  |  |






<a name="sui-types-CheckpointContents-V1"></a>

### CheckpointContents.V1



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| transactions | [CheckpointedTransactionInfo](#sui-types-CheckpointedTransactionInfo) | repeated |  |






<a name="sui-types-CheckpointSummary"></a>

### CheckpointSummary



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| epoch | [uint64](#uint64) | optional |  |
| sequence_number | [uint64](#uint64) | optional |  |
| total_network_transactions | [uint64](#uint64) | optional |  |
| content_digest | [Digest](#sui-types-Digest) | optional |  |
| previous_digest | [Digest](#sui-types-Digest) | optional |  |
| epoch_rolling_gas_cost_summary | [GasCostSummary](#sui-types-GasCostSummary) | optional |  |
| timestamp_ms | [uint64](#uint64) | optional |  |
| commitments | [CheckpointCommitment](#sui-types-CheckpointCommitment) | repeated |  |
| end_of_epoch_data | [EndOfEpochData](#sui-types-EndOfEpochData) | optional |  |
| version_specific_data | [bytes](#bytes) | optional |  |






<a name="sui-types-CheckpointedTransactionInfo"></a>

### CheckpointedTransactionInfo



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| transaction | [Digest](#sui-types-Digest) | optional | TransactionDigest |
| effects | [Digest](#sui-types-Digest) | optional | EffectsDigest |
| signatures | [UserSignature](#sui-types-UserSignature) | repeated |  |






<a name="sui-types-CircomG1"></a>

### CircomG1



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| e0 | [Bn254FieldElement](#sui-types-Bn254FieldElement) | optional |  |
| e1 | [Bn254FieldElement](#sui-types-Bn254FieldElement) | optional |  |
| e2 | [Bn254FieldElement](#sui-types-Bn254FieldElement) | optional |  |






<a name="sui-types-CircomG2"></a>

### CircomG2



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| e00 | [Bn254FieldElement](#sui-types-Bn254FieldElement) | optional |  |
| e01 | [Bn254FieldElement](#sui-types-Bn254FieldElement) | optional |  |
| e10 | [Bn254FieldElement](#sui-types-Bn254FieldElement) | optional |  |
| e11 | [Bn254FieldElement](#sui-types-Bn254FieldElement) | optional |  |
| e20 | [Bn254FieldElement](#sui-types-Bn254FieldElement) | optional |  |
| e21 | [Bn254FieldElement](#sui-types-Bn254FieldElement) | optional |  |






<a name="sui-types-Command"></a>

### Command



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| move_call | [MoveCall](#sui-types-MoveCall) |  |  |
| transfer_objects | [TransferObjects](#sui-types-TransferObjects) |  |  |
| split_coins | [SplitCoins](#sui-types-SplitCoins) |  |  |
| merge_coins | [MergeCoins](#sui-types-MergeCoins) |  |  |
| publish | [Publish](#sui-types-Publish) |  |  |
| make_move_vector | [MakeMoveVector](#sui-types-MakeMoveVector) |  |  |
| upgrade | [Upgrade](#sui-types-Upgrade) |  |  |






<a name="sui-types-CommandArgumentError"></a>

### CommandArgumentError



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| argument | [uint32](#uint32) | optional |  |
| type_mismatch | [google.protobuf.Empty](#google-protobuf-Empty) |  | The type of the value does not match the expected type |
| invalid_bcs_bytes | [google.protobuf.Empty](#google-protobuf-Empty) |  | The argument cannot be deserialized into a value of the specified type |
| invalid_usage_of_pure_argument | [google.protobuf.Empty](#google-protobuf-Empty) |  | The argument cannot be instantiated from raw bytes |
| invalid_argument_to_private_entry_function | [google.protobuf.Empty](#google-protobuf-Empty) |  | Invalid argument to private entry function. / Private entry functions cannot take arguments from other Move functions. |
| index_out_of_bounds | [uint32](#uint32) |  | Out of bounds access to input or results |
| secondary_index_out_of_bounds | [NestedResult](#sui-types-NestedResult) |  | Out of bounds access to subresult |
| invalid_result_arity | [uint32](#uint32) |  | Invalid usage of result. / Expected a single result but found either no return value or multiple. |
| invalid_gas_coin_usage | [google.protobuf.Empty](#google-protobuf-Empty) |  | Invalid usage of Gas coin. / The Gas coin can only be used by-value with a TransferObjects command. |
| invalid_value_usage | [google.protobuf.Empty](#google-protobuf-Empty) |  | Invalid usage of move value. Mutably borrowed values require unique usage. Immutably borrowed values cannot be taken or borrowed mutably. Taken values cannot be used again. |
| invalid_object_by_value | [google.protobuf.Empty](#google-protobuf-Empty) |  | Immutable objects cannot be passed by-value. |
| invalid_object_by_mut_ref | [google.protobuf.Empty](#google-protobuf-Empty) |  | Immutable objects cannot be passed by mutable reference, &amp;mut. |
| shared_object_operation_not_allowed | [google.protobuf.Empty](#google-protobuf-Empty) |  | Shared object operations such a wrapping, freezing, or converting to owned are not / allowed. |






<a name="sui-types-CongestedObjectsError"></a>

### CongestedObjectsError



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| congested_objects | [ObjectId](#sui-types-ObjectId) | repeated |  |






<a name="sui-types-ConsensusCommitPrologue"></a>

### ConsensusCommitPrologue



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| epoch | [uint64](#uint64) | optional |  |
| round | [uint64](#uint64) | optional |  |
| commit_timestamp_ms | [uint64](#uint64) | optional |  |
| consensus_commit_digest | [Digest](#sui-types-Digest) | optional |  |
| sub_dag_index | [uint64](#uint64) | optional |  |
| consensus_determined_version_assignments | [ConsensusDeterminedVersionAssignments](#sui-types-ConsensusDeterminedVersionAssignments) | optional |  |






<a name="sui-types-ConsensusDeterminedVersionAssignments"></a>

### ConsensusDeterminedVersionAssignments



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| cancelled_transactions | [CancelledTransactions](#sui-types-CancelledTransactions) |  |  |






<a name="sui-types-Digest"></a>

### Digest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| digest | [bytes](#bytes) | optional |  |






<a name="sui-types-EndOfEpochData"></a>

### EndOfEpochData



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| next_epoch_committee | [ValidatorCommitteeMember](#sui-types-ValidatorCommitteeMember) | repeated |  |
| next_epoch_protocol_version | [uint64](#uint64) | optional |  |
| epoch_commitments | [CheckpointCommitment](#sui-types-CheckpointCommitment) | repeated |  |






<a name="sui-types-EndOfEpochTransaction"></a>

### EndOfEpochTransaction



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| transactions | [EndOfEpochTransactionKind](#sui-types-EndOfEpochTransactionKind) | repeated |  |






<a name="sui-types-EndOfEpochTransactionKind"></a>

### EndOfEpochTransactionKind



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| change_epoch | [ChangeEpoch](#sui-types-ChangeEpoch) |  |  |
| authenticator_state_expire | [AuthenticatorStateExpire](#sui-types-AuthenticatorStateExpire) |  |  |
| authenticator_state_create | [google.protobuf.Empty](#google-protobuf-Empty) |  | Use higher field numbers for kinds which happen infrequently |
| randomness_state_create | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| deny_list_state_create | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| bridge_state_create | [Digest](#sui-types-Digest) |  |  |
| bridge_committee_init | [uint64](#uint64) |  |  |






<a name="sui-types-Event"></a>

### Event



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| package_id | [ObjectId](#sui-types-ObjectId) | optional |  |
| module | [Identifier](#sui-types-Identifier) | optional |  |
| sender | [Address](#sui-types-Address) | optional |  |
| event_type | [StructTag](#sui-types-StructTag) | optional |  |
| contents | [bytes](#bytes) | optional |  |






<a name="sui-types-ExecutionStatus"></a>

### ExecutionStatus



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| success | [bool](#bool) | optional |  |
| status | [FailureStatus](#sui-types-FailureStatus) | optional |  |






<a name="sui-types-FailureStatus"></a>

### FailureStatus



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| command | [uint64](#uint64) | optional |  |
| insufficient_gas | [google.protobuf.Empty](#google-protobuf-Empty) |  | Insufficient Gas |
| invalid_gas_object | [google.protobuf.Empty](#google-protobuf-Empty) |  | Invalid Gas Object. |
| invariant_violation | [google.protobuf.Empty](#google-protobuf-Empty) |  | Invariant Violation |
| feature_not_yet_supported | [google.protobuf.Empty](#google-protobuf-Empty) |  | Attempted to used feature that is not supported yet |
| object_too_big | [SizeError](#sui-types-SizeError) |  | Move object is larger than the maximum allowed size |
| package_too_big | [SizeError](#sui-types-SizeError) |  | Package is larger than the maximum allowed size |
| circular_object_ownership | [ObjectId](#sui-types-ObjectId) |  | Circular Object Ownership |
| insufficient_coin_balance | [google.protobuf.Empty](#google-protobuf-Empty) |  | Coin errors

/ Insufficient coin balance for requested operation |
| coin_balance_overflow | [google.protobuf.Empty](#google-protobuf-Empty) |  | Coin balance overflowed an u64 |
| publish_error_non_zero_address | [google.protobuf.Empty](#google-protobuf-Empty) |  | Publish/Upgrade errors

/ Publish Error, Non-zero Address. / The modules in the package must have their self-addresses set to zero. |
| sui_move_verification_error | [google.protobuf.Empty](#google-protobuf-Empty) |  | Sui Move Bytecode Verification Error. |
| move_primitive_runtime_error | [MoveError](#sui-types-MoveError) |  | MoveVm Errors

/ Error from a non-abort instruction. / Possible causes: / Arithmetic error, stack overflow, max value depth, etc.&#34; |
| move_abort | [MoveError](#sui-types-MoveError) |  | Move runtime abort |
| vm_verification_or_deserialization_error | [google.protobuf.Empty](#google-protobuf-Empty) |  | Bytecode verification error. |
| vm_invariant_violation | [google.protobuf.Empty](#google-protobuf-Empty) |  | MoveVm invariant violation |
| function_not_found | [google.protobuf.Empty](#google-protobuf-Empty) |  | Programmable Transaction Errors

/ Function not found |
| arity_mismatch | [google.protobuf.Empty](#google-protobuf-Empty) |  | Arity mismatch for Move function. / The number of arguments does not match the number of parameters |
| type_arity_mismatch | [google.protobuf.Empty](#google-protobuf-Empty) |  | Type arity mismatch for Move function. / Mismatch between the number of actual versus expected type arguments. |
| non_entry_function_invoked | [google.protobuf.Empty](#google-protobuf-Empty) |  | Non Entry Function Invoked. Move Call must start with an entry function. |
| command_argument_error | [CommandArgumentError](#sui-types-CommandArgumentError) |  | Invalid command argument |
| type_argument_error | [TypeArgumentError](#sui-types-TypeArgumentError) |  | Type argument error |
| unused_value_without_drop | [NestedResult](#sui-types-NestedResult) |  | Unused result without the drop ability. |
| invalid_public_function_return_type | [uint32](#uint32) |  | Invalid public Move function signature. / Unsupported return type for return value |
| invalid_transfer_object | [google.protobuf.Empty](#google-protobuf-Empty) |  | Invalid Transfer Object, object does not have public transfer. |
| effects_too_large | [SizeError](#sui-types-SizeError) |  | Post-execution errors

/ Effects from the transaction are too large |
| publish_upgrade_missing_dependency | [google.protobuf.Empty](#google-protobuf-Empty) |  | Publish or Upgrade is missing dependency |
| publish_upgrade_dependency_downgrade | [google.protobuf.Empty](#google-protobuf-Empty) |  | Publish or Upgrade dependency downgrade. / / Indirect (transitive) dependency of published or upgraded package has been assigned an / on-chain version that is less than the version required by one of the package&#39;s / transitive dependencies. |
| package_upgrade_error | [PackageUpgradeError](#sui-types-PackageUpgradeError) |  | Invalid package upgrade |
| written_objects_too_large | [SizeError](#sui-types-SizeError) |  | Indicates the transaction tried to write objects too large to storage |
| certificate_denied | [google.protobuf.Empty](#google-protobuf-Empty) |  | Certificate is on the deny list |
| sui_move_verification_timedout | [google.protobuf.Empty](#google-protobuf-Empty) |  | Sui Move Bytecode verification timed out. |
| shared_object_operation_not_allowed | [google.protobuf.Empty](#google-protobuf-Empty) |  | The requested shared object operation is not allowed |
| input_object_deleted | [google.protobuf.Empty](#google-protobuf-Empty) |  | Requested shared object has been deleted |
| execution_cancelled_due_to_shared_object_congestion | [CongestedObjectsError](#sui-types-CongestedObjectsError) |  | Certificate is cancelled due to congestion on shared objects |
| address_denied_for_coin | [AddressDeniedForCoinError](#sui-types-AddressDeniedForCoinError) |  | Address is denied for this coin type |
| coin_type_global_pause | [string](#string) |  | Coin type is globally paused for use |
| execution_cancelled_due_to_randomness_unavailable | [google.protobuf.Empty](#google-protobuf-Empty) |  | Certificate is cancelled because randomness could not be generated this epoch |






<a name="sui-types-GasCostSummary"></a>

### GasCostSummary



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| computation_cost | [uint64](#uint64) | optional |  |
| storage_cost | [uint64](#uint64) | optional |  |
| storage_rebate | [uint64](#uint64) | optional |  |
| non_refundable_storage_fee | [uint64](#uint64) | optional |  |






<a name="sui-types-GasPayment"></a>

### GasPayment



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| objects | [ObjectReference](#sui-types-ObjectReference) | repeated |  |
| owner | [Address](#sui-types-Address) | optional |  |
| price | [uint64](#uint64) | optional |  |
| budget | [uint64](#uint64) | optional |  |






<a name="sui-types-GenesisObject"></a>

### GenesisObject



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| object_id | [ObjectId](#sui-types-ObjectId) | optional |  |
| version | [uint64](#uint64) | optional |  |
| owner | [Owner](#sui-types-Owner) | optional |  |
| object | [ObjectData](#sui-types-ObjectData) | optional |  |






<a name="sui-types-GenesisTransaction"></a>

### GenesisTransaction



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| objects | [GenesisObject](#sui-types-GenesisObject) | repeated |  |






<a name="sui-types-I128"></a>

### I128
Little-endian encoded i128


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| bytes | [bytes](#bytes) | optional |  |






<a name="sui-types-Identifier"></a>

### Identifier



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| identifier | [string](#string) | optional |  |






<a name="sui-types-Input"></a>

### Input



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| pure | [bytes](#bytes) |  |  |
| immutable_or_owned | [ObjectReference](#sui-types-ObjectReference) |  |  |
| shared | [SharedObjectInput](#sui-types-SharedObjectInput) |  |  |
| receiving | [ObjectReference](#sui-types-ObjectReference) |  |  |






<a name="sui-types-Jwk"></a>

### Jwk



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| kty | [string](#string) | optional | Key type parameter, &lt;https://datatracker.ietf.org/doc/html/rfc7517#section-4.1&gt; |
| e | [string](#string) | optional | RSA public exponent, &lt;https://datatracker.ietf.org/doc/html/rfc7517#section-9.3&gt; |
| n | [string](#string) | optional | RSA modulus, &lt;https://datatracker.ietf.org/doc/html/rfc7517#section-9.3&gt; |
| alg | [string](#string) | optional | Algorithm parameter, &lt;https://datatracker.ietf.org/doc/html/rfc7517#section-4.4&gt; |






<a name="sui-types-JwkId"></a>

### JwkId



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| iss | [string](#string) | optional |  |
| kid | [string](#string) | optional |  |






<a name="sui-types-MakeMoveVector"></a>

### MakeMoveVector



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| element_type | [TypeTag](#sui-types-TypeTag) | optional |  |
| elements | [Argument](#sui-types-Argument) | repeated |  |






<a name="sui-types-MergeCoins"></a>

### MergeCoins



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| coin | [Argument](#sui-types-Argument) | optional |  |
| coins_to_merge | [Argument](#sui-types-Argument) | repeated |  |






<a name="sui-types-ModifiedAtVersion"></a>

### ModifiedAtVersion



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| object_id | [ObjectId](#sui-types-ObjectId) | optional |  |
| version | [uint64](#uint64) | optional |  |






<a name="sui-types-MoveCall"></a>

### MoveCall



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| package | [ObjectId](#sui-types-ObjectId) | optional |  |
| module | [Identifier](#sui-types-Identifier) | optional |  |
| function | [Identifier](#sui-types-Identifier) | optional |  |
| type_arguments | [TypeTag](#sui-types-TypeTag) | repeated |  |
| arguments | [Argument](#sui-types-Argument) | repeated |  |






<a name="sui-types-MoveError"></a>

### MoveError



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| location | [MoveLocation](#sui-types-MoveLocation) | optional |  |
| abort_code | [uint64](#uint64) | optional |  |






<a name="sui-types-MoveField"></a>

### MoveField



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| name | [Identifier](#sui-types-Identifier) | optional |  |
| value | [MoveValue](#sui-types-MoveValue) | optional |  |






<a name="sui-types-MoveLocation"></a>

### MoveLocation



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| package | [ObjectId](#sui-types-ObjectId) | optional |  |
| module | [Identifier](#sui-types-Identifier) | optional |  |
| function | [uint32](#uint32) | optional |  |
| instruction | [uint32](#uint32) | optional |  |
| function_name | [Identifier](#sui-types-Identifier) | optional |  |






<a name="sui-types-MoveModule"></a>

### MoveModule



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| name | [Identifier](#sui-types-Identifier) | optional |  |
| contents | [bytes](#bytes) | optional |  |






<a name="sui-types-MovePackage"></a>

### MovePackage



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| id | [ObjectId](#sui-types-ObjectId) | optional |  |
| version | [uint64](#uint64) | optional |  |
| modules | [MoveModule](#sui-types-MoveModule) | repeated |  |
| type_origin_table | [TypeOrigin](#sui-types-TypeOrigin) | repeated |  |
| linkage_table | [UpgradeInfo](#sui-types-UpgradeInfo) | repeated |  |






<a name="sui-types-MoveStruct"></a>

### MoveStruct



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| object_id | [ObjectId](#sui-types-ObjectId) | optional |  |
| object_type | [StructTag](#sui-types-StructTag) | optional |  |
| has_public_transfer | [bool](#bool) | optional |  |
| version | [uint64](#uint64) | optional |  |
| contents | [bytes](#bytes) | optional |  |






<a name="sui-types-MoveStructValue"></a>

### MoveStructValue



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| struct_type | [StructTag](#sui-types-StructTag) | optional |  |
| fields | [MoveField](#sui-types-MoveField) | repeated |  |






<a name="sui-types-MoveValue"></a>

### MoveValue



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| bool | [bool](#bool) |  |  |
| u8 | [uint32](#uint32) |  |  |
| u16 | [uint32](#uint32) |  |  |
| u32 | [uint32](#uint32) |  |  |
| u64 | [uint64](#uint64) |  |  |
| u128 | [U128](#sui-types-U128) |  |  |
| u256 | [U256](#sui-types-U256) |  |  |
| address | [Address](#sui-types-Address) |  |  |
| vector | [MoveVector](#sui-types-MoveVector) |  |  |
| struct | [MoveStructValue](#sui-types-MoveStructValue) |  |  |
| signer | [Address](#sui-types-Address) |  |  |
| variant | [MoveVariant](#sui-types-MoveVariant) |  |  |






<a name="sui-types-MoveVariant"></a>

### MoveVariant



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| enum_type | [StructTag](#sui-types-StructTag) | optional |  |
| variant_name | [Identifier](#sui-types-Identifier) | optional |  |
| tag | [uint32](#uint32) | optional |  |
| fields | [MoveField](#sui-types-MoveField) | repeated |  |






<a name="sui-types-MoveVector"></a>

### MoveVector



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| values | [MoveValue](#sui-types-MoveValue) | repeated |  |






<a name="sui-types-MultisigAggregatedSignature"></a>

### MultisigAggregatedSignature



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| signatures | [MultisigMemberSignature](#sui-types-MultisigMemberSignature) | repeated |  |
| bitmap | [uint32](#uint32) | optional |  |
| legacy_bitmap | [RoaringBitmap](#sui-types-RoaringBitmap) | optional |  |
| committee | [MultisigCommittee](#sui-types-MultisigCommittee) | optional |  |






<a name="sui-types-MultisigCommittee"></a>

### MultisigCommittee



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| members | [MultisigMember](#sui-types-MultisigMember) | repeated |  |
| threshold | [uint32](#uint32) | optional |  |






<a name="sui-types-MultisigMember"></a>

### MultisigMember



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| public_key | [MultisigMemberPublicKey](#sui-types-MultisigMemberPublicKey) | optional |  |
| weight | [uint32](#uint32) | optional |  |






<a name="sui-types-MultisigMemberPublicKey"></a>

### MultisigMemberPublicKey



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| ed25519 | [bytes](#bytes) |  |  |
| secp256k1 | [bytes](#bytes) |  |  |
| secp256r1 | [bytes](#bytes) |  |  |
| zklogin | [ZkLoginPublicIdentifier](#sui-types-ZkLoginPublicIdentifier) |  |  |






<a name="sui-types-MultisigMemberSignature"></a>

### MultisigMemberSignature



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| ed25519 | [bytes](#bytes) |  |  |
| secp256k1 | [bytes](#bytes) |  |  |
| secp256r1 | [bytes](#bytes) |  |  |
| zklogin | [ZkLoginAuthenticator](#sui-types-ZkLoginAuthenticator) |  |  |






<a name="sui-types-NestedResult"></a>

### NestedResult



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| result | [uint32](#uint32) | optional |  |
| subresult | [uint32](#uint32) | optional |  |






<a name="sui-types-Object"></a>

### Object



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| object_id | [ObjectId](#sui-types-ObjectId) | optional |  |
| version | [uint64](#uint64) | optional |  |
| owner | [Owner](#sui-types-Owner) | optional |  |
| object | [ObjectData](#sui-types-ObjectData) | optional |  |
| previous_transaction | [Digest](#sui-types-Digest) | optional |  |
| storage_rebate | [uint64](#uint64) | optional |  |






<a name="sui-types-ObjectData"></a>

### ObjectData



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| struct | [MoveStruct](#sui-types-MoveStruct) |  |  |
| package | [MovePackage](#sui-types-MovePackage) |  |  |






<a name="sui-types-ObjectExist"></a>

### ObjectExist



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| version | [uint64](#uint64) | optional |  |
| digest | [Digest](#sui-types-Digest) | optional |  |
| owner | [Owner](#sui-types-Owner) | optional |  |






<a name="sui-types-ObjectId"></a>

### ObjectId



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| object_id | [bytes](#bytes) | optional |  |






<a name="sui-types-ObjectReference"></a>

### ObjectReference



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| object_id | [ObjectId](#sui-types-ObjectId) | optional |  |
| version | [uint64](#uint64) | optional |  |
| digest | [Digest](#sui-types-Digest) | optional |  |






<a name="sui-types-ObjectReferenceWithOwner"></a>

### ObjectReferenceWithOwner



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| reference | [ObjectReference](#sui-types-ObjectReference) | optional |  |
| owner | [Owner](#sui-types-Owner) | optional |  |






<a name="sui-types-ObjectWrite"></a>

### ObjectWrite



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| digest | [Digest](#sui-types-Digest) | optional |  |
| owner | [Owner](#sui-types-Owner) | optional |  |






<a name="sui-types-Owner"></a>

### Owner



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| address | [Address](#sui-types-Address) |  |  |
| object | [ObjectId](#sui-types-ObjectId) |  |  |
| shared | [uint64](#uint64) |  |  |
| immutable | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |






<a name="sui-types-PackageIdDoesNotMatch"></a>

### PackageIdDoesNotMatch



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| package_id | [ObjectId](#sui-types-ObjectId) | optional |  |
| object_id | [ObjectId](#sui-types-ObjectId) | optional |  |






<a name="sui-types-PackageUpgradeError"></a>

### PackageUpgradeError



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| unable_to_fetch_package | [ObjectId](#sui-types-ObjectId) |  |  |
| not_a_package | [ObjectId](#sui-types-ObjectId) |  |  |
| incompatible_upgrade | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| digets_does_not_match | [Digest](#sui-types-Digest) |  |  |
| unknown_upgrade_policy | [uint32](#uint32) |  |  |
| package_id_does_not_match | [PackageIdDoesNotMatch](#sui-types-PackageIdDoesNotMatch) |  |  |






<a name="sui-types-PackageWrite"></a>

### PackageWrite



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| version | [uint64](#uint64) | optional |  |
| digest | [Digest](#sui-types-Digest) | optional |  |






<a name="sui-types-PasskeyAuthenticator"></a>

### PasskeyAuthenticator



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| authenticator_data | [bytes](#bytes) | optional |  |
| client_data_json | [string](#string) | optional |  |
| signature | [SimpleSignature](#sui-types-SimpleSignature) | optional |  |






<a name="sui-types-ProgrammableTransaction"></a>

### ProgrammableTransaction



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| inputs | [Input](#sui-types-Input) | repeated |  |
| commands | [Command](#sui-types-Command) | repeated |  |






<a name="sui-types-Publish"></a>

### Publish



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| modules | [bytes](#bytes) | repeated |  |
| dependencies | [ObjectId](#sui-types-ObjectId) | repeated |  |






<a name="sui-types-RandomnessStateUpdate"></a>

### RandomnessStateUpdate



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| epoch | [uint64](#uint64) | optional |  |
| randomness_round | [uint64](#uint64) | optional |  |
| random_bytes | [bytes](#bytes) | optional |  |
| randomness_object_initial_shared_version | [uint64](#uint64) | optional |  |






<a name="sui-types-ReadOnlyRoot"></a>

### ReadOnlyRoot



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| version | [uint64](#uint64) | optional |  |
| digest | [Digest](#sui-types-Digest) | optional |  |






<a name="sui-types-RoaringBitmap"></a>

### RoaringBitmap



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| bitmap | [bytes](#bytes) | optional |  |






<a name="sui-types-SharedObjectInput"></a>

### SharedObjectInput



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| object_id | [ObjectId](#sui-types-ObjectId) | optional |  |
| initial_shared_version | [uint64](#uint64) | optional |  |
| mutable | [bool](#bool) | optional |  |






<a name="sui-types-SimpleSignature"></a>

### SimpleSignature



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| scheme | [SignatureScheme](#sui-types-SignatureScheme) | optional |  |
| signature | [bytes](#bytes) | optional |  |
| public_key | [bytes](#bytes) | optional |  |






<a name="sui-types-SizeError"></a>

### SizeError



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| size | [uint64](#uint64) | optional |  |
| max_size | [uint64](#uint64) | optional |  |






<a name="sui-types-SplitCoins"></a>

### SplitCoins



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| coin | [Argument](#sui-types-Argument) | optional |  |
| amounts | [Argument](#sui-types-Argument) | repeated |  |






<a name="sui-types-StructTag"></a>

### StructTag



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| address | [Address](#sui-types-Address) | optional |  |
| module | [Identifier](#sui-types-Identifier) | optional |  |
| name | [Identifier](#sui-types-Identifier) | optional |  |
| type_parameters | [TypeTag](#sui-types-TypeTag) | repeated |  |






<a name="sui-types-SystemPackage"></a>

### SystemPackage



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| version | [uint64](#uint64) | optional |  |
| modules | [bytes](#bytes) | repeated |  |
| dependencies | [ObjectId](#sui-types-ObjectId) | repeated |  |






<a name="sui-types-Transaction"></a>

### Transaction



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| v1 | [Transaction.TransactionV1](#sui-types-Transaction-TransactionV1) |  |  |






<a name="sui-types-Transaction-TransactionV1"></a>

### Transaction.TransactionV1



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| kind | [TransactionKind](#sui-types-TransactionKind) | optional |  |
| sender | [Address](#sui-types-Address) | optional |  |
| gas_payment | [GasPayment](#sui-types-GasPayment) | optional |  |
| expiration | [TransactionExpiration](#sui-types-TransactionExpiration) | optional |  |






<a name="sui-types-TransactionEffects"></a>

### TransactionEffects



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| v1 | [TransactionEffectsV1](#sui-types-TransactionEffectsV1) |  |  |
| v2 | [TransactionEffectsV2](#sui-types-TransactionEffectsV2) |  |  |






<a name="sui-types-TransactionEffectsV1"></a>

### TransactionEffectsV1



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| status | [ExecutionStatus](#sui-types-ExecutionStatus) | optional | The status of the execution |
| epoch | [uint64](#uint64) | optional | The epoch when this transaction was executed. |
| gas_used | [GasCostSummary](#sui-types-GasCostSummary) | optional |  |
| modified_at_versions | [ModifiedAtVersion](#sui-types-ModifiedAtVersion) | repeated | The version that every modified (mutated or deleted) object had before it was modified by / this transaction. |
| shared_objects | [ObjectReference](#sui-types-ObjectReference) | repeated | The object references of the shared objects used in this transaction. Empty if no shared objects were used. |
| transaction_digest | [Digest](#sui-types-Digest) | optional | The transaction digest |
| created | [ObjectReferenceWithOwner](#sui-types-ObjectReferenceWithOwner) | repeated | ObjectReference and owner of new objects created. |
| mutated | [ObjectReferenceWithOwner](#sui-types-ObjectReferenceWithOwner) | repeated | ObjectReference and owner of mutated objects, including gas object. |
| unwrapped | [ObjectReferenceWithOwner](#sui-types-ObjectReferenceWithOwner) | repeated | ObjectReference and owner of objects that are unwrapped in this transaction. / Unwrapped objects are objects that were wrapped into other objects in the past, / and just got extracted out. |
| deleted | [ObjectReference](#sui-types-ObjectReference) | repeated | Object Refs of objects now deleted (the new refs). |
| unwrapped_then_deleted | [ObjectReference](#sui-types-ObjectReference) | repeated | Object refs of objects previously wrapped in other objects but now deleted. |
| wrapped | [ObjectReference](#sui-types-ObjectReference) | repeated | Object refs of objects now wrapped in other objects. |
| gas_object | [ObjectReferenceWithOwner](#sui-types-ObjectReferenceWithOwner) | optional | The updated gas object reference. Have a dedicated field for convenient access. / It&#39;s also included in mutated. |
| events_digest | [Digest](#sui-types-Digest) | optional | The digest of the events emitted during execution, / can be None if the transaction does not emit any event. |
| dependencies | [Digest](#sui-types-Digest) | repeated | The set of transaction digests this transaction depends on. |






<a name="sui-types-TransactionEffectsV2"></a>

### TransactionEffectsV2



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| status | [ExecutionStatus](#sui-types-ExecutionStatus) | optional | The status of the execution |
| epoch | [uint64](#uint64) | optional | The epoch when this transaction was executed. |
| gas_used | [GasCostSummary](#sui-types-GasCostSummary) | optional |  |
| transaction_digest | [Digest](#sui-types-Digest) | optional | The transaction digest |
| gas_object_index | [uint32](#uint32) | optional | The updated gas object reference, as an index into the `changed_objects` vector. / Having a dedicated field for convenient access. / System transaction that don&#39;t require gas will leave this as None. |
| events_digest | [Digest](#sui-types-Digest) | optional | The digest of the events emitted during execution, / can be None if the transaction does not emit any event. |
| dependencies | [Digest](#sui-types-Digest) | repeated | The set of transaction digests this transaction depends on. |
| lamport_version | [uint64](#uint64) | optional | The version number of all the written Move objects by this transaction. |
| changed_objects | [ChangedObject](#sui-types-ChangedObject) | repeated | Objects whose state are changed in the object store. |
| unchanged_shared_objects | [UnchangedSharedObject](#sui-types-UnchangedSharedObject) | repeated | Shared objects that are not mutated in this transaction. Unlike owned objects, / read-only shared objects&#39; version are not committed in the transaction, / and in order for a node to catch up and execute it without consensus sequencing, / the version needs to be committed in the effects. |
| auxiliary_data_digest | [Digest](#sui-types-Digest) | optional | Auxiliary data that are not protocol-critical, generated as part of the effects but are stored separately. / Storing it separately allows us to avoid bloating the effects with data that are not critical. / It also provides more flexibility on the format and type of the data. |






<a name="sui-types-TransactionEvents"></a>

### TransactionEvents



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| events | [Event](#sui-types-Event) | repeated |  |






<a name="sui-types-TransactionExpiration"></a>

### TransactionExpiration



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| none | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| epoch | [uint64](#uint64) |  |  |






<a name="sui-types-TransactionKind"></a>

### TransactionKind



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| programmable_transaction | [ProgrammableTransaction](#sui-types-ProgrammableTransaction) |  |  |
| change_epoch | [ChangeEpoch](#sui-types-ChangeEpoch) |  |  |
| genesis | [GenesisTransaction](#sui-types-GenesisTransaction) |  |  |
| consensus_commit_prologue_v1 | [ConsensusCommitPrologue](#sui-types-ConsensusCommitPrologue) |  |  |
| authenticator_state_update | [AuthenticatorStateUpdate](#sui-types-AuthenticatorStateUpdate) |  |  |
| end_of_epoch | [EndOfEpochTransaction](#sui-types-EndOfEpochTransaction) |  |  |
| randomness_state_update | [RandomnessStateUpdate](#sui-types-RandomnessStateUpdate) |  |  |
| consensus_commit_prologue_v2 | [ConsensusCommitPrologue](#sui-types-ConsensusCommitPrologue) |  |  |
| consensus_commit_prologue_v3 | [ConsensusCommitPrologue](#sui-types-ConsensusCommitPrologue) |  |  |






<a name="sui-types-TransferObjects"></a>

### TransferObjects



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| objects | [Argument](#sui-types-Argument) | repeated |  |
| address | [Argument](#sui-types-Argument) | optional |  |






<a name="sui-types-TypeArgumentError"></a>

### TypeArgumentError



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| type_argument | [uint32](#uint32) | optional |  |
| type_not_found | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| constraint_not_satisfied | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |






<a name="sui-types-TypeOrigin"></a>

### TypeOrigin



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| module_name | [Identifier](#sui-types-Identifier) | optional |  |
| struct_name | [Identifier](#sui-types-Identifier) | optional |  |
| package_id | [ObjectId](#sui-types-ObjectId) | optional |  |






<a name="sui-types-TypeTag"></a>

### TypeTag



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| u8 | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| u16 | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| u32 | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| u64 | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| u128 | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| u256 | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| bool | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| address | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| signer | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |
| vector | [TypeTag](#sui-types-TypeTag) |  |  |
| struct | [StructTag](#sui-types-StructTag) |  |  |






<a name="sui-types-U128"></a>

### U128
Little-endian encoded u128


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| bytes | [bytes](#bytes) | optional |  |






<a name="sui-types-U256"></a>

### U256
Little-endian encoded u256


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| bytes | [bytes](#bytes) | optional |  |






<a name="sui-types-UnchangedSharedObject"></a>

### UnchangedSharedObject



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| object_id | [ObjectId](#sui-types-ObjectId) | optional |  |
| read_only_root | [ReadOnlyRoot](#sui-types-ReadOnlyRoot) |  |  |
| mutate_deleted | [uint64](#uint64) |  |  |
| read_deleted | [uint64](#uint64) |  |  |
| cancelled | [uint64](#uint64) |  |  |
| per_epoch_config | [google.protobuf.Empty](#google-protobuf-Empty) |  |  |






<a name="sui-types-Upgrade"></a>

### Upgrade



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| modules | [bytes](#bytes) | repeated |  |
| dependencies | [ObjectId](#sui-types-ObjectId) | repeated |  |
| package | [ObjectId](#sui-types-ObjectId) | optional |  |
| ticket | [Argument](#sui-types-Argument) | optional |  |






<a name="sui-types-UpgradeInfo"></a>

### UpgradeInfo



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| original_id | [ObjectId](#sui-types-ObjectId) | optional |  |
| upgraded_id | [ObjectId](#sui-types-ObjectId) | optional |  |
| upgraded_version | [uint64](#uint64) | optional |  |






<a name="sui-types-UserSignature"></a>

### UserSignature



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| simple | [SimpleSignature](#sui-types-SimpleSignature) |  |  |
| multisig | [MultisigAggregatedSignature](#sui-types-MultisigAggregatedSignature) |  |  |
| zklogin | [ZkLoginAuthenticator](#sui-types-ZkLoginAuthenticator) |  |  |
| passkey | [PasskeyAuthenticator](#sui-types-PasskeyAuthenticator) |  |  |






<a name="sui-types-ValidatorAggregatedSignature"></a>

### ValidatorAggregatedSignature



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| epoch | [uint64](#uint64) | optional |  |
| signature | [bytes](#bytes) | optional |  |
| bitmap | [RoaringBitmap](#sui-types-RoaringBitmap) | optional |  |






<a name="sui-types-ValidatorCommittee"></a>

### ValidatorCommittee



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| epoch | [uint64](#uint64) | optional |  |
| members | [ValidatorCommitteeMember](#sui-types-ValidatorCommitteeMember) | repeated |  |






<a name="sui-types-ValidatorCommitteeMember"></a>

### ValidatorCommitteeMember



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| public_key | [bytes](#bytes) | optional |  |
| stake | [uint64](#uint64) | optional |  |






<a name="sui-types-VersionAssignment"></a>

### VersionAssignment



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| object_id | [ObjectId](#sui-types-ObjectId) | optional |  |
| version | [uint64](#uint64) | optional |  |






<a name="sui-types-ZkLoginAuthenticator"></a>

### ZkLoginAuthenticator



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| inputs | [ZkLoginInputs](#sui-types-ZkLoginInputs) | optional |  |
| max_epoch | [uint64](#uint64) | optional |  |
| signature | [SimpleSignature](#sui-types-SimpleSignature) | optional |  |






<a name="sui-types-ZkLoginClaim"></a>

### ZkLoginClaim



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| value | [string](#string) | optional |  |
| index_mod_4 | [uint32](#uint32) | optional |  |






<a name="sui-types-ZkLoginInputs"></a>

### ZkLoginInputs



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| proof_points | [ZkLoginProof](#sui-types-ZkLoginProof) | optional |  |
| iss_base64_details | [ZkLoginClaim](#sui-types-ZkLoginClaim) | optional |  |
| header_base64 | [string](#string) | optional |  |
| address_seed | [Bn254FieldElement](#sui-types-Bn254FieldElement) | optional |  |






<a name="sui-types-ZkLoginProof"></a>

### ZkLoginProof



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| a | [CircomG1](#sui-types-CircomG1) | optional |  |
| b | [CircomG2](#sui-types-CircomG2) | optional |  |
| c | [CircomG1](#sui-types-CircomG1) | optional |  |






<a name="sui-types-ZkLoginPublicIdentifier"></a>

### ZkLoginPublicIdentifier



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| iss | [string](#string) | optional |  |
| address_seed | [Bn254FieldElement](#sui-types-Bn254FieldElement) | optional |  |





 


<a name="sui-types-SignatureScheme"></a>

### SignatureScheme
note: values do not match their bcs serialized values

| Name | Number | Description |
| ---- | ------ | ----------- |
| SIGNATURE_SCHEME_UNKNOWN | 0 |  |
| SIGNATURE_SCHEME_ED25519 | 1 |  |
| SIGNATURE_SCHEME_SECP256K1 | 2 |  |
| SIGNATURE_SCHEME_SECP256R1 | 3 |  |
| SIGNATURE_SCHEME_MULTISIG | 4 |  |
| SIGNATURE_SCHEME_BLS12381 | 5 |  |
| SIGNATURE_SCHEME_ZKLOGIN | 6 |  |
| SIGNATURE_SCHEME_PASSKEY | 7 |  |


 

 

 



<a name="google_protobuf_empty-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## google/protobuf/empty.proto



<a name="google-protobuf-Empty"></a>

### Empty
A generic empty message that you can re-use to avoid defining duplicated
empty messages in your APIs. A typical example is to use it as the request
or the response type of an API method. For instance:

    service Foo {
      rpc Bar(google.protobuf.Empty) returns (google.protobuf.Empty);
    }





 

 

 

 



<a name="google_protobuf_timestamp-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## google/protobuf/timestamp.proto



<a name="google-protobuf-Timestamp"></a>

### Timestamp
A Timestamp represents a point in time independent of any time zone
or calendar, represented as seconds and fractions of seconds at
nanosecond resolution in UTC Epoch time. It is encoded using the
Proleptic Gregorian Calendar which extends the Gregorian calendar
backwards to year one. It is encoded assuming all minutes are 60
seconds long, i.e. leap seconds are &#34;smeared&#34; so that no leap second
table is needed for interpretation. Range is from
0001-01-01T00:00:00Z to 9999-12-31T23:59:59.999999999Z.
By restricting to that range, we ensure that we can convert to
and from  RFC 3339 date strings.
See [https://www.ietf.org/rfc/rfc3339.txt](https://www.ietf.org/rfc/rfc3339.txt).

# Examples

Example 1: Compute Timestamp from POSIX `time()`.

    Timestamp timestamp;
    timestamp.set_seconds(time(NULL));
    timestamp.set_nanos(0);

Example 2: Compute Timestamp from POSIX `gettimeofday()`.

    struct timeval tv;
    gettimeofday(&amp;tv, NULL);

    Timestamp timestamp;
    timestamp.set_seconds(tv.tv_sec);
    timestamp.set_nanos(tv.tv_usec * 1000);

Example 3: Compute Timestamp from Win32 `GetSystemTimeAsFileTime()`.

    FILETIME ft;
    GetSystemTimeAsFileTime(&amp;ft);
    UINT64 ticks = (((UINT64)ft.dwHighDateTime) &lt;&lt; 32) | ft.dwLowDateTime;

    // A Windows tick is 100 nanoseconds. Windows epoch 1601-01-01T00:00:00Z
    // is 11644473600 seconds before Unix epoch 1970-01-01T00:00:00Z.
    Timestamp timestamp;
    timestamp.set_seconds((INT64) ((ticks / 10000000) - 11644473600LL));
    timestamp.set_nanos((INT32) ((ticks % 10000000) * 100));

Example 4: Compute Timestamp from Java `System.currentTimeMillis()`.

    long millis = System.currentTimeMillis();

    Timestamp timestamp = Timestamp.newBuilder().setSeconds(millis / 1000)
        .setNanos((int) ((millis % 1000) * 1000000)).build();


Example 5: Compute Timestamp from current time in Python.

    timestamp = Timestamp()
    timestamp.GetCurrentTime()

# JSON Mapping

In JSON format, the Timestamp type is encoded as a string in the
[RFC 3339](https://www.ietf.org/rfc/rfc3339.txt) format. That is, the
format is &#34;{year}-{month}-{day}T{hour}:{min}:{sec}[.{frac_sec}]Z&#34;
where {year} is always expressed using four digits while {month}, {day},
{hour}, {min}, and {sec} are zero-padded to two digits each. The fractional
seconds, which can go up to 9 digits (i.e. up to 1 nanosecond resolution),
are optional. The &#34;Z&#34; suffix indicates the timezone (&#34;UTC&#34;); the timezone
is required, though only UTC (as indicated by &#34;Z&#34;) is presently supported.

For example, &#34;2017-01-15T01:30:15.01Z&#34; encodes 15.01 seconds past
01:30 UTC on January 15, 2017.

In JavaScript, one can convert a Date object to this format using the
standard [toISOString()](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Date/toISOString]
method. In Python, a standard `datetime.datetime` object can be converted
to this format using [`strftime`](https://docs.python.org/2/library/time.html#time.strftime)
with the time format spec &#39;%Y-%m-%dT%H:%M:%S.%fZ&#39;. Likewise, in Java, one
can use the Joda Time&#39;s [`ISODateTimeFormat.dateTime()`](
http://www.joda.org/joda-time/apidocs/org/joda/time/format/ISODateTimeFormat.html#dateTime--)
to obtain a formatter capable of generating timestamps in this format.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| seconds | [int64](#int64) |  | Represents seconds of UTC time since Unix epoch 1970-01-01T00:00:00Z. Must be from 0001-01-01T00:00:00Z to 9999-12-31T23:59:59Z inclusive. |
| nanos | [int32](#int32) |  | Non-negative fractions of a second at nanosecond resolution. Negative second values with fractions must still have non-negative nanos values that count forward in time. Must be from 0 to 999,999,999 inclusive. |





 

 

 

 



## Scalar Value Types

| .proto Type | Notes | C++ | Java | Python | Go | C# | PHP | Ruby |
| ----------- | ----- | --- | ---- | ------ | -- | -- | --- | ---- |
| <a name="double" /> double |  | double | double | float | float64 | double | float | Float |
| <a name="float" /> float |  | float | float | float | float32 | float | float | Float |
| <a name="int32" /> int32 | Uses variable-length encoding. Inefficient for encoding negative numbers – if your field is likely to have negative values, use sint32 instead. | int32 | int | int | int32 | int | integer | Bignum or Fixnum (as required) |
| <a name="int64" /> int64 | Uses variable-length encoding. Inefficient for encoding negative numbers – if your field is likely to have negative values, use sint64 instead. | int64 | long | int/long | int64 | long | integer/string | Bignum |
| <a name="uint32" /> uint32 | Uses variable-length encoding. | uint32 | int | int/long | uint32 | uint | integer | Bignum or Fixnum (as required) |
| <a name="uint64" /> uint64 | Uses variable-length encoding. | uint64 | long | int/long | uint64 | ulong | integer/string | Bignum or Fixnum (as required) |
| <a name="sint32" /> sint32 | Uses variable-length encoding. Signed int value. These more efficiently encode negative numbers than regular int32s. | int32 | int | int | int32 | int | integer | Bignum or Fixnum (as required) |
| <a name="sint64" /> sint64 | Uses variable-length encoding. Signed int value. These more efficiently encode negative numbers than regular int64s. | int64 | long | int/long | int64 | long | integer/string | Bignum |
| <a name="fixed32" /> fixed32 | Always four bytes. More efficient than uint32 if values are often greater than 2^28. | uint32 | int | int | uint32 | uint | integer | Bignum or Fixnum (as required) |
| <a name="fixed64" /> fixed64 | Always eight bytes. More efficient than uint64 if values are often greater than 2^56. | uint64 | long | int/long | uint64 | ulong | integer/string | Bignum |
| <a name="sfixed32" /> sfixed32 | Always four bytes. | int32 | int | int | int32 | int | integer | Bignum or Fixnum (as required) |
| <a name="sfixed64" /> sfixed64 | Always eight bytes. | int64 | long | int/long | int64 | long | integer/string | Bignum |
| <a name="bool" /> bool |  | bool | boolean | boolean | bool | bool | boolean | TrueClass/FalseClass |
| <a name="string" /> string | A string must always contain UTF-8 encoded or 7-bit ASCII text. | string | String | str/unicode | string | string | string | String (UTF-8) |
| <a name="bytes" /> bytes | May contain any arbitrary sequence of bytes. | string | ByteString | str | []byte | ByteString | string | String (ASCII-8BIT) |

