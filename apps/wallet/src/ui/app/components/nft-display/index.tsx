// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetObject } from '@mysten/core';
import { formatAddress } from '@mysten/sui.js';
import { cva, cx } from 'class-variance-authority';

import { useResolveVideo } from '../../hooks/useResolveVideo';
import { Heading } from '_app/shared/heading';
import Loading from '_components/loading';
import { NftImage, type NftImageProps } from '_components/nft-display/NftImage';
import { useGetNFTMeta, useFileExtensionType } from '_hooks';

import type { VariantProps } from 'class-variance-authority';

const nftDisplayCardStyles = cva('flex flex-nowrap items-center h-full', {
	variants: {
		animateHover: {
			true: 'group',
		},
		listView: {
			true: '',
			false: 'flex-col',
		},
		wideView: {
			true: 'bg-gray-40 p-2.5 rounded-lg gap-2.5 flex-row-reverse justify-between',
			false: '',
		},
	},
	defaultVariants: {
		wideView: false,
		listView: false,
	},
});

export interface NFTsProps extends VariantProps<typeof nftDisplayCardStyles> {
	objectId: string;
	showLabel?: boolean;
	size: NftImageProps['size'];
	borderRadius?: NftImageProps['borderRadius'];
	playable?: boolean;
}

export function NFTDisplayCard({
	objectId,
	showLabel,
	size,
	wideView,
	animateHover,
	borderRadius = 'md',
	playable,
	listView,
}: NFTsProps) {
	const { data: objectData } = useGetObject(objectId);
	const { data: nftMeta, isLoading } = useGetNFTMeta(objectId);
	const nftName = nftMeta?.name || formatAddress(objectId);
	const nftImageUrl = nftMeta?.imageUrl || '';
	const video = useResolveVideo(objectData);
	const fileExtensionType = useFileExtensionType(nftImageUrl);

	return (
		<div className={nftDisplayCardStyles({ animateHover, wideView, listView })}>
			<Loading loading={isLoading}>
				{video && playable ? (
					<video controls className="h-full w-full rounded-md overflow-hidden" src={video} />
				) : (
					<NftImage
						name={nftName}
						src={nftImageUrl}
						title={nftMeta?.description || ''}
						animateHover={true}
						showLabel={!wideView || !listView}
						borderRadius={borderRadius}
						size={size}
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
				{showLabel && !wideView && (
					<div
						className={cx(
							'flex-1 text-steel-dark truncate overflow-hidden max-w-full',
							animateHover ? 'group-hover:text-black duration-200 ease-ease-in-out-cubic' : '',
							listView ? 'ml-2' : 'mt-2',
						)}
					>
						{nftName}
					</div>
				)}
			</Loading>
		</div>
	);
}
