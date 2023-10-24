// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import { NoActivityCard } from '_components/transactions-card/NoActivityCard';
import { isQredoAccountSerializedUI } from '_src/background/accounts/QredoAccount';
import { type TransactionStatus } from '_src/shared/qredo-api';
import Alert from '_src/ui/app/components/alert';
import { useActiveAccount } from '_src/ui/app/hooks/useActiveAccount';
import { useGetQredoTransactions } from '_src/ui/app/hooks/useGetQredoTransactions';

import { QredoTransaction } from './QredoTransaction';

const PENDING_QREDO_TRANSACTION_STATUSES: TransactionStatus[] = [
	'approved',
	'authorized',
	'created',
	'pending',
	'pushed',
	'scheduled',
	'signed',
];

export function QredoPendingTransactions() {
	const activeAccount = useActiveAccount();
	const activeAddress = activeAccount?.address;
	const isQredoAccount = !!(activeAccount && isQredoAccountSerializedUI(activeAccount));
	const qredoID = isQredoAccount ? activeAccount.sourceID : undefined;
	const {
		data: qredoTransactions,
		isPending,
		error,
	} = useGetQredoTransactions({
		qredoID,
		filterStatus: PENDING_QREDO_TRANSACTION_STATUSES,
	});
	if (error) {
		return <Alert>{(error as Error)?.message}</Alert>;
	}
	return (
		<Loading loading={isPending}>
			{qredoTransactions?.length && activeAddress ? (
				qredoTransactions.map((txn) => (
					<ErrorBoundary key={txn.txID}>
						<QredoTransaction qredoID={qredoID} qredoTransactionID={txn.txID} />
					</ErrorBoundary>
				))
			) : (
				<NoActivityCard message="When available, pending Qredo transactions will show up here." />
			)}
		</Loading>
	);
}
