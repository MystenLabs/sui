// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#client
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { decodeSuiPrivateKey } from '@mysten/sui/cryptography';
import { deepbook, type DeepBookClient, type BalanceManager } from '@mysten/deepbook-v3';
import type { ClientWithExtensions } from '@mysten/sui/client';

export type DeepBookTestnetClient = ClientWithExtensions<{ deepbook: DeepBookClient }>;

export function getKeypair(privateKey: string): Ed25519Keypair {
	const { secretKey } = decodeSuiPrivateKey(privateKey);
	return Ed25519Keypair.fromSecretKey(secretKey);
}

// Testnet DeepBook client. The SDK ships Testnet package, coin, and pool
// constants, so you reference pools and coins by key (for example 'DEEP_SUI' or
// 'DEEP') instead of hardcoding IDs. Read-only calls work without a manager.
export function deepbookClient(
	address: string,
	balanceManagers?: { [key: string]: BalanceManager },
): DeepBookTestnetClient {
	return new SuiGrpcClient({
		network: 'testnet',
		baseUrl: 'https://fullnode.testnet.sui.io:443',
	}).$extend(deepbook({ address, balanceManagers }));
}
// docs::/#client
