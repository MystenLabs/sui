// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useOnScreen } from '@mysten/core';
import { Check12, EyeClose16 } from '@mysten/icons';
import { get, set } from 'idb-keyval';
import { useRef, useEffect, useCallback, useState, useMemo } from 'react';
import toast from 'react-hot-toast';
import { Link } from 'react-router-dom';

import AssetsOptionsMenu from './AssetsOptionsMenu';
import { Link as InlineLink } from '../../../shared/Link';
import { Text } from '../../../shared/text';
import { useActiveAddress } from '_app/hooks/useActiveAddress';
import Alert from '_components/alert';
import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import LoadingSpinner from '_components/loading/LoadingIndicator';
import { NFTDisplayCard } from '_components/nft-display';
import { ampli } from '_src/shared/analytics/ampli';
import { useGetNFTs } from '_src/ui/app/hooks/useGetNFTs';
import { Button } from '_src/ui/app/shared/ButtonUI';
import PageTitle from '_src/ui/app/shared/PageTitle';

const HIDDEN_ASSET_IDS = 'hidden-asset-ids';

function NftsPage() {
	const [internalHiddenAssetIds, internalSetHiddenAssetIds] = useState<string[]>([]);
	const accountAddress = useActiveAddress();
	const {
		data: nfts,
		hasNextPage,
		isInitialLoading,
		isFetchingNextPage,
		error,
		isLoading,
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
	}, [nfts.length, isIntersecting, fetchNextPage, hasNextPage, isFetchingNextPage]);

	useEffect(() => {
		(async () => {
			const hiddenAssets = await get<string[]>(HIDDEN_ASSET_IDS);
			if (hiddenAssets) {
				internalSetHiddenAssetIds(hiddenAssets);
			}
		})();
	}, []);

	const hideAssetId = useCallback(
		async (newAssetId: string) => {
			if (internalHiddenAssetIds.includes(newAssetId)) return;

			const newHiddenAssetIds = [...internalHiddenAssetIds, newAssetId];
			internalSetHiddenAssetIds(newHiddenAssetIds);
			await set(HIDDEN_ASSET_IDS, newHiddenAssetIds);

			const undoHideAsset = async (assetId: string) => {
				try {
					let updatedHiddenAssetIds;
					internalSetHiddenAssetIds((prevIds) => {
						updatedHiddenAssetIds = prevIds.filter((id) => id !== assetId);
						return updatedHiddenAssetIds;
					});
					await set(HIDDEN_ASSET_IDS, updatedHiddenAssetIds);
				} catch (error) {
					// Handle any error that occurred during the unhide process
					toast.error('Failed to unhide asset.');
					// Restore the asset ID back to the hidden asset IDs list
					internalSetHiddenAssetIds([...internalHiddenAssetIds, assetId]);
					await set(HIDDEN_ASSET_IDS, internalHiddenAssetIds);
				}
			};

			const showAssetHiddenToast = async (objectId: string) => {
				toast.custom(
					(t) => (
						<div
							className="flex items-center justify-between gap-2 bg-white w-full shadow-notification border-solid border-gray-45 rounded-full px-3 py-2"
							style={{
								animation: 'fade-in-up 200ms ease-in-out',
							}}
						>
							<div className="flex gap-2 items-center">
								<Check12 className="text-gray-90" />
								<div
									onClick={() => {
										toast.dismiss(t.id);
									}}
								>
									<InlineLink
										to="/nfts/hidden-assets"
										color="hero"
										weight="medium"
										before={
											<Text variant="body" color="gray-80">
												Moved to
											</Text>
										}
										text="Hidden Assets"
										onClick={() => toast.dismiss(t.id)}
									/>
								</div>
							</div>

							<div className="w-auto">
								<InlineLink
									size="bodySmall"
									onClick={() => {
										undoHideAsset(objectId);
										toast.dismiss(t.id);
									}}
									color="hero"
									weight="medium"
									text="UNDO"
								/>
							</div>
						</div>
					),
					{
						duration: 4000,
					},
				);
			};

			showAssetHiddenToast(newAssetId);
		},
		[internalHiddenAssetIds],
	);

	const hideAsset = (objectId: string, event: React.MouseEvent<HTMLButtonElement>) => {
		event.stopPropagation();
		event.preventDefault();
		hideAssetId(objectId);
	};

	const filteredNFTs = useMemo(() => {
		return nfts?.filter((nft) => !internalHiddenAssetIds.includes(nft.objectId));
	}, [nfts, internalHiddenAssetIds]);

	if (isInitialLoading) {
		return (
			<div className="mt-1 flex w-full justify-center">
				<LoadingSpinner />
			</div>
		);
	}

	return (
		<div className="flex flex-1 flex-col flex-nowrap items-center gap-4">
			<PageTitle title="Assets" after={<AssetsOptionsMenu />} />
			<Loading loading={isLoading}>
				{isError ? (
					<Alert>
						<div>
							<strong>Sync error (data might be outdated)</strong>
						</div>
						<small>{(error as Error).message}</small>
					</Alert>
				) : null}
				{filteredNFTs?.length ? (
					<div className="grid w-full grid-cols-2 gap-x-3.5 gap-y-4 mb-5">
						{filteredNFTs.map(({ objectId, type }) => (
							<Link
								to={`/nft-details?${new URLSearchParams({
									objectId,
								}).toString()}`}
								onClick={() => {
									ampli.clickedCollectibleCard({
										objectId,
										collectibleType: type!,
									});
								}}
								key={objectId}
								className="no-underline relative"
							>
								<div className="group">
									<div className="w-full h-full justify-center z-10 absolute pointer-events-auto text-gray-60 transition-colors duration-200 p-0">
										<div className="absolute top-2 right-3 rounded-md h-8 w-8 opacity-0 group-hover:opacity-100">
											<Button
												variant="hidden"
												size="icon"
												onClick={(event: any) => {
													ampli.clickedHideAsset({ objectId, collectibleType: type! });
													hideAsset(objectId, event);
												}}
												after={<EyeClose16 />}
											/>
										</div>
									</div>
									<ErrorBoundary>
										<NFTDisplayCard
											objectId={objectId}
											size="md"
											showLabel
											animateHover
											borderRadius="xl"
										/>
									</ErrorBoundary>
								</div>
							</Link>
						))}
						<div ref={observerElem}>
							{isSpinnerVisible ? (
								<div className="mt-1 flex w-full justify-center">
									<LoadingSpinner />
								</div>
							) : null}
						</div>
					</div>
				) : (
					<div className="flex flex-1 items-center self-center text-caption font-semibold text-steel-darker">
						No NFTs found
					</div>
				)}
			</Loading>
		</div>
	);
}

export default NftsPage;
