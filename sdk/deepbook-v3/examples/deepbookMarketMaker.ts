// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import type { Keypair } from '@mysten/sui/cryptography';

import { DeepBookClient } from '../src/index.js'; // Adjust path according to new structure

export class DeepBookMarketMaker {
	dbClient: DeepBookClient;
	keypair: Keypair;
	suiClient: SuiClient;

	constructor(keypair: Keypair, env: 'testnet' | 'mainnet') {
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
}
