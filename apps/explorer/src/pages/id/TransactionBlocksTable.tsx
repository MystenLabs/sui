// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { TabHeader } from '~/ui/Tabs';
import TransactionBlocksForAddress, {
	FILTER_VALUES,
	FiltersControl,
} from '~/components/TransactionBlocksForAddress';
import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { TransactionsForAddress } from '~/components/transactions/TransactionsForAddress';
import { useState } from 'react';

export function TransactionBlocksTable({
	pageType,
	address,
}: {
	pageType: 'Package' | 'Object' | 'Address';
	address: string;
}) {
	const [filterValue, setFilterValue] = useState(FILTER_VALUES.CHANGED);

	return (
		<TabHeader
			title="Transaction Blocks"
			after={
				pageType !== 'Address' && (
					<div>
						<FiltersControl filterValue={filterValue} setFilterValue={setFilterValue} />
					</div>
				)
			}
		>
			<ErrorBoundary>
				{pageType === 'Address' ? (
					<div data-testid="address-txn-table">
						<TransactionsForAddress type="address" address={address} />
					</div>
				) : (
					<div data-testid="object-txn-table">
						<TransactionBlocksForAddress address={address} filter={filterValue} />
					</div>
				)}
			</ErrorBoundary>
		</TabHeader>
	);
}
