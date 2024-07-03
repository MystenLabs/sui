# `@mysten/graphql-transport`

This package provides a `SuiTransport` that enables `SuiClient` to make requests using the RPC 2.0
(GraphQL) API instead of the JSON RPC API.

## Install

```bash
npm install --save @mysten/graphql-transport
```

## Setup

```ts
import { SuiClientGraphQLTransport } from '@mysten/graphql-transport';
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';

const client = new SuiClient({
	transport: new SuiClientGraphQLTransport({
		url: 'https://sui-testnet.mystenlabs.com/graphql',
		// When specified, the transport will fallback to JSON RPC for unsupported method and parameters
		fallbackFullNodeUrl: getFullnodeUrl('testnet'),
	}),
});
```

## Limitations

### Unsupported methods

The following methods are currently unsupported in SuiClientGraphQLTransport, and will either error,
or fallback to the JSON RPC API if a `fallbackFullNodeUrl` is provided:

- `subscribeTransaction`
- `subscribeEvents`
- `call`
- `getNetworkMetrics`
- `getMoveCallMetrics`
- `getAddressMetrics`
- `getEpochs`
- `dryRunTransactionBlock`
- `devInspectTransactionBlock`
- `executeTransactionBlock`

### Unsupported parameters

Some supported methods in `SuiClientGraphQLTransport` do not support the full set of parameters
available in the JSON RPC API.

If an unsupported parameter is used, the request will error, or fallback to JSON RPC API if a
`fallbackFullNodeUrl` is provided.

- `getOwnedObjects`:
  - missing the `MatchAll`, `MatchAny`, `MatchNone`, and `Version` filters
- `queryEvents`:
  - missing the `MoveEventField`, `Module`, `TimeRange`, `All`, `Any`, `And`, and `Or` filters

### Unsupported fields

- `queryTransactionBlocks`, `getTransactionBlock`, and `multiGetTransactionBlocks`
  - missing `messageVersion`, `eventsDigest`, `sharedObjects`, `unwrapped`, `wrapped`, and
    `unwrappedThenDeleted` in effects
  - missing `id` for `events`
- `getStakes` and `getStakesByIds`
  - missing `validatorAddress`
- `getLatestSuiSystemState`
  - missing `stakingPoolMappingsId`, `inactivePoolsId`, `pendingActiveValidatorsId`,
    `validatorCandidatesId`
  - missing `reportRecords` on validators
- `getCurrentEpoch`
  - missing `reportRecords` on validators
- `queryEvents`
  - missing `id` for `events`
- `getCheckpoint` and `getCheckpoints`
  - missing `checkpointCommitments`
- `getCurrentEpoch`
  - missing `epochTotalTransactions`
- `getDynamicFields`
  - missing `objectId`, `digest` and `version` available for `DynamicObject` but not `DynamicField`

### Performance

Some may require multiple requests to properly resolve:

- `getDynamicFieldObject` requires 2 requests
- `queryTransactionBlocks`, `getTransactionBlock`, and `multiGetTransactionBlocks`
  - may require additional requests to load all `objectChanges`, `balanceChanges`, `dependencies`
    and `events`
- `getNormalizedMoveModule` and `getNormalizedMoveModulesByPage`
  - may require additional requests to load all `friends`, `functions`, and `structs`
- `getCheckpoint` and `getCheckpoints`,
  - may require additional requests to load all `transactionBlocks` and `validators`
- `getLatestSuiSystemState`, `getCurrentEpoch`, `getValidatorsApy` and `getCommitteeInfo`:
  - may require additional requests to load all `validators`

### Pagination

Page sizes and limits for paginated methods are based on the defaults and limits of the GraphQL API,
so page sizes and limits may be different than those returned by the JSON RPC API
