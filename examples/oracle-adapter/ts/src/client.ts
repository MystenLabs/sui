// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#client
import { SuiClient } from '@mysten/sui/client';
import { SuiPythClient, SuiPriceServiceConnection } from '@pythnetwork/pyth-sui-js';
import { TESTNET } from './config.js';

// A Sui RPC client, the Pyth on-chain client (which builds the Wormhole verify
// plus price-update commands), and a Hermes connection (which serves the signed
// off-chain price updates). Pyth on Sui is a pull oracle: you fetch an update
// from Hermes and apply it on-chain in the same transaction that reads it.
export function suiClient(): SuiClient {
	return new SuiClient({ url: TESTNET.rpcUrl });
}

export function pythClient(sui: SuiClient): SuiPythClient {
	return new SuiPythClient(sui, TESTNET.pythStateId, TESTNET.wormholeStateId);
}

export function hermes(): SuiPriceServiceConnection {
	return new SuiPriceServiceConnection(TESTNET.hermesEndpoint);
}
// docs::/#client
