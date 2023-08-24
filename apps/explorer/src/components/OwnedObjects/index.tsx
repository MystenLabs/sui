// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetKioskContents, useGetOwnedObjects, useLocalStorage } from '@mysten/core';
import { ViewList16, ViewSmallThumbnails16 } from '@mysten/icons';
import { Heading, IconButton, RadioGroup, RadioGroupItem, Text } from '@mysten/ui';
import clsx from 'clsx';
import { useMemo, useState } from 'react';

import { ListView } from '~/components/OwnedObjects/ListView';
import { SmallThumbNailsView } from '~/components/OwnedObjects/SmallThumbNailsView';
import { OBJECT_VIEW_MODES } from '~/components/OwnedObjects/utils';
import { Pagination, useCursorPagination } from '~/ui/Pagination';

const PAGE_SIZES = [10, 20, 30, 40, 50];
const SHOW_PAGINATION_MAX_ITEMS = 9;
const OWNED_OBJECTS_LOCAL_STORAGE_VIEW_MODE = 'owned-objects-viewMode';

const FILTER_OPTIONS = [
	{ label: 'NFTS', value: 'all' },
	{ label: 'KIOSKS', value: 'kiosks' },
];

const VIEW_MODES = [
	{ icon: <ViewList16 />, value: OBJECT_VIEW_MODES.LIST },
	{ icon: <ViewSmallThumbnails16 />, value: OBJECT_VIEW_MODES.SMALL_THUMBNAILS },
];

function getItemsRangeFromCurrentPage(currentPage: number, itemsPerPage: number) {
	const start = currentPage * itemsPerPage + 1;
	const end = start + itemsPerPage - 1;
	return { start, end };
}

function getShowPagination(itemsLength: number, currentPage: number, isFetching: boolean) {
	if (isFetching) {
		return true;
	}

	return currentPage !== 0 || itemsLength > SHOW_PAGINATION_MAX_ITEMS;
}

export function OwnedObjects({ id }: { id: string }) {
	const [filter, setFilter] = useState('all');
	const [limit, setLimit] = useState(PAGE_SIZES[4]);
	const [viewMode, setViewMode] = useLocalStorage(
		OWNED_OBJECTS_LOCAL_STORAGE_VIEW_MODE,
		OBJECT_VIEW_MODES.SMALL_THUMBNAILS,
	);

	const ownedObjects = useGetOwnedObjects(
		id,
		{
			MatchNone: [{ StructType: '0x2::coin::Coin' }],
		},
		limit,
	);
	const { data: kioskData } = useGetKioskContents(id);

	const { data, isError, isFetching, pagination } = useCursorPagination(ownedObjects);

	const filteredData = useMemo(
		() => (filter === 'all' ? data?.data : kioskData?.list),
		[filter, data, kioskData],
	);

	const { start, end } = useMemo(
		() =>
			getItemsRangeFromCurrentPage(pagination.currentPage, filteredData?.length || PAGE_SIZES[0]),
		[filteredData?.length, pagination.currentPage],
	);

	const sortedDataByDisplayImages = useMemo(() => {
		if (!filteredData) {
			return [];
		}

		const hasImageUrl = [];
		const noImageUrl = [];

		for (const obj of filteredData) {
			const displayMeta = obj.data?.display?.data;

			if (displayMeta?.image_url) {
				hasImageUrl.push(obj);
			} else {
				noImageUrl.push(obj);
			}
		}

		return [...hasImageUrl, ...noImageUrl];
	}, [filteredData]);

	const showPagination = getShowPagination(
		filteredData?.length || 0,
		pagination.currentPage,
		isFetching,
	);

	if (isError) {
		return <div className="pt-2 font-sans font-semibold text-issue-dark">Failed to load NFTs</div>;
	}

	return (
		<div className="flex h-full overflow-hidden md:pl-10">
			<div className="flex h-full w-full flex-col gap-4">
				<div className="flex w-full flex-col items-start gap-3 border-b border-gray-45 max-sm:pb-3 sm:h-14 sm:min-h-14 sm:flex-row sm:items-center">
					<Heading color="steel-darker" variant="heading4/semibold">
						Assets
					</Heading>

					<div className="flex w-full flex-row-reverse justify-between sm:flex-row">
						<div className="flex items-center gap-1">
							{VIEW_MODES.map((mode) => {
								const selected = mode.value === viewMode;
								return (
									<div
										key={mode.value}
										className={clsx(
											'flex h-6 w-6 items-center justify-center',
											selected ? 'text-white' : 'text-steel',
										)}
									>
										<IconButton
											className={clsx(
												'flex h-full w-full items-center justify-center rounded',
												selected ? 'bg-steel' : 'bg-white',
											)}
											aria-label="view-filter"
											onClick={() => {
												setViewMode(mode.value);
											}}
										>
											{mode.icon}
										</IconButton>
									</div>
								);
							})}
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
				</div>

				{viewMode === OBJECT_VIEW_MODES.LIST && (
					<ListView loading={isFetching} data={sortedDataByDisplayImages} />
				)}
				{viewMode === OBJECT_VIEW_MODES.SMALL_THUMBNAILS && (
					<SmallThumbNailsView loading={isFetching} data={sortedDataByDisplayImages} />
				)}
				{showPagination && (
					<div className="mt-auto flex flex-row flex-wrap gap-2 md:mb-5">
						<Pagination {...pagination} />
						<div className="ml-auto flex items-center">
							{!isFetching && (
								<Text variant="body/medium" color="steel">
									Showing {start} - {end}
								</Text>
							)}
						</div>
						<div className="hidden sm:block">
							<select
								className="form-select rounded-md border border-gray-45 px-3 py-2 pr-8 text-bodySmall font-medium leading-[1.2] text-steel-dark shadow-button"
								value={limit}
								onChange={(e) => {
									setLimit(Number(e.target.value));
									pagination.onFirst();
								}}
							>
								{PAGE_SIZES.map((size) => (
									<option key={size} value={size}>
										{size} Per Page
									</option>
								))}
							</select>
						</div>
					</div>
				)}
			</div>
		</div>
	);
}
