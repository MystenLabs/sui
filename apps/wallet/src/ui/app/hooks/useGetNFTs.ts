// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetOwnedObjects, useGetKioskContents, hasDisplayData } from '@mysten/core';
import { type SuiObjectData, type SuiAddress } from '@mysten/sui.js';

import useAppSelector from './useAppSelector';

export function useGetNFTs(address?: SuiAddress | null) {
	const {
		data,
		isLoading,
		error,
		isError,
		isFetchingNextPage,
		hasNextPage,
		fetchNextPage,
		isInitialLoading,
	} = useGetOwnedObjects(
		address,
		{
			MatchNone: [{ StructType: '0x2::coin::Coin' }],
		},
		50,
	);
	const { apiEnv } = useAppSelector((state) => state.app);

	const disableOriginByteKiosk = apiEnv !== 'mainnet';
	const { data: kioskContents, isLoading: areKioskContentsLoading } = useGetKioskContents(
		address,
		disableOriginByteKiosk,
	);

	const filteredKioskContents = kioskContents
		?.filter(hasDisplayData)
		.map((data) => data.data as SuiObjectData);

	const nfts = [
		...(filteredKioskContents ?? []),
		...(data?.pages
			.flatMap((page) => page.data)
			.filter(hasDisplayData)
			.map(({ data }) => data as SuiObjectData) || []),
	];
	return {
		data: nfts,
		isInitialLoading,
		hasNextPage,
		isFetchingNextPage,
		fetchNextPage,
		isLoading: isLoading || areKioskContentsLoading,
		isError: isError,
		error,
	};
}
