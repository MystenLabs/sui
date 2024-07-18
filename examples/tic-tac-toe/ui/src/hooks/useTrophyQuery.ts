// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import { normalizeSuiAddress } from '@mysten/sui/utils';
import { useQuery, useQueryClient, UseQueryResult } from '@tanstack/react-query';
import { Game } from 'hooks/useGameQuery';
import { useTransactions } from 'hooks/useTransactions';

export enum Trophy {
	None = 0,
	Draw,
	Win,
}

export type UseTrophyQueryResponse = UseQueryResult<Trophy, Error>;
export type InvalidateTrophyQuery = () => void;

/** Refetch trophy status every 5 seconds */
const REFETCH_INTERVAL = 5000;

/**
 * Query the trophy state of the game (whether the game has a winner
 * or not).
 *
 * `id` is the Object ID of the game, and `kind` is what kind of game
 * it is (whether it is shared or owned). The query in this hook
 * depends on the value of `kind` (will not be enabled unless `kind`
 * is available).
 */
export function useTrophyQuery(game?: Game): [UseTrophyQueryResponse, InvalidateTrophyQuery] {
	const client = useSuiClient();
	const queryClient = useQueryClient();
	const tx = useTransactions()!!;

	const response = useQuery({
		enabled: !!game,
		refetchInterval: REFETCH_INTERVAL,
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: ['game-end-state', game?.id],
		queryFn: async () => {
			const { results } = await client.devInspectTransactionBlock({
				// It doesn't matter who's sending this query.
				sender: normalizeSuiAddress('0x0'),
				transactionBlock: tx.ended(game!!),
			});

			const trophy = results?.[0]?.returnValues?.[0]?.[0]?.[0];
			if (trophy === undefined) {
				throw new Error('Failed to get game state');
			}

			return trophy as Trophy;
		},
	});

	const invalidate = async () => {
		await queryClient.invalidateQueries({
			queryKey: ['game-end-state', game?.id],
		});
	};

	return [response, invalidate];
}
