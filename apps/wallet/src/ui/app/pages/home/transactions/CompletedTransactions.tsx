// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getTransactionDigest } from '@mysten/sui.js';

import Alert from '_components/alert';
import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import { TransactionCard } from '_components/transactions-card';
import { NoActivityCard } from '_components/transactions-card/NoActivityCard';
import { useQueryTransactionsByAddress } from '_hooks';
import { useActiveAddress } from '_src/ui/app/hooks/useActiveAddress';

export function CompletedTransactions() {
	const activeAddress = useActiveAddress();
	const { data: txns, isLoading, error } = useQueryTransactionsByAddress(activeAddress);
	if (error) {
		return <Alert>{(error as Error)?.message}</Alert>;
	}
	return (
		<Loading loading={isLoading}>
			{txns?.length && activeAddress ? (
				txns.map((txn) => (
					<ErrorBoundary key={getTransactionDigest(txn)}>
						<TransactionCard txn={txn} address={activeAddress} />
					</ErrorBoundary>
				))
			) : (
				<NoActivityCard message="When available, your Sui network transactions will show up here." />
			)}
		</Loading>
	);
}
