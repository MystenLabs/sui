// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#client
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { decodeSuiPrivateKey } from '@mysten/sui/cryptography';
import { deepbook, type DeepBookClient, type MarginManager } from '@mysten/deepbook-v3';
import type { ClientWithExtensions } from '@mysten/sui/client';

export type DeepBookMarginClient = ClientWithExtensions<{ deepbook: DeepBookClient }>;

export function getKeypair(privateKey: string): Ed25519Keypair {
	const { secretKey } = decodeSuiPrivateKey(privateKey);
	return Ed25519Keypair.fromSecretKey(secretKey);
}

// Testnet DeepBook client with margin enabled. On Testnet the SDK auto-loads the
// margin package IDs, margin pools, and Pyth config, so you reference pools,
// coins, and margin managers by key. Read-only calls (risk parameters, pool
// liquidity) work without a margin manager; borrowing and trading need one, so
// pass it under `marginManagers` once you have created it.
export function marginClient(
	address: string,
	marginManagers?: { [key: string]: MarginManager },
): DeepBookMarginClient {
	return new SuiGrpcClient({
		network: 'testnet',
		baseUrl: 'https://fullnode.testnet.sui.io:443',
	}).$extend(deepbook({ address, marginManagers }));
}
// docs::/#client
