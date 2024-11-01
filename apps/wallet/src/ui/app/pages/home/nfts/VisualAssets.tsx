// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorBoundary } from '_components/error-boundary';
import { ampli } from '_src/shared/analytics/ampli';
import { useBuyNLargeAssets } from '_src/ui/app/components/buynlarge/useBuyNLargeAssets';
import { NFTDisplayCard } from '_src/ui/app/components/nft-display';
import { Button } from '_src/ui/app/shared/ButtonUI';
import { getKioskIdFromOwnerCap, isKioskOwnerToken } from '@mysten/core';
import { useKioskClient } from '@mysten/core/src/hooks/useKioskClient';
import { EyeClose16 } from '@mysten/icons';
import { type SuiObjectData } from '@mysten/sui/client';
import { Link } from 'react-router-dom';

import { useHiddenAssets } from '../hidden-assets/HiddenAssetsProvider';

export default function VisualAssets({ items }: { items: SuiObjectData[] }) {
	const { hideAsset } = useHiddenAssets();
	const kioskClient = useKioskClient();
	const bnl = useBuyNLargeAssets();

	return (
		<div className="grid w-full grid-cols-2 gap-x-3.5 gap-y-4">
			{items.map((object) => (
				<Link
					to={
						isKioskOwnerToken(kioskClient.network, object)
							? `/kiosk?${new URLSearchParams({
									kioskId: getKioskIdFromOwnerCap(object),
								})}`
							: `/nft-details?${new URLSearchParams({
									objectId: object.objectId,
								}).toString()}`
					}
					onClick={() => {
						ampli.clickedCollectibleCard({
							objectId: object.objectId,
							collectibleType: object.type!,
						});
					}}
					key={object.objectId}
					className="no-underline relative"
				>
					<div className="group">
						<div className="w-full h-full justify-center z-10 absolute pointer-events-auto text-gray-60 transition-colors duration-200 p-0">
							{!isKioskOwnerToken(kioskClient.network, object) &&
							!bnl.some((item) => item?.objectType === object.type) ? (
								<div className="absolute top-2 right-3 rounded-md h-8 w-8 opacity-0 group-hover:opacity-100">
									<Button
										variant="hidden"
										size="icon"
										onClick={(event) => {
											event.preventDefault();
											event.stopPropagation();
											ampli.clickedHideAsset({
												objectId: object.objectId,
												collectibleType: object.type!,
											});
											hideAsset(object.objectId);
										}}
										after={<EyeClose16 />}
									/>
								</div>
							) : null}
						</div>
						<ErrorBoundary>
							<NFTDisplayCard
								hideLabel
								objectId={object.objectId}
								size="lg"
								animateHover
								borderRadius="xl"
							/>
						</ErrorBoundary>
					</div>
				</Link>
			))}
		</div>
	);
}
