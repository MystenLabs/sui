// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import FiltersPortal from '_components/filters-tags';
import { isQredoAccountSerializedUI } from '_src/background/accounts/QredoAccount';
import { useActiveAccount } from '_src/ui/app/hooks/useActiveAccount';
import { useUnlockedGuard } from '_src/ui/app/hooks/useUnlockedGuard';
import PageTitle from '_src/ui/app/shared/PageTitle';
import cl from 'clsx';
import { Navigate, useParams } from 'react-router-dom';

import { CompletedTransactions } from './CompletedTransactions';
import { QredoPendingTransactions } from './QredoPendingTransactions';

function TransactionBlocksPage() {
	const activeAccount = useActiveAccount();
	const isQredoAccount = !!(activeAccount && isQredoAccountSerializedUI(activeAccount));
	const { status } = useParams();
	const isPendingTransactions = status === 'pending';
	if (useUnlockedGuard()) {
		return null;
	}
	if (activeAccount && !isQredoAccount && isPendingTransactions) {
		return <Navigate to="/transactions" replace />;
	}
	return (
		<div className="flex flex-col flex-nowrap h-full overflow-x-visible">
			{isQredoAccount ? (
				<FiltersPortal
					tags={[
						{ name: 'Complete', link: 'transactions' },
						{
							name: 'Pending Transactions',
							link: 'transactions/pending',
						},
					]}
				/>
			) : null}
			<PageTitle title="Your Activity" />
			<div
				className={cl(
					'mt-5 flex-grow overflow-y-auto px-5 -mx-5 divide-y divide-solid divide-gray-45 divide-x-0',
					{ 'mb-4': isQredoAccount },
				)}
			>
				{isPendingTransactions ? <QredoPendingTransactions /> : <CompletedTransactions />}
			</div>
		</div>
	);
}

export default TransactionBlocksPage;
