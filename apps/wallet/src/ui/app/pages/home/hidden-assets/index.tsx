// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMultiGetObjects } from '@mysten/core';
import { Check12, EyeClose16 } from '@mysten/icons';
import { get, set } from 'idb-keyval';
import { useEffect, useCallback, useState } from 'react';
import toast from 'react-hot-toast';
import { Link } from 'react-router-dom';

import { Link as InlineLink } from '../../../shared/Link';
import { Text } from '../../../shared/text';
import Alert from '_components/alert';
import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import LoadingSpinner from '_components/loading/LoadingIndicator';
import { NFTDisplayCard } from '_components/nft-display';
import { ampli } from '_src/shared/analytics/ampli';
import { Button } from '_src/ui/app/shared/ButtonUI';
import PageTitle from '_src/ui/app/shared/PageTitle';

const HIDDEN_ASSET_IDS = 'hidden-asset-ids';

function NftsPage() {
	const [internalHiddenAssetIds, internalSetHiddenAssetIds] = useState<string[]>([]);

	const { data, isInitialLoading, isLoading, isError, error } = useMultiGetObjects(
		// Prevents dupes
		Array.from(new Set(internalHiddenAssetIds))!,
		{ showContent: true },
	);

	const hiddenNfts = data?.flatMap((data) => data.data) || [];

	useEffect(() => {
		(async () => {
			const hiddenAssets = await get<string[]>(HIDDEN_ASSET_IDS);
			if (hiddenAssets) {
				internalSetHiddenAssetIds(hiddenAssets);
			}
		})();
	}, []);

	const showAssetId = useCallback(
		async (newAssetId: string) => {
			if (!internalHiddenAssetIds.includes(newAssetId)) return;

			try {
				const updatedHiddenAssetIds = internalHiddenAssetIds.filter((id) => id !== newAssetId);
				internalSetHiddenAssetIds(updatedHiddenAssetIds);
				await set(HIDDEN_ASSET_IDS, updatedHiddenAssetIds);
			} catch (error) {
				// Handle any error that occurred during the unhide process
				toast.error('Failed to show asset.');
				// Restore the asset ID back to the hidden asset IDs list
				internalSetHiddenAssetIds([...internalHiddenAssetIds, newAssetId]);
				await set(HIDDEN_ASSET_IDS, internalHiddenAssetIds);
			}

			const undoShowAsset = async (assetId: string) => {
				let newHiddenAssetIds;
				internalSetHiddenAssetIds((prevIds) => {
					return (newHiddenAssetIds = [...prevIds, assetId]);
				});
				await set(HIDDEN_ASSET_IDS, newHiddenAssetIds);
			};

			const assetShownToast = async (objectId: string) => {
				toast.custom(
					(t) => (
						<div className="flex items-center justify-between gap-2 bg-white w-full shadow-summary-card border-solid border-gray-45 rounded-full px-3 py-1">
							<div className="flex gap-1 items-center">
								<Check12 className="text-gray-90" />
								<div
									onClick={() => {
										toast.dismiss(t.id);
									}}
								>
									<InlineLink
										to="/nfts"
										color="hero"
										weight="medium"
										before={
											<Text variant="body" color="gray-80">
												Moved to
											</Text>
										}
										text="Visual Assets"
									/>
								</div>
							</div>

							<div className="w-auto">
								<InlineLink
									size="bodySmall"
									onClick={() => {
										undoShowAsset(objectId);
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

			assetShownToast(newAssetId);
		},
		[internalHiddenAssetIds],
	);

	const showAsset = (objectId: string) => {
		showAssetId(objectId);
	};

	if (isInitialLoading) {
		return (
			<div className="mt-1 flex w-full justify-center">
				<LoadingSpinner />
			</div>
		);
	}

	return (
		<div className="flex flex-1 flex-col flex-nowrap items-center gap-4">
			<PageTitle title="Hidden Assets" back="/nfts" />
			<Loading loading={isLoading && Boolean(internalHiddenAssetIds.length)}>
				{isError ? (
					<Alert>
						<div>
							<strong>Sync error (data might be outdated)</strong>
						</div>
						<small>{(error as Error).message}</small>
					</Alert>
				) : null}
				{hiddenNfts?.length ? (
					<div className="flex flex-col w-full divide-y divide-solid divide-gray-40 divide-x-0 gap-2">
						{hiddenNfts
							.filter((nft) => nft && internalHiddenAssetIds.includes(nft.objectId))
							.map((nft, index) => {
								const { objectId, type } = nft!;
								return (
									<div className="flex justify-between items-center pt-2" key={objectId}>
										<Link
											to={`/nft-details?${new URLSearchParams({
												objectId: objectId,
											}).toString()}`}
											onClick={() => {
												ampli.clickedCollectibleCard({
													objectId,
													collectibleType: type!,
												});
											}}
											className="no-underline relative truncate"
										>
											<ErrorBoundary>
												<NFTDisplayCard
													objectId={objectId}
													size="xs"
													showLabel
													orientation="horizontal"
												/>
											</ErrorBoundary>
										</Link>
										<div>
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
						No NFTs found
					</div>
				)}
			</Loading>
		</div>
	);
}

export default NftsPage;
