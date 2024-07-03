// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { hasDisplayData, isKioskOwnerToken, useGetOwnedObjects } from '@mysten/core';
import { useKioskClient } from '@mysten/core/src/hooks/useKioskClient';
import { type SuiObjectData } from '@mysten/sui/client';
import { useMemo } from 'react';

import { useBuyNLargeAssets } from '../components/buynlarge/useBuyNLargeAssets';
import { useHiddenAssets } from '../pages/home/hidden-assets/HiddenAssetsProvider';

type OwnedAssets = {
	visual: SuiObjectData[];
	other: SuiObjectData[];
	hidden: SuiObjectData[];
};

export enum AssetFilterTypes {
	visual = 'visual',
	other = 'other',
}

export function useGetNFTs(address?: string | null) {
	const kioskClient = useKioskClient();
	const bnl = useBuyNLargeAssets();
	const {
		data,
		isPending,
		error,
		isError,
		isFetchingNextPage,
		hasNextPage,
		fetchNextPage,
		isLoading,
	} = useGetOwnedObjects(
		address,
		{
			MatchNone: [
				{ StructType: '0x2::coin::Coin' },
				...(bnl
					.filter((item) => !!item?.objectType)
					.map((item) => ({ StructType: item?.objectType })) as { StructType: string }[]),
			],
		},
		50,
	);
	const { hiddenAssetIds } = useHiddenAssets();

	const assets = useMemo(() => {
		const ownedAssets: OwnedAssets = {
			visual: [],
			other: [],
			hidden: [],
		};

		const groupedAssets = data?.pages
			.flatMap((page) => page.data)
			.filter((asset) => !hiddenAssetIds.includes(asset.data?.objectId!))
			.reduce((acc, curr) => {
				if (hasDisplayData(curr) || isKioskOwnerToken(kioskClient.network, curr))
					acc.visual.push(curr.data as SuiObjectData);
				if (!hasDisplayData(curr)) acc.other.push(curr.data as SuiObjectData);
				if (hiddenAssetIds.includes(curr.data?.objectId!))
					acc.hidden.push(curr.data as SuiObjectData);
				return acc;
			}, ownedAssets);

		bnl.forEach((item) => {
			if (item?.asset?.data) {
				groupedAssets?.visual.unshift(item.asset.data);
			}
		});

		return groupedAssets;
	}, [hiddenAssetIds, data?.pages, kioskClient.network, bnl]);

	return {
		data: assets,
		isLoading,
		hasNextPage,
		isFetchingNextPage,
		fetchNextPage,
		isPending: isPending,
		isError: isError,
		error,
	};
}
