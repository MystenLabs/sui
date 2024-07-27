// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { decodeSuiPrivateKey } from '@mysten/sui/cryptography';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Secp256k1Keypair } from '@mysten/sui/keypairs/secp256k1';
import { Secp256r1Keypair } from '@mysten/sui/keypairs/secp256r1';

export const getSignerFromPK = (privateKey: string) => {
	const { schema, secretKey } = decodeSuiPrivateKey(privateKey);
	if (schema === 'ED25519') return Ed25519Keypair.fromSecretKey(secretKey);
	if (schema === 'Secp256k1') return Secp256k1Keypair.fromSecretKey(secretKey);
	if (schema === 'Secp256r1') return Secp256r1Keypair.fromSecretKey(secretKey);

	throw new Error(`Unsupported schema: ${schema}`);
};
