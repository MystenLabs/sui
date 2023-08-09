// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { Navigate, useParams } from 'react-router-dom';

import { CompletedTransactions } from './CompletedTransactions';
import { QredoPendingTransactions } from './QredoPendingTransactions';
import FiltersPortal from '_components/filters-tags';
import { AccountType } from '_src/background/keyring/Account';
import { useActiveAccount } from '_src/ui/app/hooks/useActiveAccount';
import PageTitle from '_src/ui/app/shared/PageTitle';

function TransactionBlocksPage() {
	const activeAccount = useActiveAccount();
	const isQredoAccount = activeAccount?.type === AccountType.QREDO;
	const { status } = useParams();
	const isPendingTransactions = status === 'pending';
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
