# Sui Experimental (dapp) SDK

This package provides a lightweight, browser compatible SDK, originally built for dapp development.

## Feature set

- Local transaction building and signing thanks to BCS
- Supported in both NodeJS and browser environments
- Minimal set of dependencies: *tweetnacl*, *@mysten/bcs*, *js-sha3* and *bn.js*
- Object tracking - every state change or query that goes through `SuiClient` updates the object references storage

## Usage

```ts
import { SuiClient } from 'experimental';

// also possible: SuiClient.devnet();
// or:            SuiClient.local();
// or:        new SuiClient(gatewayUrl, fullNodeUrl);
const sui = SuiClient.devnet();

// ... example to be added here ...
```
