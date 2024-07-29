// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import { decodeSuiPrivateKey } from '@mysten/sui/cryptography';
import type { Keypair } from '@mysten/sui/cryptography';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import type { Transaction } from '@mysten/sui/transactions';

import { DeepBookClient } from '../src/index.js'; // Adjust path according to new structure
import type { BalanceManager } from '../src/types/index.js';

export class DeepBookMarketMaker extends DeepBookClient {
	keypair: Keypair;
	suiClient: SuiClient;

	constructor(
		keypair: string | Keypair,
		env: 'testnet' | 'mainnet',
		balanceManagers?: { [key: string]: BalanceManager },
		adminCap?: string,
	) {
		let resolvedKeypair: Keypair;

		if (typeof keypair === 'string') {
			resolvedKeypair = DeepBookMarketMaker.#getSignerFromPK(keypair);
		} else {
			resolvedKeypair = keypair;
		}

		const address = resolvedKeypair.toSuiAddress();

		super({
			address: address,
			env: env,
			client: new SuiClient({
				url: getFullnodeUrl(env),
			}),
			balanceManagers: balanceManagers,
			adminCap: adminCap,
		});

		this.keypair = resolvedKeypair;
		this.suiClient = new SuiClient({
			url: getFullnodeUrl(env),
		});
	}

	static #getSignerFromPK = (privateKey: string) => {
		const { schema, secretKey } = decodeSuiPrivateKey(privateKey);
		if (schema === 'ED25519') return Ed25519Keypair.fromSecretKey(secretKey);

		throw new Error(`Unsupported schema: ${schema}`);
	};

	signAndExecute = async (tx: Transaction) => {
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
		const [deepCoin, flashLoan] = tx.add(this.flashLoans.borrowBaseAsset('DEEP_SUI', borrowAmount));

		// Execute trade using borrowed DEEP
		const [baseOut, quoteOut, deepOut] = tx.add(
			this.deepBook.swapExactQuoteForBase({
				poolKey: 'SUI_DBUSDC',
				amount: 0.5,
				deepAmount: 1,
				minOut: 0,
				deepCoin: deepCoin,
			}),
		);

		tx.transferObjects([baseOut, quoteOut, deepOut], this.getActiveAddress());

		// Execute second trade to get back DEEP for repayment
		const [baseOut2, quoteOut2, deepOut2] = tx.add(
			this.deepBook.swapExactQuoteForBase({
				poolKey: 'DEEP_SUI',
				amount: 10,
				deepAmount: 0,
				minOut: 0,
			}),
		);

		tx.transferObjects([quoteOut2, deepOut2], this.getActiveAddress());

		// Return borrowed DEEP
		const loanRemain = tx.add(
			this.flashLoans.returnBaseAsset('DEEP_SUI', borrowAmount, baseOut2, flashLoan),
		);
		tx.transferObjects([loanRemain], this.getActiveAddress());
	};

	placeLimitOrderExample = (tx: Transaction) => {
		tx.add(
			this.deepBook.placeLimitOrder({
				poolKey: 'SUI_DBUSDC',
				balanceManagerKey: 'MANAGER_1',
				clientOrderId: '123456789',
				price: 1,
				quantity: 10,
				isBid: true,
				// orderType default: no restriction
				// selfMatchingOption default: allow self matching
				// payWithDeep default: true
			}),
		);
	};
}
