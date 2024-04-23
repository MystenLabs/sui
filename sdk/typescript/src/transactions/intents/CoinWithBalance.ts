// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/bcs';

import type { CoinStruct } from '../../client/index.js';
import type { Argument } from '../blockData/v2.js';
import { Inputs } from '../Inputs.js';
import type { BuildTransactionBlockOptions } from '../json-rpc-resolver.js';
import { getClient } from '../json-rpc-resolver.js';
import type { TransactionBlock } from '../TransactionBlock.js';
import type { TransactionBlockDataBuilder } from '../TransactionBlockData.js';
import { Transactions } from '../Transactions.js';

const COIN_WITH_BALANCE = 'CoinWithBalance';

export function coinWithBalance(type: string, balance: bigint | number) {
	return (txb: TransactionBlock) => {
		txb.addIntentResolver(COIN_WITH_BALANCE, resolveCoinBalance);

		return txb.add({
			$kind: 'Intent',
			Intent: {
				name: COIN_WITH_BALANCE,
				inputs: {},
				data: {
					type,
					balance,
				},
			},
		});
	};
}

async function resolveCoinBalance(
	blockData: TransactionBlockDataBuilder,
	buildOptions: BuildTransactionBlockOptions,
	next: () => Promise<void>,
) {
	const coinTypes = new Set<string>();
	const totalByType = new Map<string, bigint>();

	if (!blockData.sender) {
		throw new Error('Sender must be set to resolve CoinWithBalance');
	}

	for (const transaction of blockData.transactions) {
		if (transaction.$kind === 'Intent' && transaction.Intent.name === COIN_WITH_BALANCE) {
			const { type, balance } = transaction.Intent.data as {
				type: string;
				balance: bigint;
			};

			if (type !== '0x2::sui::SUI') {
				coinTypes.add(type);
			}
			totalByType.set(type, (totalByType.get(type) ?? 0n) + balance);
		}
	}
	const usedIds = new Set<string>();

	for (const input of blockData.inputs) {
		if (input.Object?.ImmOrOwnedObject) {
			usedIds.add(input.Object.ImmOrOwnedObject.objectId);
		}
	}

	const coinsByType = new Map<string, CoinStruct[]>();
	const client = getClient(buildOptions);
	await Promise.all(
		[...coinTypes].map(async (coinType) => {
			const result = await client.getCoins({ owner: blockData.sender!, coinType });

			if (result.data.length === 0) {
				throw new Error(`No coins of type ${coinType} owned by ${blockData.sender}`);
			}

			coinsByType.set(
				coinType,
				result.data.filter((coin) => !usedIds.has(coin.coinObjectId)),
			);
		}),
	);

	const mergedCoins = new Map<string, Argument>();
	mergedCoins.set('0x2::sui::SUI', { $kind: 'GasCoin', GasCoin: true });

	for (const [index, transaction] of blockData.transactions.entries()) {
		if (transaction.$kind !== 'Intent' || transaction.Intent.name !== COIN_WITH_BALANCE) {
			continue;
		}

		const { type, balance } = transaction.Intent.data as {
			type: string;
			balance: bigint;
		};

		const transactions = [];

		if (!mergedCoins.has(type)) {
			const [first, ...rest] = coinsByType.get(type)!.map((coin) =>
				blockData.addInput(
					'object',
					Inputs.ObjectRef({
						objectId: coin.coinObjectId,
						digest: coin.digest,
						version: coin.version,
					}),
				),
			);

			if (rest.length > 0) {
				transactions.push(Transactions.MergeCoins(first, rest));
			}

			mergedCoins.set(type, first);
		}

		transactions.push(
			Transactions.SplitCoins(mergedCoins.get(type)!, [
				blockData.addInput('pure', Inputs.Pure(bcs.u64().serialize(balance))),
			]),
		);

		blockData.replaceTransaction(index, transactions);

		blockData.mapArguments((arg) => {
			if (arg.$kind === 'Result' && arg.Result === index) {
				return {
					$kind: 'NestedResult',
					NestedResult: [index + transactions.length - 1, 0],
				};
			}

			return arg;
		});
	}

	return next();
}
