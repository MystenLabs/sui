## Overview
This package defines a number of APIs for interacting with services in the Sui
ecosystem. Some of the below API semantics and guidelines are inspired by
[Google's AIPs](https://google.aip.dev/) and apply to all services in this
package. For information on the individual services themselves, see their
definitions:

- [`LedgerService`](./ledger_service.proto)
- [`LiveDataService`](./live_data_service.proto)
- [`MovePackageService`](./move_package_service.proto.proto)
- [`SignatureVerificationService`](./signature_verification_service.proto)
- [`SubscriptionService`](./subscription_service.proto.proto)
- [`TransactionExecutionService`](./transaction_execution_service.proto.proto)

## Encoding
In order to improve usability of these APIs a number of identifiers are
encoding in messages in this package using their human-readable string
representations instead of a more compact bytes representation.

- `Address` and `ObjectId`: Represented as 64 hexadecimal characters with a
  leading `0x`.
- `Digest`s: Represented as
  [Base58](https://learnmeabitcoin.com/technical/keys/base58/).
- `TypeTag` and `StructTag`: Represented in their canonical string format (for
  example,
  `0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>`)

## Field Masks and Partial Responses
Some APIs may return resources that are either larger or expensive to compute
and the API may want to give the user control over which fields it returns.

In such circumstances Field Masks
[(google.protobuf.FieldMask)](../../../google/protobuf/field_mask.proto) can be
used for granting the user fine-grained control over what fields are returned.

For APIs where Field Masks are supported:

- The field masks must be a google.protobuf.FieldMask and should be named
  `read_mask`.
- The field mask parameter must be optional:
    - An explicit value of `"*"` should be supported, and must return all
      fields.
    - If the field mask parameter is omitted, it must default to `"*"`, unless
      otherwise documented.
- An API may allow read masks with non-terminal repeated fields (counter to the
  documentation on Field Masks).

## Pagination
Some APIs often need to provide collections of data. However, collections can
often be arbitrarily sized, and also often grow over time, increasing lookup
time as well as the size of the responses being sent over the wire. Therefore,
it is important that such APIs listing over a collection be paginated.

APIs returning collections of data must provide pagination at the outset, as it
is a backwards-incompatible change to add pagination to an existing method.

- Request messages for collections should define an `uint32 page_size` field,
  allowing users to specify the maximum number of results to return.
    - The `page_size` field must not be required.
    - If the user does not specify `page_size` (or specifies `0`), the API
      chooses an appropriate default, which the API should document. The API
      must not return an error.
    - If the user specifies `page_size` greater than the maximum permitted by
      the API, the API should coerce down to the maximum permitted page size.
    - The API may return fewer results than the number requested (including
      zero results), even if not at the end of the collection.
- Request messages for collections should define a `bytes page_token` field,
  allowing users to advance to the next page in the collection.
    - The `page_token` field must not be required.
    - If the user changes the `page_size` in a request for subsequent pages,
      the service must honor the new page size.
    - The user is expected to keep all other arguments to the RPC the same; if
      any arguments are different, the API should send an `INVALID_ARGUMENT`
      error.
- Response messages for collections should define a `bytes next_page_token`
  field, providing the user with a page token that may be used to retrieve the
  next page.
    - The field containing pagination results should be the first field in the
      message and have a field number of `1`. It should be a repeated field
      containing a list of resources constituting a single page of results.
    - If the end of the collection has been reached, the `next_page_token`
      field must be empty. This is the only way to communicate
      "end-of-collection" to users.
    - If the end of the collection has not been reached (or if the API can not
      determine in time), the API must provide a `next_page_token`.

## Field Presence
The proto files used in this package for message and service definitions are
all defined using protobuf version 3 (`proto3`). In `proto3`, fields that are
primitives (that is, they are not a `message`) and are not present on the wire
are zero-initialized. To gain the ability to detect [field
presence](https://github.com/protocolbuffers/protobuf/blob/main/docs/field_presence.md),
these definitions follow the convention of having all fields marked `optional`,
and wrapping `repeated` fields in a message as needed.

## Errors
The services defined in this package support the [richer error
model](https://grpc.io/docs/guides/error/#richer-error-model) following [AIP
193](https://google.aip.dev/193). In particular, when an RPC returns a response
with a non-OK status code, richer error information can generally be found by
looking at the `grpc-status-details-bin` header which contains a
[google.rpc.Status](https://github.com/googleapis/googleapis/blob/master/google/rpc/status.proto)
message encoded in Base64.

## HTTP Headers
Implementations of the services defined in this package should set the
following HTTP headers in responses where possible:

- `x-sui-chain-id`: Chain Id of the current chain
- `x-sui-chain`: Human-readable name of the current chain
- `x-sui-checkpoint-height`: Current checkpoint height
- `x-sui-lowest-available-checkpoint`: Lowest available checkpoint for which
  transaction and checkpoint data can be requested. Specifically this is the
  lowest checkpoint for which the following data can be requested: checkpoints,
  transactions, effects and events.
- `x-sui-lowest-available-checkpoint-objects`: Lowest available checkpoint for
  which object data can be requested. Specifically this is the lowest
  checkpoint for which input/output object data will be available.
- `x-sui-epoch`: Current epoch of the chain.
- `x-sui-timestamp-ms`: Current timestamp of the chain, represented as number
  of milliseconds from the Unix epoch.
- `x-sui-timestamp`: Current timestamp of the chain, encoded in the
  [RFC 3339](https://www.ietf.org/rfc/rfc3339.txt) format.
