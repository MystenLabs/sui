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
	const a = {
		objectId: '0x3af52c25dae6749045b2ceed6ac0f57e21905981659d622bd21e414ed1bf9c00',
		version: '81729675',
		digest: '5k27zHiP2EhPErASYUHjcskQWYmZrNmJfGP38cxgWbvf',
		type: '0xf80e71b9fe884d3170e04f85ea91151f569e2077a909d05ac90d4cd497bc3037::SUIREWARD_W::SUIREWARD_PASS',
		display: {
			data: {
				description:
					'The DEEP token secures the DeepBook protocol, the premier wholesale liquidity venue for on-chain trading. Holders of this NFT will be able to convert it to DEEP tokens upon launch.',
				image_url:
					'https://suivision.mypinata.cloud/ipfs/Qmept9qMaZ6qXPP1MNwwZ9ftkUHJEQwpM8pPbgNsxbz5r4?pinataGatewayToken=XRz-H79S4Su_2PfKu9Ka-W7djbN8b0emIpVtsLxkbnebfobn-IIl-y6Elzyza7p-&img-fit=cover&img-quality=80&img-onerror=redirect&img-fit=pad&img-format=webp',
				name: 'cool beanz',
			},
			error: null,
		},
	};
	ownedAssets?.visual.push(a);
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
