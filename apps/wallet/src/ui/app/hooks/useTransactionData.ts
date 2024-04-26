// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { useSuiClient } from '@mysten/dapp-kit';
import { TransactionBlock } from '@mysten/sui/transactions';
import { SUI_TYPE_ARG } from '@mysten/sui/utils';
import { useQuery } from '@tanstack/react-query';

export function useTransactionData(sender?: string | null, transaction?: TransactionBlock | null) {
	const client = useSuiClient();
	return useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: ['transaction-data', transaction?.serialize()],
		queryFn: async () => {
			const clonedTransaction = TransactionBlock.from(transaction!);
			if (sender) {
				clonedTransaction.setSenderIfNotSet(sender);
			}
			// Build the transaction to bytes, which will ensure that the transaction data is fully populated:
			await clonedTransaction!.build({ client });
			return clonedTransaction!.getBlockData();
		},
		enabled: !!transaction,
	});
}

export function useTransactionGasBudget(
	sender?: string | null,
	transaction?: TransactionBlock | null,
) {
	const { data, ...rest } = useTransactionData(sender, transaction);

	const [formattedGas] = useFormatCoin(data?.gasData.budget, SUI_TYPE_ARG);

	return {
		data: formattedGas,
		...rest,
	};
}
