// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type TransactionBlock } from '@mysten/sui.js/transactions';
import { useQuery } from '@tanstack/react-query';

import { useSigner } from '_hooks';

export function useTransactionDryRun(
	sender: string | undefined,
	transactionBlock: TransactionBlock,
) {
	const signer = useSigner(sender);
	const response = useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: ['dryRunTransaction', transactionBlock.serialize()],
		queryFn: () => {
			return signer!.dryRunTransactionBlock({ transactionBlock });
		},
		enabled: !!signer,
	});
	return response;
}
