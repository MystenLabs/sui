// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useActiveAddress } from '_app/hooks/useActiveAddress';
import { useBlockedObjectList } from '_app/hooks/useBlockedObjectList';
import Alert from '_components/alert';
import FiltersPortal from '_components/filters-tags';
import Loading from '_components/loading';
import LoadingSpinner from '_components/loading/LoadingIndicator';
import { setToSessionStorage } from '_src/background/storage-utils';
import { AssetFilterTypes, useGetNFTs } from '_src/ui/app/hooks/useGetNFTs';
import PageTitle from '_src/ui/app/shared/PageTitle';
import { useOnScreen } from '@mysten/core';
import { normalizeStructTag } from '@mysten/sui/utils';
import { useEffect, useMemo, useRef } from 'react';
import { useParams } from 'react-router-dom';

import { useHiddenAssets } from '../hidden-assets/HiddenAssetsProvider';
import AssetsOptionsMenu from './AssetsOptionsMenu';
import NonVisualAssets from './NonVisualAssets';
import VisualAssets from './VisualAssets';

function NftsPage() {
	const accountAddress = useActiveAddress();
	const { data: blockedObjectList } = useBlockedObjectList();
	const {
		data: ownedAssets,
		hasNextPage,
		isLoading,
		isFetchingNextPage,
		error,
		isPending,
		fetchNextPage,
		isError,
	} = useGetNFTs(accountAddress);
	const observerElem = useRef<HTMLDivElement | null>(null);
	const { isIntersecting } = useOnScreen(observerElem);
	const isSpinnerVisible = isFetchingNextPage && hasNextPage;

	useEffect(() => {
		if (isIntersecting && hasNextPage && !isFetchingNextPage) {
			fetchNextPage();
		}
	}, [isIntersecting, fetchNextPage, hasNextPage, isFetchingNextPage]);

	const handleFilterChange = async (tag: any) => {
		await setToSessionStorage<string>('NFTS_PAGE_NAVIGATION', tag.link);
	};
	const { filterType } = useParams();
	const filteredNFTs = useMemo(() => {
		let filteredData = ownedAssets?.visual;
		if (filterType) {
			filteredData = ownedAssets?.[filterType as AssetFilterTypes] ?? [];
		}
		return filteredData?.filter((ownedAsset) => {
			if (!ownedAsset.type) {
				return true;
			}
			const normalizedType = normalizeStructTag(ownedAsset.type);
			return !blockedObjectList?.includes(normalizedType);
		});
	}, [ownedAssets, filterType, blockedObjectList]);
	const { hiddenAssetIds } = useHiddenAssets();

	if (isLoading) {
		return (
			<div className="mt-1 flex w-full justify-center">
				<LoadingSpinner />
			</div>
		);
	}

	const tags = [
		{ name: 'Visual Assets', link: 'nfts' },
		{ name: 'Everything Else', link: 'nfts/other' },
	];

	return (
		<div className="flex min-h-full flex-col flex-nowrap items-center gap-4">
			<PageTitle title="Assets" after={hiddenAssetIds.length ? <AssetsOptionsMenu /> : null} />
			{!!ownedAssets?.other.length && (
				<FiltersPortal firstLastMargin tags={tags} callback={handleFilterChange} />
			)}
			<Loading loading={isPending}>
				{isError ? (
					<Alert>
						<div>
							<strong>Sync error (data might be outdated)</strong>
						</div>
						<small>{(error as Error).message}</small>
					</Alert>
				) : null}
				{filteredNFTs?.length ? (
					filterType === AssetFilterTypes.other ? (
						<NonVisualAssets items={filteredNFTs} />
					) : (
						<VisualAssets items={filteredNFTs} />
					)
				) : (
					<div className="flex flex-1 items-center self-center text-caption font-semibold text-steel-darker">
						No Assets found
					</div>
				)}
			</Loading>
			<div ref={observerElem}>
				{isSpinnerVisible ? (
					<div className="mt-1 flex w-full justify-center">
						<LoadingSpinner />
					</div>
				) : null}
			</div>
		</div>
	);
}

export default NftsPage;
