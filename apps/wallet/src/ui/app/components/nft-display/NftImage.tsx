// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Image32, LockLocked16, MediaPlay16 } from '@mysten/icons';
import { cva } from 'class-variance-authority';
import cl from 'classnames';
import { useState } from 'react';

import type { VariantProps } from 'class-variance-authority';

export const nftImageStyles = cva('overflow-hidden bg-gray-40 relative', {
	variants: {
		animateHover: {
			true: [
				'ease-ease-out-cubic duration-400',
				'group-hover:shadow-blurXl group-hover:shadow-steel/50',
			],
		},
		borderRadius: {
			md: 'rounded-md',
			xl: 'rounded-xl',
			sm: 'rounded',
		},
		size: {
			xs: 'w-10 h-10',
			sm: 'w-12 h-12',
			md: 'w-24 h-24',
			lg: 'w-36 h-36',
			xl: 'w-50 h-50',
		},
	},
	compoundVariants: [
		{
			animateHover: true,
			borderRadius: 'xl',
			class: 'group-hover:rounded-md',
		},
	],
	defaultVariants: {
		borderRadius: 'md',
	},
});

export interface NftImageProps extends VariantProps<typeof nftImageStyles> {
	src: string | null;
	video?: string | null;
	name: string | null;
	title?: string;
	showLabel?: boolean;
	playable?: boolean;
	className?: string;
	isLocked?: boolean;
}

export function NftImage({
	src,
	name,
	title,
	showLabel,
	animateHover,
	borderRadius,
	size,
	video,
	playable,
	className,
	isLocked,
}: NftImageProps) {
	const [error, setError] = useState(false);
	const imgCls = cl(
		'w-full h-full object-cover ',
		animateHover && 'group-hover:scale-110 duration-500 ease-ease-out-cubic',
		className,
	);
	const imgSrc = src ? src.replace(/^ipfs:\/\//, 'https://ipfs.io/ipfs/') : '';
	return (
		<div
			className={nftImageStyles({
				animateHover,
				borderRadius,
				size,
			})}
		>
			{error || !imgSrc ? (
				<div
					className={cl(
						imgCls,
						'flex flex-col flex-nowrap items-center justify-center',
						'select-none uppercase text-steel-dark gap-2 bg-gray-40',
					)}
					title={title}
				>
					<Image32 className="text-steel text-3xl" />
					{showLabel ? <span className="text-captionSmall font-medium">No media</span> : null}
				</div>
			) : (
				<img
					className={imgCls}
					src={imgSrc}
					alt={name || 'NFT'}
					title={title}
					onError={() => setError(true)}
				/>
			)}

			{video ? (
				playable ? (
					<video controls className="h-full w-full rounded-md overflow-hidden" src={video} />
				) : (
					<div className="pointer-events-none absolute bottom-2 right-2 z-10 flex items-center justify-center rounded-full opacity-80 text-black">
						<MediaPlay16 className="h-8 w-8" />
					</div>
				)
			) : null}
			{isLocked ? (
				<div className="right-1.5 bottom-1.5 flex items-center justify-center absolute h-6 w-6 bg-gray-100 text-white rounded-md">
					<LockLocked16 className="h-4 w-4" />
				</div>
			) : null}
		</div>
	);
}
