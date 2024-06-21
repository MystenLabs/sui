// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Heading } from '_app/shared/heading';
import Loading from '_components/loading';
import { NftImage, type NftImageProps } from '_components/nft-display/NftImage';
import { useFileExtensionType, useGetNFTMeta } from '_hooks';
import { isKioskOwnerToken, useGetObject } from '@mysten/core';
import { useKioskClient } from '@mysten/core/src/hooks/useKioskClient';
import { formatAddress } from '@mysten/sui/utils';
import { cva } from 'class-variance-authority';
import type { VariantProps } from 'class-variance-authority';

import { useResolveVideo } from '../../hooks/useResolveVideo';
import { Text } from '../../shared/text';
import { Kiosk } from './Kiosk';

const nftDisplayCardStyles = cva('flex flex-nowrap items-center h-full relative', {
	variants: {
		animateHover: {
			true: 'group',
		},
		wideView: {
			true: 'bg-gray-40 p-2.5 rounded-lg gap-2.5 flex-row-reverse justify-between',
			false: '',
		},
		orientation: {
			horizontal: 'flex truncate',
			vertical: 'flex-col',
		},
	},
	defaultVariants: {
		wideView: false,
		orientation: 'vertical',
	},
});

export interface NFTDisplayCardProps extends VariantProps<typeof nftDisplayCardStyles> {
	objectId: string;
	hideLabel?: boolean;
	size: NftImageProps['size'];
	borderRadius?: NftImageProps['borderRadius'];
	playable?: boolean;
	isLocked?: boolean;
}

export function NFTDisplayCard({
	objectId,
	hideLabel,
	size,
	wideView,
	animateHover,
	borderRadius = 'md',
	playable,
	orientation,
	isLocked,
}: NFTDisplayCardProps) {
	const { data: objectData } = useGetObject(objectId);
	const { data: nftMeta, isPending } = useGetNFTMeta(objectId);
	const nftName = nftMeta?.name || formatAddress(objectId);
	const nftImageUrl = nftMeta?.imageUrl || '';
	const video = useResolveVideo(objectData);
	const fileExtensionType = useFileExtensionType(nftImageUrl);
	const kioskClient = useKioskClient();
	const isOwnerToken = isKioskOwnerToken(kioskClient.network, objectData);
	const shouldShowLabel = !wideView && orientation !== 'horizontal';

	return (
		<div className={nftDisplayCardStyles({ animateHover, wideView, orientation })}>
			<Loading loading={isPending}>
				{objectData?.data && isOwnerToken ? (
					<Kiosk
						object={objectData}
						borderRadius={borderRadius}
						size={size}
						orientation={orientation}
						playable={playable}
						showLabel={shouldShowLabel}
					/>
				) : (
					<NftImage
						name={nftName}
						src={nftImageUrl}
						animateHover={animateHover}
						showLabel={shouldShowLabel}
						borderRadius={borderRadius}
						size={size}
						isLocked={isLocked}
						video={video}
					/>
				)}
				{wideView && (
					<div className="flex flex-col gap-1 flex-1 min-w-0 ml-1">
						<Heading variant="heading6" color="gray-90" truncate>
							{nftName}
						</Heading>
						<div className="text-gray-75 text-body font-medium">
							{nftImageUrl ? (
								`${fileExtensionType.name} ${fileExtensionType.type}`
							) : (
								<span className="uppercase font-normal text-bodySmall">NO MEDIA</span>
							)}
						</div>
					</div>
				)}

				{orientation === 'horizontal' ? (
					<div className="flex-1 text-steel-dark overflow-hidden max-w-full ml-2">{nftName}</div>
				) : !isOwnerToken && !hideLabel ? (
					<div className="w-10/12 absolute bottom-2 bg-white/90 rounded-lg left-1/2 -translate-x-1/2 flex items-center justify-center opacity-0 group-hover:opacity-100">
						<div className="mt-0.5 px-2 py-1 overflow-hidden">
							<Text variant="subtitleSmall" weight="semibold" mono color="steel-darker" truncate>
								{nftName}
							</Text>
						</div>
					</div>
				) : null}
			</Loading>
		</div>
	);
}
