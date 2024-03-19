// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_src/ui/app/shared/text';
import { Check12 } from '@mysten/icons';
import { get, set } from 'idb-keyval';
import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react';
import { toast } from 'react-hot-toast';

import { Link as InlineLink } from '../../../shared/Link';

const HIDDEN_ASSET_IDS = 'hidden-asset-ids';

interface HiddenAssetContext {
	hiddenAssetIds: string[];
	setHiddenAssetIds: (hiddenAssetIds: string[]) => void;
	hideAsset: (assetId: string) => void;
	showAsset: (assetId: string) => void;
}

export const HiddenAssetsContext = createContext<HiddenAssetContext>({
	hiddenAssetIds: [],
	setHiddenAssetIds: () => {},
	hideAsset: () => {},
	showAsset: () => {},
});

export const HiddenAssetsProvider = ({ children }: { children: ReactNode }) => {
	const [hiddenAssetIds, setHiddenAssetIds] = useState<string[]>([]);

	useEffect(() => {
		(async () => {
			const hiddenAssets = await get<string[]>(HIDDEN_ASSET_IDS);
			if (hiddenAssets) {
				setHiddenAssetIds(hiddenAssets);
			}
		})();
	}, []);

	const hideAssetId = useCallback(
		async (newAssetId: string) => {
			if (hiddenAssetIds.includes(newAssetId)) return;

			const newHiddenAssetIds = [...hiddenAssetIds, newAssetId];
			setHiddenAssetIds(newHiddenAssetIds);
			await set(HIDDEN_ASSET_IDS, newHiddenAssetIds);

			const undoHideAsset = async (assetId: string) => {
				try {
					let updatedHiddenAssetIds;
					setHiddenAssetIds((prevIds) => {
						updatedHiddenAssetIds = prevIds.filter((id) => id !== assetId);
						return updatedHiddenAssetIds;
					});
					await set(HIDDEN_ASSET_IDS, updatedHiddenAssetIds);
				} catch (error) {
					// Handle any error that occurred during the unhide process
					toast.error('Failed to unhide asset.');
					// Restore the asset ID back to the hidden asset IDs list
					setHiddenAssetIds([...hiddenAssetIds, assetId]);
					await set(HIDDEN_ASSET_IDS, hiddenAssetIds);
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
		[hiddenAssetIds],
	);

	const showAssetId = useCallback(
		async (newAssetId: string) => {
			if (!hiddenAssetIds.includes(newAssetId)) return;

			try {
				const updatedHiddenAssetIds = hiddenAssetIds.filter((id) => id !== newAssetId);
				setHiddenAssetIds(updatedHiddenAssetIds);
				await set(HIDDEN_ASSET_IDS, updatedHiddenAssetIds);
			} catch (error) {
				// Handle any error that occurred during the unhide process
				toast.error('Failed to show asset.');
				// Restore the asset ID back to the hidden asset IDs list
				setHiddenAssetIds([...hiddenAssetIds, newAssetId]);
				await set(HIDDEN_ASSET_IDS, hiddenAssetIds);
			}

			const undoShowAsset = async (assetId: string) => {
				let newHiddenAssetIds;
				setHiddenAssetIds((prevIds) => {
					return (newHiddenAssetIds = [...prevIds, assetId]);
				});
				await set(HIDDEN_ASSET_IDS, newHiddenAssetIds);
			};

			const assetShownToast = async (objectId: string) => {
				toast.custom(
					(t) => (
						<div
							className="flex items-center justify-between gap-2 bg-white w-full shadow-notification border-solid border-gray-45 rounded-full px-3 py-2"
							style={{
								animation: 'fade-in-up 200ms ease-in-out',
							}}
						>
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
										onClick={() => toast.dismiss(t.id)}
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
		[hiddenAssetIds],
	);

	const showAsset = (objectId: string) => {
		showAssetId(objectId);
	};

	return (
		<HiddenAssetsContext.Provider
			value={{
				hiddenAssetIds: Array.from(new Set(hiddenAssetIds)),
				setHiddenAssetIds,
				hideAsset: hideAssetId,
				showAsset,
			}}
		>
			{children}
		</HiddenAssetsContext.Provider>
	);
};

export const useHiddenAssets = () => {
	return useContext(HiddenAssetsContext);
};
