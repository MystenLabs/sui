# Protocol Documentation
<a name="top"></a>

## Table of Contents

- [narwhal.proto](#narwhal-proto)
    - [Batch](#narwhal-Batch)
    - [BatchDigest](#narwhal-BatchDigest)
    - [BatchMessage](#narwhal-BatchMessage)
    - [BincodeEncodedPayload](#narwhal-BincodeEncodedPayload)
    - [CertificateDigest](#narwhal-CertificateDigest)
    - [CollectionError](#narwhal-CollectionError)
    - [CollectionRetrievalResult](#narwhal-CollectionRetrievalResult)
    - [Empty](#narwhal-Empty)
    - [GetCollectionsRequest](#narwhal-GetCollectionsRequest)
    - [GetCollectionsResponse](#narwhal-GetCollectionsResponse)
    - [MultiAddr](#narwhal-MultiAddr)
    - [NewEpochRequest](#narwhal-NewEpochRequest)
    - [NewNetworkInfoRequest](#narwhal-NewNetworkInfoRequest)
    - [NodeReadCausalRequest](#narwhal-NodeReadCausalRequest)
    - [NodeReadCausalResponse](#narwhal-NodeReadCausalResponse)
    - [PrimaryAddresses](#narwhal-PrimaryAddresses)
    - [PublicKey](#narwhal-PublicKey)
    - [ReadCausalRequest](#narwhal-ReadCausalRequest)
    - [ReadCausalResponse](#narwhal-ReadCausalResponse)
    - [RemoveCollectionsRequest](#narwhal-RemoveCollectionsRequest)
    - [RoundsRequest](#narwhal-RoundsRequest)
    - [RoundsResponse](#narwhal-RoundsResponse)
    - [Transaction](#narwhal-Transaction)
    - [ValidatorData](#narwhal-ValidatorData)
  
    - [CollectionErrorType](#narwhal-CollectionErrorType)
  
    - [Configuration](#narwhal-Configuration)
    - [PrimaryToPrimary](#narwhal-PrimaryToPrimary)
    - [PrimaryToWorker](#narwhal-PrimaryToWorker)
    - [Proposer](#narwhal-Proposer)
    - [Transactions](#narwhal-Transactions)
    - [Validator](#narwhal-Validator)
    - [WorkerToPrimary](#narwhal-WorkerToPrimary)
    - [WorkerToWorker](#narwhal-WorkerToWorker)
  
- [Scalar Value Types](#scalar-value-types)



<a name="narwhal-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## narwhal.proto



<a name="narwhal-Batch"></a>

### Batch



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| transaction | [Transaction](#narwhal-Transaction) | repeated |  |






<a name="narwhal-BatchDigest"></a>

### BatchDigest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| digest | [bytes](#bytes) |  |  |






<a name="narwhal-BatchMessage"></a>

### BatchMessage



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| id | [BatchDigest](#narwhal-BatchDigest) |  |  |
| transactions | [Batch](#narwhal-Batch) |  |  |






<a name="narwhal-BincodeEncodedPayload"></a>

### BincodeEncodedPayload
A bincode encoded payload. This is intended to be used in the short-term
while we don&#39;t have good protobuf definitions for Narwhal types


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| payload | [bytes](#bytes) |  |  |






<a name="narwhal-CertificateDigest"></a>

### CertificateDigest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| digest | [bytes](#bytes) |  |  |






<a name="narwhal-CollectionError"></a>

### CollectionError



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| id | [CertificateDigest](#narwhal-CertificateDigest) |  |  |
| error | [CollectionErrorType](#narwhal-CollectionErrorType) |  |  |






<a name="narwhal-CollectionRetrievalResult"></a>

### CollectionRetrievalResult



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| batch | [BatchMessage](#narwhal-BatchMessage) |  |  |
| error | [CollectionError](#narwhal-CollectionError) |  |  |






<a name="narwhal-Empty"></a>

### Empty
Empty message for when we don&#39;t have anything to return






<a name="narwhal-GetCollectionsRequest"></a>

### GetCollectionsRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| collection_ids | [CertificateDigest](#narwhal-CertificateDigest) | repeated | List of collections to be retreived. |






<a name="narwhal-GetCollectionsResponse"></a>

### GetCollectionsResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| result | [CollectionRetrievalResult](#narwhal-CollectionRetrievalResult) | repeated | TODO: Revisit this for spec compliance. List of retrieval results of collections. |






<a name="narwhal-MultiAddr"></a>

### MultiAddr



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| address | [string](#string) |  |  |






<a name="narwhal-NewEpochRequest"></a>

### NewEpochRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| epoch_number | [uint32](#uint32) |  |  |
| validators | [ValidatorData](#narwhal-ValidatorData) | repeated |  |






<a name="narwhal-NewNetworkInfoRequest"></a>

### NewNetworkInfoRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| epoch_number | [uint32](#uint32) |  |  |
| validators | [ValidatorData](#narwhal-ValidatorData) | repeated |  |






<a name="narwhal-NodeReadCausalRequest"></a>

### NodeReadCausalRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| public_key | [PublicKey](#narwhal-PublicKey) |  |  |
| round | [uint64](#uint64) |  |  |






<a name="narwhal-NodeReadCausalResponse"></a>

### NodeReadCausalResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| collection_ids | [CertificateDigest](#narwhal-CertificateDigest) | repeated | Resulting sequence of collections from DAG walk. |






<a name="narwhal-PrimaryAddresses"></a>

### PrimaryAddresses



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| primary_to_primary | [MultiAddr](#narwhal-MultiAddr) |  |  |
| worker_to_primary | [MultiAddr](#narwhal-MultiAddr) |  |  |






<a name="narwhal-PublicKey"></a>

### PublicKey



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| bytes | [bytes](#bytes) |  |  |






<a name="narwhal-ReadCausalRequest"></a>

### ReadCausalRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| collection_id | [CertificateDigest](#narwhal-CertificateDigest) |  | A collection for which a sequence of related collections are to be retrieved. |






<a name="narwhal-ReadCausalResponse"></a>

### ReadCausalResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| collection_ids | [CertificateDigest](#narwhal-CertificateDigest) | repeated | Resulting sequence of collections from DAG walk. |






<a name="narwhal-RemoveCollectionsRequest"></a>

### RemoveCollectionsRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| collection_ids | [CertificateDigest](#narwhal-CertificateDigest) | repeated | List of collections to be removed. |






<a name="narwhal-RoundsRequest"></a>

### RoundsRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| public_key | [PublicKey](#narwhal-PublicKey) |  | The validator&#39;s key for which we want to retrieve / the available rounds. |






<a name="narwhal-RoundsResponse"></a>

### RoundsResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| oldest_round | [uint64](#uint64) |  | The oldest round for which the node has available / blocks to propose for the defined validator. |
| newest_round | [uint64](#uint64) |  | The newest (latest) round for which the node has available / blocks to propose for the defined validator. |






<a name="narwhal-Transaction"></a>

### Transaction



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| transaction | [bytes](#bytes) |  |  |






<a name="narwhal-ValidatorData"></a>

### ValidatorData



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| public_key | [PublicKey](#narwhal-PublicKey) |  |  |
| stake_weight | [int64](#int64) |  |  |
| primary_addresses | [PrimaryAddresses](#narwhal-PrimaryAddresses) |  |  |





 


<a name="narwhal-CollectionErrorType"></a>

### CollectionErrorType


| Name | Number | Description |
| ---- | ------ | ----------- |
| COLLECTION_NOT_FOUND | 0 |  |
| COLLECTION_TIMEOUT | 1 |  |
| COLLECTION_ERROR | 2 |  |


 

 


<a name="narwhal-Configuration"></a>

### Configuration


| Method Name | Request Type | Response Type | Description |
| ----------- | ------------ | ------------- | ------------|
| NewEpoch | [NewEpochRequest](#narwhal-NewEpochRequest) | [Empty](#narwhal-Empty) | Signals a new epoch |
| NewNetworkInfo | [NewNetworkInfoRequest](#narwhal-NewNetworkInfoRequest) | [Empty](#narwhal-Empty) | Signals a change in networking info |


<a name="narwhal-PrimaryToPrimary"></a>

### PrimaryToPrimary
The primary-to-primary interface

| Method Name | Request Type | Response Type | Description |
| ----------- | ------------ | ------------- | ------------|
| SendMessage | [BincodeEncodedPayload](#narwhal-BincodeEncodedPayload) | [Empty](#narwhal-Empty) | Sends a message |


<a name="narwhal-PrimaryToWorker"></a>

### PrimaryToWorker
The primary-to-worker interface

| Method Name | Request Type | Response Type | Description |
| ----------- | ------------ | ------------- | ------------|
| SendMessage | [BincodeEncodedPayload](#narwhal-BincodeEncodedPayload) | [Empty](#narwhal-Empty) | Sends a message |


<a name="narwhal-Proposer"></a>

### Proposer
The API that hosts the endpoints that should be used to help
/ proposing a block.

| Method Name | Request Type | Response Type | Description |
| ----------- | ------------ | ------------- | ------------|
| Rounds | [RoundsRequest](#narwhal-RoundsRequest) | [RoundsResponse](#narwhal-RoundsResponse) |  |
| NodeReadCausal | [NodeReadCausalRequest](#narwhal-NodeReadCausalRequest) | [NodeReadCausalResponse](#narwhal-NodeReadCausalResponse) | Returns the read_causal obtained by starting the DAG walk at the collection proposed by the input authority (as indicated by their public key) at the input round |


<a name="narwhal-Transactions"></a>

### Transactions


| Method Name | Request Type | Response Type | Description |
| ----------- | ------------ | ------------- | ------------|
| SubmitTransaction | [Transaction](#narwhal-Transaction) | [Empty](#narwhal-Empty) | Submit a Transactions |
| SubmitTransactionStream | [Transaction](#narwhal-Transaction) stream | [Empty](#narwhal-Empty) | Submit a Transactions |


<a name="narwhal-Validator"></a>

### Validator
The consensus to mempool interface for validator actions.

| Method Name | Request Type | Response Type | Description |
| ----------- | ------------ | ------------- | ------------|
| GetCollections | [GetCollectionsRequest](#narwhal-GetCollectionsRequest) | [GetCollectionsResponse](#narwhal-GetCollectionsResponse) | Returns collection contents for each requested collection. |
| RemoveCollections | [RemoveCollectionsRequest](#narwhal-RemoveCollectionsRequest) | [Empty](#narwhal-Empty) | Expunges collections from the mempool. |
| ReadCausal | [ReadCausalRequest](#narwhal-ReadCausalRequest) | [ReadCausalResponse](#narwhal-ReadCausalResponse) | Returns collections along a DAG walk with a well-defined starting point. |


<a name="narwhal-WorkerToPrimary"></a>

### WorkerToPrimary
The worker-to-primary interface

| Method Name | Request Type | Response Type | Description |
| ----------- | ------------ | ------------- | ------------|
| SendMessage | [BincodeEncodedPayload](#narwhal-BincodeEncodedPayload) | [Empty](#narwhal-Empty) | Sends a message |


<a name="narwhal-WorkerToWorker"></a>

### WorkerToWorker
The worker-to-worker interface

| Method Name | Request Type | Response Type | Description |
| ----------- | ------------ | ------------- | ------------|
| SendMessage | [BincodeEncodedPayload](#narwhal-BincodeEncodedPayload) | [Empty](#narwhal-Empty) | Sends a worker message |
| ClientBatchRequest | [BincodeEncodedPayload](#narwhal-BincodeEncodedPayload) | [BincodeEncodedPayload](#narwhal-BincodeEncodedPayload) stream | requests a number of batches that the service then streams back to the client |

 



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

