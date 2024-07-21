// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useBlockedObjectList } from '_app/hooks/useBlockedObjectList';
import Alert from '_components/alert';
import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import LoadingSpinner from '_components/loading/LoadingIndicator';
import { NFTDisplayCard } from '_components/nft-display';
import { ampli } from '_src/shared/analytics/ampli';
import { Button } from '_src/ui/app/shared/ButtonUI';
import PageTitle from '_src/ui/app/shared/PageTitle';
import { getKioskIdFromOwnerCap, isKioskOwnerToken, useMultiGetObjects } from '@mysten/core';
import { useKioskClient } from '@mysten/core/src/hooks/useKioskClient';
import { EyeClose16 } from '@mysten/icons';
import { normalizeStructTag } from '@mysten/sui/utils';
import { keepPreviousData } from '@tanstack/react-query';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { useHiddenAssets } from './HiddenAssetsProvider';

function HiddenNftsPage() {
	const { hiddenAssetIds, showAsset } = useHiddenAssets();
	const kioskClient = useKioskClient();
	const { data: blockedObjectList } = useBlockedObjectList();

	const { data, isLoading, isPending, isError, error } = useMultiGetObjects(
		hiddenAssetIds,
		{
			showDisplay: true,
			showType: true,
		},
		{ placeholderData: keepPreviousData },
	);

	const filteredAndSortedNfts = useMemo(() => {
		const hiddenNfts =
			data?.flatMap((data) => {
				return {
					data: data.data,
					display: data.data?.display?.data,
				};
			}) || [];

		return hiddenNfts
			?.filter((nft) => nft.data && hiddenAssetIds.includes(nft?.data?.objectId))
			.filter((nft) => {
				if (!nft.data?.type) {
					return true;
				}
				const normalizedType = normalizeStructTag(nft.data.type);
				return !blockedObjectList?.includes(normalizedType);
			})
			.sort((nftA, nftB) => {
				let nameA = nftA.display?.name || '';
				let nameB = nftB.display?.name || '';

				if (nameA < nameB) {
					return -1;
				} else if (nameA > nameB) {
					return 1;
				}
				return 0;
			});
	}, [hiddenAssetIds, data, blockedObjectList]);

	if (isLoading) {
		return (
			<div className="mt-1 flex w-full justify-center">
				<LoadingSpinner />
			</div>
		);
	}

	return (
		<div className="flex flex-1 flex-col flex-nowrap items-center gap-4">
			<PageTitle title="Hidden Assets" back="/nfts" />
			<Loading loading={isPending && Boolean(hiddenAssetIds.length)}>
				{isError ? (
					<Alert>
						<div>
							<strong>Sync error (data might be outdated)</strong>
						</div>
						<small>{(error as Error).message}</small>
					</Alert>
				) : null}
				{filteredAndSortedNfts?.length ? (
					<div className="flex flex-col w-full divide-y divide-solid divide-gray-40 divide-x-0 gap-2">
						{filteredAndSortedNfts.map((nft) => {
							const { objectId, type } = nft.data!;
							return (
								<div className="flex justify-between items-center pt-2 pr-1" key={objectId}>
									<Link
										to={
											isKioskOwnerToken(kioskClient.network, nft.data)
												? `/kiosk?${new URLSearchParams({
														kioskId: getKioskIdFromOwnerCap(nft.data!),
													})}`
												: `/nft-details?${new URLSearchParams({
														objectId,
													}).toString()}`
										}
										onClick={() => {
											ampli.clickedCollectibleCard({
												objectId,
												collectibleType: type!,
											});
										}}
										className="no-underline relative truncate"
									>
										<ErrorBoundary>
											<NFTDisplayCard objectId={objectId} size="xs" orientation="horizontal" />
										</ErrorBoundary>
									</Link>
									<div className="h-8 w-8">
										<Button
											variant="secondarySui"
											size="icon"
											onClick={() => {
												showAsset(objectId);
											}}
											after={<EyeClose16 />}
										/>
									</div>
								</div>
							);
						})}
					</div>
				) : (
					<div className="flex flex-1 items-center self-center text-caption font-semibold text-steel-darker">
						No Assets found
					</div>
				)}
			</Loading>
		</div>
	);
}

export default HiddenNftsPage;
