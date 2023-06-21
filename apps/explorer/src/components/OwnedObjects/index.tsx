// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetKioskContents, useGetOwnedObjects } from '@mysten/core';
import { useMemo, useState } from 'react';

import OwnedObject from './OwnedObject';

import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Pagination, useCursorPagination } from '~/ui/Pagination';
import { RadioGroup, RadioOption } from '~/ui/Radio';

const FILTER_OPTIONS = [
	{ label: 'NFTs', value: 'all' },
	{ label: 'Kiosks', value: 'kiosks' },
];

export function OwnedObjects({ id }: { id: string }) {
	const [filter, setFilter] = useState('all');
	const ownedObjects = useGetOwnedObjects(id, {
		MatchNone: [{ StructType: '0x2::coin::Coin' }],
	});
	const { data: kioskContents } = useGetKioskContents(id);

	const { data, isError, isFetching, pagination } = useCursorPagination(ownedObjects);

	const filteredData = useMemo(
		() => (filter === 'all' ? data?.data : kioskContents),
		[filter, data, kioskContents],
	);

	if (isError) {
		return <div className="pt-2 font-sans font-semibold text-issue-dark">Failed to load NFTs</div>;
	}

	return (
		<div className="flex flex-col gap-4 pt-5">
			<RadioGroup
				className="flex"
				ariaLabel="transaction filter"
				value={filter}
				onChange={setFilter}
			>
				{FILTER_OPTIONS.map((filter) => (
					<RadioOption
						key={filter.value}
						value={filter.value}
						label={filter.label}
						disabled={filter.value === 'kiosks' && !kioskContents?.length}
					/>
				))}
			</RadioGroup>
			{isFetching ? (
				<LoadingSpinner />
			) : (
				<>
					<div className="flex max-h-80 flex-col overflow-auto">
						<div className="grid grid-cols-1 gap-4 md:grid-cols-2">
							{filteredData?.map((obj) => (
								<OwnedObject obj={obj} key={obj?.data?.objectId} />
							))}
						</div>
					</div>
					{filter !== 'kiosks' && <Pagination {...pagination} />}
				</>
			)}
		</div>
	);
}
