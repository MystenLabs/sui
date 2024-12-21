// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '../client/index.js';
import type { Signer } from '../cryptography/index.js';
import { Transaction } from '../transactions/index.js';

export interface MergeAllSuiCoinsOptions {
	client: SuiClient;
	signer: Signer;
}

/**
 * Merges all Sui coins owned by the signer into a single coin.
 *
 * This function can be used to merge coins when there are too many small or empty coins owned by the address for
 * the sdks default coins selection logic to work.
 *
 * This function retrieves all coins owned by the signer, sorts them in descending order based on their balance,
 * and merges them into the largest coin through multiple transactions. The largest coin is used as the initial gas coin.
 *
 * @example
 * const client = new SuiClient(...);
 * const signer = new Signer(...);
 * await mergeAllSuiCoins({ client, signer });
 */
export async function mergeAllSuiCoins({ client, signer }: MergeAllSuiCoinsOptions) {
	const owner = signer.toSuiAddress();
	const coins = [];
	let hasNextPage = true;
	let nextPageCursor: string | null = null;
	while (hasNextPage) {
		const res = await client.getCoins({
			coinType: '0x2::sui::SUI',
			owner,
			cursor: nextPageCursor,
		});

		coins.push(...res.data);
	}

	coins.sort((a, b) => Number(BigInt(b.balance) - BigInt(a.balance)));

	const largestCoin = coins.shift();

	if (!largestCoin) {
		throw new Error('No coins found');
	}

	let gasCoin = {
		objectId: largestCoin.coinObjectId,
		version: largestCoin.version,
		digest: largestCoin.digest,
	};

	while (coins.length > 0) {
		const coinsToMerge = coins.splice(0, 254).map((coin) => ({
			objectId: coin.coinObjectId,
			digest: coin.digest,
			version: coin.version,
		}));

		const transaction = new Transaction();
		transaction.setGasPayment([gasCoin, ...coinsToMerge]);

		const { effects } = await client.signAndExecuteTransaction({
			transaction,
			signer,
			options: {
				showEffects: true,
			},
		});

		if (effects?.status.error) {
			throw new Error(`Failed to merge coins: ${effects.status.error}`);
		}

		if (!effects?.gasObject.reference) {
			throw new Error('No gas coin reference found in effects');
		}

		gasCoin = effects?.gasObject.reference;
	}
}
