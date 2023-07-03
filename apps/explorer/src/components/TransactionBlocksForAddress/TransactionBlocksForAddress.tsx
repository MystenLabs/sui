// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type TransactionFilter } from '@mysten/sui.js';
import { useState } from 'react';

import { genTableDataFromTxData } from '../transactions/TxCardUtils';
import {
	DEFAULT_TRANSACTIONS_LIMIT,
	useGetTransactionBlocks,
} from '~/hooks/useGetTransactionBlocks';
import { Heading } from '~/ui/Heading';
import { Pagination, useCursorPagination } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';

export enum ADDRESS_FILTER_VALUES {
	TO = 'ToAddress',
	FROM = 'FromAddress',
}

type TransactionBlocksForAddressProps = {
	address: string;
	filter?: ADDRESS_FILTER_VALUES;
	initialLimit?: number;
};

function TransactionBlocksForAddress({
	address,
	filter = ADDRESS_FILTER_VALUES.TO,
	initialLimit = DEFAULT_TRANSACTIONS_LIMIT,
}: TransactionBlocksForAddressProps) {
	const [limit, setLimit] = useState(initialLimit);

	const transactions = useGetTransactionBlocks(
		{
			[filter]: address,
		} as TransactionFilter,
		limit,
	);
	const { data, isFetching, pagination, isLoading } = useCursorPagination(transactions);

	const cardData = data ? genTableDataFromTxData(data.data) : undefined;

	return (
		<div data-testid="tx" className="w-full">
			<div className="flex flex-col space-y-5 pt-5 text-left md:pr-10">
				<div className="flex items-center justify-between border-b border-gray-45 pb-2 ">
					<Heading color="gray-90" variant="heading6/semibold">
						{filter === ADDRESS_FILTER_VALUES.TO ? 'Received' : 'Sent'}
					</Heading>
				</div>
				{isLoading || isFetching || !cardData ? (
					<PlaceholderTable
						rowCount={DEFAULT_TRANSACTIONS_LIMIT}
						rowHeight="16px"
						colHeadings={['Digest', 'Sender', 'Txns', 'Gas', 'Time']}
						colWidths={['30%', '30%', '10%', '20%', '10%']}
					/>
				) : (
					<div>
						<TableCard data={cardData.data} columns={cardData.columns} />
					</div>
				)}
				<div className="flex justify-between">
					<Pagination {...pagination} />
					<div className="flex items-center space-x-3">
						<select
							className="form-select rounded-md border border-gray-45 px-3 py-2 pr-8 text-bodySmall font-medium leading-[1.2] text-steel-dark shadow-button"
							value={limit}
							onChange={(e) => {
								setLimit(Number(e.target.value));
								pagination.onFirst();
							}}
						>
							<option value={20}>20 Per Page</option>
							<option value={40}>40 Per Page</option>
							<option value={60}>60 Per Page</option>
						</select>
					</div>
				</div>
			</div>
		</div>
	);
}

export default TransactionBlocksForAddress;
