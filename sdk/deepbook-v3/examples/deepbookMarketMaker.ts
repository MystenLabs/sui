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

	// Example of a flash loan transaction
	// Borrow 1 DEEP from DEEP_SUI pool
	// Swap 0.5 DBUSDC for SUI in SUI_DBUSDC pool, pay with deep borrowed
	// Swap SUI back to DEEP
	// Return 1 DEEP to DEEP_SUI pool
	flashLoanExample = async (tx: Transaction) => {
		const borrowAmount = 1;
		const [deepCoin, flashLoan] = await tx.add(
			this.dbClient.flashLoans.borrowBaseAsset('DEEP_SUI', borrowAmount),
		);

		// Execute trade using borrowed DEEP
		const [baseOut, quoteOut, deepOut] = await tx.add(
			this.dbClient.deepBook.swapExactQuoteForBase({
				poolKey: 'SUI_DBUSDC',
				amount: 0.5,
				deepAmount: 1,
				minOut: 0,
				deepCoin: deepCoin,
			}),
		);

		tx.transferObjects([baseOut, quoteOut, deepOut], this.getActiveAddress());

		// Execute second trade to get back DEEP for repayment
		const [baseOut2, quoteOut2, deepOut2] = await tx.add(
			this.dbClient.deepBook.swapExactQuoteForBase({
				poolKey: 'DEEP_SUI',
				amount: 10,
				deepAmount: 0,
				minOut: 0,
			}),
		);

		tx.transferObjects([quoteOut2, deepOut2], this.getActiveAddress());

		// Return borrowed DEEP
		const loanRemain = await tx.add(
			this.dbClient.flashLoans.returnBaseAsset('DEEP_SUI', borrowAmount, baseOut2, flashLoan),
		);
		tx.transferObjects([loanRemain], this.getActiveAddress());
	};

	placeLimitOrderExample = async (tx: Transaction) => {
		tx.add(
			await this.dbClient.deepBook.placeLimitOrder({
				poolKey: 'SUI_DBUSDC',
				balanceManagerKey: 'MANAGER_1',
				clientOrderId: 888,
				price: 1,
				quantity: 10,
				isBid: false,
				// orderType default: no restriction
				// selfMatchingOption default: allow self matching
				// payWithDeep default: true
			}),
		);
	};
}
