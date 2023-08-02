// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetKioskContents, useGetOwnedObjects } from '@mysten/core';
import { ViewList16, ViewSmallThumbnails16, ViewThumbnails16 } from '@mysten/icons';
import { Heading, IconButton, LoadingIndicator, RadioGroup, RadioGroupItem } from '@mysten/ui';
import clsx from 'clsx';
import { useMemo, useState } from 'react';

import OwnedObject from './OwnedObject';
import { OBJECT_VIEW_MODES } from '~/ui/ObjectDetails';
import { Pagination, useCursorPagination } from '~/ui/Pagination';

const FILTER_OPTIONS = [
	{ label: 'NFTs', value: 'all' },
	{ label: 'Kiosks', value: 'kiosks' },
];

const VIEW_MODES = [
	{ icon: <ViewList16 />, value: OBJECT_VIEW_MODES.LIST },
	{ icon: <ViewSmallThumbnails16 />, value: OBJECT_VIEW_MODES.SMALL_THUMBNAILS },
	{ icon: <ViewThumbnails16 />, value: OBJECT_VIEW_MODES.THUMBNAILS },
];

const VIEW_FILTER_ELS = [
	{ label: '', icon: <ViewList16 />, value: 'list' },
	{ label: '', icon: <ViewSmallThumbnails16 />, value: 'smallthumbnails' },
	{ label: '', icon: <ViewList16 />, value: 'thumbnails' },
];

export function OwnedObjects({ id }: { id: string }) {
	const [filter, setFilter] = useState('all');
	const [viewMode, setViewMode] = useState(OBJECT_VIEW_MODES.LIST);
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
		<div className="flex h-full flex-col gap-4 pt-5">
			<div className='md:mt-12" flex w-full justify-between border-b border-gray-45 pb-3'>
				<div className="flex items-center gap-3">
					<Heading color="steel-darker" variant="heading4/semibold">
						Assets
					</Heading>
					<div className="flex items-center gap-1">
						{VIEW_MODES.map((mode) => {
							const selected = mode.value === viewMode;
							return (
								<div
									className={clsx(
										'flex h-6 w-6 items-center justify-center',
										selected ? 'text-white' : 'text-steel',
									)}
								>
									<IconButton
										className={clsx(
											'flex h-full w-full items-center justify-center rounded-md',
											selected ? 'bg-steel' : 'bg-white',
										)}
										aria-label="view-filter"
										children={mode.icon}
										onClick={() => {
											setViewMode(mode.value);
										}}
									/>
								</div>
							);
						})}
					</div>
				</div>

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
			</div>
			{isFetching ? (
				<LoadingIndicator />
			) : (
				<>
					<div className="flex h-full overflow-auto">
						<div className="flex h-full max-h-80 w-full flex-wrap">
							{filteredData?.map((obj) => (
								<div className="max-w-1/2 m-2 flex flex-grow">
									<OwnedObject viewMode={viewMode} obj={obj} key={obj?.data?.objectId} />
								</div>
							))}
						</div>
					</div>
					{filter !== 'kiosks' && <Pagination {...pagination} />}
				</>
			)}
		</div>
	);
}
