// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { InferInput } from 'valibot';
import { bigint, object, parse, string } from 'valibot';

import type { CoinStruct, SuiClient } from '../../client/index.js';
import { normalizeStructTag } from '../../utils/sui-types.js';
import { Commands } from '../Commands.js';
import type { Argument } from '../data/internal.js';
import { Inputs } from '../Inputs.js';
import type { BuildTransactionOptions } from '../json-rpc-resolver.js';
import { getClient } from '../json-rpc-resolver.js';
import type { Transaction } from '../Transaction.js';
import type { TransactionDataBuilder } from '../TransactionData.js';

const SUI_TYPE = normalizeStructTag('0x2::sui::SUI');
const MERGE_ALL_COINS = 'MergeAllCoins';

export function coinWithBalance({
	type = SUI_TYPE,
	balance,
	useGasCoin = true,
}: {
	balance: bigint | number;
	type?: string;
	useGasCoin?: boolean;
}) {
	return (tx: Transaction) => {
		const coinType = normalizeStructTag(type);

		if (coinType === SUI_TYPE && useGasCoin) {
			return tx.splitCoins(tx.gas, [BigInt(balance)])[0];
		}

		return tx.splitCoins(mergeAllCoins({ type, requiredBalance: balance }), [BigInt(balance)])[0];
	};
}

export function mergeAllCoins({
	type,
	requiredBalance = 0,
}: {
	type: string;
	requiredBalance: bigint | number;
}) {
	return (tx: Transaction) => {
		tx.addIntentResolver(MERGE_ALL_COINS, resolveMergeAllCoins);

		return tx.add(
			Commands.Intent({
				name: MERGE_ALL_COINS,
				inputs: {},
				data: {
					type: normalizeStructTag(type),
					requiredBalance: BigInt(requiredBalance),
				} satisfies InferInput<typeof MergeAllCoinsData>,
			}),
		);
	};
}

const MergeAllCoinsData = object({
	type: string(),
	requiredBalance: bigint(),
});

async function resolveMergeAllCoins(
	transactionData: TransactionDataBuilder,
	buildOptions: BuildTransactionOptions,
	next: () => Promise<void>,
) {
	const coinTypes = new Set<string>();
	const totalByType = new Map<string, bigint>();

	if (!transactionData.sender) {
		throw new Error('Sender must be set to resolve mergeAllCoins intent');
	}

	for (const command of transactionData.commands) {
		if (command.$kind === '$Intent' && command.$Intent.name === MERGE_ALL_COINS) {
			const { type, requiredBalance } = parse(MergeAllCoinsData, command.$Intent.data);

			totalByType.set(type, (totalByType.get(type) ?? 0n) + requiredBalance);
		}
	}
	const usedIds = new Set<string>();

	for (const input of transactionData.inputs) {
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
					requiredBalance: totalByType.get(coinType)!,
					client,
					owner: transactionData.sender!,
					usedIds,
				}),
			);
		}),
	);

	const mergedCoins = new Map<string, Argument>();

	for (const [index, command] of transactionData.commands.entries()) {
		if (command.$kind !== '$Intent' || command.$Intent.name !== MERGE_ALL_COINS) {
			continue;
		}

		const { type } = parse(MergeAllCoinsData, command.$Intent.data);

		const commands = [];

		if (!mergedCoins.has(type)) {
			const [first, ...rest] = coinsByType.get(type)!.map((coin) =>
				transactionData.addInput(
					'object',
					Inputs.ObjectRef({
						objectId: coin.coinObjectId,
						digest: coin.digest,
						version: coin.version,
					}),
				),
			);

			if (rest.length > 0) {
				commands.push(Commands.MergeCoins(first, rest));
			}

			mergedCoins.set(type, first);
		}

		transactionData.replaceCommand(index, commands);

		transactionData.mapArguments((arg) => {
			if (arg.$kind === 'Result' && arg.Result === index) {
				return mergedCoins.get(type)!;
			}

			return arg;
		});
	}

	return next();
}

export async function getCoinsOfType({
	coinType,
	requiredBalance,
	client,
	owner,
	usedIds,
}: {
	coinType: string;
	requiredBalance: bigint;
	client: SuiClient;
	owner: string;
	usedIds: Set<string>;
}) {
	let totalBalance = 0n;
	const coins: CoinStruct[] = [];

	const result = await loadMoreCoins();

	if (result.totalBalance < requiredBalance) {
		throw new Error(`Not enough coins of type ${coinType} to satisfy requested balance`);
	}

	return result.coins;

	async function loadMoreCoins(
		cursor: string | null = null,
	): Promise<{ totalBalance: bigint; coins: CoinStruct[] }> {
		const { data, hasNextPage, nextCursor } = await client.getCoins({ owner, coinType, cursor });

		const sortedCoins = data.sort((a, b) => Number(BigInt(b.balance) - BigInt(a.balance)));

		for (const coin of sortedCoins) {
			if (usedIds.has(coin.coinObjectId)) {
				continue;
			}

			const coinBalance = BigInt(coin.balance);

			coins.push(coin);
			totalBalance += coinBalance;
		}

		if (hasNextPage) {
			return loadMoreCoins(nextCursor);
		}

		return {
			coins,
			totalBalance,
		};
	}
}
