// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { decodeSuiPrivateKey } from '@mysten/sui.js/cryptography';
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import type { Keypair } from '@mysten/sui/cryptography';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import type { Transaction } from '@mysten/sui/transactions';

import { DeepBookClient } from '../src/index.js'; // Adjust path according to new structure

export class DeepBookMarketMaker {
	dbClient: DeepBookClient;
	keypair: Keypair;
	suiClient: SuiClient;

	constructor(keypair: string | Keypair, env: 'testnet' | 'mainnet') {
		if (typeof keypair === 'string') {
			keypair = this.getSignerFromPK(keypair);
		}
		this.keypair = keypair;
		const suiClient = new SuiClient({
			url: getFullnodeUrl(env),
		});
		const address = keypair.toSuiAddress();
		this.dbClient = new DeepBookClient({
			address: address,
			env: env,
			client: suiClient,
		});
		this.suiClient = suiClient;
	}

	getSignerFromPK = (privateKey: string) => {
		const { schema, secretKey } = decodeSuiPrivateKey(privateKey);
		if (schema === 'ED25519') return Ed25519Keypair.fromSecretKey(secretKey);

		throw new Error(`Unsupported schema: ${schema}`);
	};

	signAndExecute = async (tx: Transaction) => {
		// remove arguments
		return this.suiClient.signAndExecuteTransaction({
			transaction: tx,
			signer: this.keypair,
			options: {
				showEffects: true,
				showObjectChanges: true,
			},
		});
	};

	getActiveAddress() {
		return this.keypair.getPublicKey().toSuiAddress();
	}
}
