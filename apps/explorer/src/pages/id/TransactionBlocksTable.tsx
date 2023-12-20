// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { TabHeader } from '~/ui/Tabs';
import TransactionBlocksForAddress, {
	FILTER_VALUES,
	FiltersControl,
} from '~/components/TransactionBlocksForAddress';
import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { useState } from 'react';

export function TransactionBlocksTable({
	address,
	isObject,
}: {
	address: string;
	isObject: boolean;
}) {
	const [filterValue, setFilterValue] = useState(FILTER_VALUES.CHANGED);

	return (
		<TabHeader
			title="Transaction Blocks"
			after={
				isObject && (
					<div>
						<FiltersControl filterValue={filterValue} setFilterValue={setFilterValue} />
					</div>
				)
			}
		>
			<ErrorBoundary>
				<TransactionBlocksForAddress address={address} filter={filterValue} isObject={isObject} />
			</ErrorBoundary>
		</TabHeader>
	);
}
