// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type TransactionFilter } from '@mysten/sui.js/client';
import { Heading, RadioGroup, RadioGroupItem } from '@mysten/ui';
import { useReducer } from 'react';

import { genTableDataFromTxData } from '../transactions/TxCardUtils';
import {
	DEFAULT_TRANSACTIONS_LIMIT,
	useGetTransactionBlocks,
} from '~/hooks/useGetTransactionBlocks';
import { Pagination } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import clsx from 'clsx';

export enum FILTER_VALUES {
	INPUT = 'InputObject',
	CHANGED = 'ChangedObject',
}

type TransactionBlocksForAddressProps = {
	address: string;
	filter?: FILTER_VALUES;
	header?: string;
};

enum PAGE_ACTIONS {
	NEXT,
	PREV,
	FIRST,
}

type TransactionBlocksForAddressActionType = {
	type: PAGE_ACTIONS;
	filterValue: FILTER_VALUES;
};

type PageStateByFilterMap = {
	InputObject: number;
	ChangedObject: number;
};

const FILTER_OPTIONS = [
	{ label: 'Input Objects', value: 'InputObject' },
	{ label: 'Updated Objects', value: 'ChangedObject' },
];

const reducer = (state: PageStateByFilterMap, action: TransactionBlocksForAddressActionType) => {
	switch (action.type) {
		case PAGE_ACTIONS.NEXT:
			return {
				...state,
				[action.filterValue]: state[action.filterValue] + 1,
			};
		case PAGE_ACTIONS.PREV:
			return {
				...state,
				[action.filterValue]: state[action.filterValue] - 1,
			};
		case PAGE_ACTIONS.FIRST:
			return {
				...state,
				[action.filterValue]: 0,
			};
		default:
			return { ...state };
	}
};

export function FiltersControl({
	filterValue,
	setFilterValue,
}: {
	filterValue: string;
	setFilterValue: any;
}) {
	return (
		<RadioGroup
			aria-label="transaction filter"
			value={filterValue}
			onValueChange={(value) => setFilterValue(value as FILTER_VALUES)}
		>
			{FILTER_OPTIONS.map((filter) => (
				<RadioGroupItem key={filter.value} value={filter.value} label={filter.label} />
			))}
		</RadioGroup>
	);
}

function TransactionBlocksForAddress({
	address,
	filter = FILTER_VALUES.CHANGED,
	header,
}: TransactionBlocksForAddressProps) {
	// const [filterValue, setFilterValue] = useState(filter);
	const [currentPageState, dispatch] = useReducer(reducer, {
		InputObject: 0,
		ChangedObject: 0,
	});

	const { data, isPending, isFetching, isFetchingNextPage, fetchNextPage, hasNextPage } =
		useGetTransactionBlocks({
			[filter]: address,
		} as TransactionFilter);

	const currentPage = currentPageState[filter];
	const cardData =
		data && data.pages[currentPage]
			? genTableDataFromTxData(data.pages[currentPage].data)
			: undefined;

	return (
		<div data-testid="tx">
			{header && (
				<div className="flex items-center justify-between border-b border-gray-45 pb-5">
					<Heading color="gray-90" variant="heading4/semibold">
						{header}
					</Heading>
				</div>
			)}

			<div className={clsx(header && 'pt-5', 'flex flex-col space-y-5 text-left xl:pr-10')}>
				{isPending || isFetching || isFetchingNextPage || !cardData ? (
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

				{(hasNextPage || (data && data?.pages.length > 1)) && (
					<Pagination
						onNext={() => {
							if (isPending || isFetching) {
								return;
							}

							// Make sure we are at the end before fetching another page
							if (
								data &&
								currentPageState[filter] === data?.pages.length - 1 &&
								!isPending &&
								!isFetching
							) {
								fetchNextPage();
							}
							dispatch({
								type: PAGE_ACTIONS.NEXT,
								filterValue: filter,
							});
						}}
						hasNext={
							(Boolean(hasNextPage) && Boolean(data?.pages[currentPage])) ||
							currentPage < (data?.pages.length ?? 0) - 1
						}
						hasPrev={currentPageState[filter] !== 0}
						onPrev={() =>
							dispatch({
								type: PAGE_ACTIONS.PREV,
								filterValue: filter,
							})
						}
						onFirst={() =>
							dispatch({
								type: PAGE_ACTIONS.FIRST,
								filterValue: filter,
							})
						}
					/>
				)}
			</div>
		</div>
	);
}

export default TransactionBlocksForAddress;
