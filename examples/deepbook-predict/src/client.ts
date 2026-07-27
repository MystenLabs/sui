// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#client
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { decodeSuiPrivateKey } from '@mysten/sui/cryptography';
import { PREDICT } from './config.js';

export function getKeypair(privateKey: string): Ed25519Keypair {
	const { secretKey } = decodeSuiPrivateKey(privateKey);
	return Ed25519Keypair.fromSecretKey(secretKey);
}

export const client = new SuiGrpcClient({
	network: PREDICT.network,
	baseUrl: 'https://fullnode.testnet.sui.io:443',
});
// docs::/#client
