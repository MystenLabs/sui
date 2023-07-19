// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetKioskContents, useGetOwnedObjects } from '@mysten/core';
import { LoadingIndicator, RadioGroup, RadioGroupItem } from '@mysten/ui';
import { useMemo, useState } from 'react';

import OwnedObject from './OwnedObject';
import { Pagination, useCursorPagination } from '~/ui/Pagination';

const FILTER_OPTIONS = [
	{ label: 'NFTs', value: 'all' },
	{ label: 'Kiosks', value: 'kiosks' },
];

export function OwnedObjects({ id }: { id: string }) {
	const [filter, setFilter] = useState('all');
	const ownedObjects = useGetOwnedObjects(id, {
		MatchNone: [{ StructType: '0x2::coin::Coin' }],
	});
	const { data: kioskData } = useGetKioskContents(id);

	const { data, isError, isFetching, pagination } = useCursorPagination(ownedObjects);

	const filteredData = useMemo(
		() => (filter === 'all' ? data?.data : kioskData?.list),
		[filter, data, kioskData],
	);

	if (isError) {
		return <div className="pt-2 font-sans font-semibold text-issue-dark">Failed to load NFTs</div>;
	}

	return (
		<div className="flex flex-col gap-4 pt-5">
			<RadioGroup
				aria-label="View transactions by a specific filter"
				value={filter}
				onValueChange={setFilter}
			>
				{FILTER_OPTIONS.map((filter) => (
					<RadioGroupItem
						key={filter.value}
						value={filter.value}
						label={filter.label}
						disabled={filter.value === 'kiosks' && !kioskData?.list?.length}
					/>
				))}
			</RadioGroup>
			{isFetching ? (
				<LoadingIndicator />
			) : (
				<>
					<div className="flex max-h-80 flex-col overflow-auto">
						<div className="grid grid-cols-1 gap-4 md:grid-cols-2">
							{filteredData?.map((obj) => <OwnedObject obj={obj} key={obj?.data?.objectId} />)}
						</div>
					</div>
					{filter !== 'kiosks' && <Pagination {...pagination} />}
				</>
			)}
		</div>
	);
}
