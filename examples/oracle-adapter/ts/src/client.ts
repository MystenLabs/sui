// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#client
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { SuiPythClient, SuiPriceServiceConnection } from '@pythnetwork/pyth-sui-js';
import { TESTNET } from './config.js';

// A Sui client, the Pyth onchain client (which builds the Wormhole-verify plus
// price-update commands), and a Hermes connection (which serves the signed
// offchain price updates). Pyth on Sui is a pull oracle: you fetch an update
// from Hermes and apply it onchain in the same transaction that reads it.
//
// `SuiPythClient` takes any `@mysten/sui` 2.x client, because it reads through
// the unified `.core` API rather than a transport-specific one. This example
// uses gRPC; a `SuiJsonRpcClient` works the same way.
export function suiClient(): SuiGrpcClient {
	return new SuiGrpcClient({ network: 'testnet', baseUrl: TESTNET.grpcUrl });
}

export function pythClient(sui: SuiGrpcClient): SuiPythClient {
	return new SuiPythClient(sui, TESTNET.pythStateId, TESTNET.wormholeStateId);
}

export function hermes(): SuiPriceServiceConnection {
	return new SuiPriceServiceConnection(TESTNET.hermesEndpoint);
}
// docs::/#client
