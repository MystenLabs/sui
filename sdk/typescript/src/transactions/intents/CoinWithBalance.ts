// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/bcs';
import { bigint, object, parse, string } from 'valibot';

import type { CoinStruct, SuiClient } from '../../client/index.js';
import { normalizeStructTag } from '../../utils/sui-types.js';
import type { Argument } from '../blockData/internal.js';
import { Inputs } from '../Inputs.js';
import type { BuildTransactionBlockOptions } from '../json-rpc-resolver.js';
import { getClient } from '../json-rpc-resolver.js';
import type { TransactionBlock } from '../TransactionBlock.js';
import type { TransactionBlockDataBuilder } from '../TransactionBlockData.js';
import { Transactions } from '../Transactions.js';

const COIN_WITH_BALANCE = 'CoinWithBalance';
const SUI_TYPE = normalizeStructTag('0x2::sui::SUI');

export function coinWithBalance({
	type,
	balance,
	useGasCoin = true,
}: {
	type: string;
	balance: bigint | number;
	useGasCoin?: boolean;
}) {
	return (txb: TransactionBlock) => {
		txb.addIntentResolver(COIN_WITH_BALANCE, resolveCoinBalance);
		const coinType = type === 'gas' ? type : normalizeStructTag(type);

		return txb.add({
			$kind: 'Intent',
			Intent: {
				name: COIN_WITH_BALANCE,
				inputs: {},
				data: {
					type: coinType === SUI_TYPE && useGasCoin ? 'gas' : coinType,
					balance,
				},
			},
		});
	};
}

const CoinWithBalanceData = object({
	type: string(),
	balance: bigint(),
});

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
			const { type, balance } = parse(CoinWithBalanceData, transaction.Intent.data);

			if (type !== 'gas') {
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
		if (input.UnresolvedObject?.objectId) {
			usedIds.add(input.UnresolvedObject.objectId);
		}
	}

	const coinsByType = new Map<string, CoinStruct[]>();
	const client = getClient(buildOptions);
	await Promise.all(
		[...coinTypes].map(async (coinType) => {
			coinsByType.set(
				coinType,
				await getCoinsOfType({
					coinType,
					balance: totalByType.get(coinType)!,
					client,
					owner: blockData.sender!,
					usedIds,
				}),
			);
		}),
	);

	const mergedCoins = new Map<string, Argument>();

	mergedCoins.set('gas', { $kind: 'GasCoin', GasCoin: true });

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

async function getCoinsOfType({
	coinType,
	balance,
	client,
	owner,
	usedIds,
}: {
	coinType: string;
	balance: bigint;
	client: SuiClient;
	owner: string;
	usedIds: Set<string>;
}): Promise<CoinStruct[]> {
	let remainingBalance = balance;
	const coins: CoinStruct[] = [];

	return loadMoreCoins();

	async function loadMoreCoins(cursor: string | null = null): Promise<CoinStruct[]> {
		const { data, hasNextPage, nextCursor } = await client.getCoins({ owner, coinType, cursor });

		const sortedCoins = data.sort((a, b) => Number(BigInt(b.balance) - BigInt(a.balance)));

		for (const coin of sortedCoins) {
			if (usedIds.has(coin.coinObjectId)) {
				continue;
			}

			const coinBalance = BigInt(coin.balance);

			coins.push(coin);
			remainingBalance -= coinBalance;

			if (remainingBalance <= 0) {
				return coins;
			}
		}

		if (hasNextPage) {
			return loadMoreCoins(nextCursor);
		}

		throw new Error(`Not enough coins of type ${coinType} to satisfy requested balance`);
	}
}
