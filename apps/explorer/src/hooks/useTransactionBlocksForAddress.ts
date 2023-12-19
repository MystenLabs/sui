// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useQuery } from '@tanstack/react-query';
import { type SuiTransactionBlockResponse } from '@mysten/sui.js/client';
import { useSuiClient } from '@mysten/dapp-kit';

export function useTransactionBlocksForAddress(address: string, disabled?: boolean) {
	const client = useSuiClient();

	return useQuery({
		queryKey: ['transactions-for-address', address],
		queryFn: async () => {
			const filters = [{ ToAddress: address }, { FromAddress: address }];

			const results = await Promise.all(
				filters.map((filter) =>
					client.queryTransactionBlocks({
						filter,
						order: 'descending',
						limit: 100,
						options: {
							showEffects: true,
							showInput: true,
						},
					}),
				),
			);

			const inserted = new Map();
			const uniqueList: SuiTransactionBlockResponse[] = [];

			[...results[0].data, ...results[1].data]
				.sort((a, b) => Number(b.timestampMs ?? 0) - Number(a.timestampMs ?? 0))
				.forEach((txb) => {
					if (inserted.get(txb.digest)) return;
					uniqueList.push(txb);
					inserted.set(txb.digest, true);
				});

			return uniqueList;
		},
		enabled: !!address && !disabled,
	});
}
