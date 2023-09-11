// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { MediaPlay16 } from '@mysten/icons';
import { cva, type VariantProps } from 'class-variance-authority';
import clsx from 'clsx';
import { useState } from 'react';

import { ObjectModal } from '~/ui/Modal/ObjectModal';
import { Image as ImageComponent, type ImageProps } from '~/ui/image/Image';

const imageStyles = cva(['z-0 flex-shrink-0 relative'], {
	variants: {
		variant: {
			xs: 'h-8',
			small: 'h-16',
			medium: 'md:h-31.5 h-16',
			large: 'h-50',
			xl: 'h-objectVideoImgEvenXl',
		},
		orientation: {
			landscape: '',
			portrait: '',
			even: '',
		},
		disablePreview: {
			true: '',
			false: 'cursor-pointer',
		},
	},
	defaultVariants: {
		disablePreview: false,
		orientation: 'even',
	},
	compoundVariants: [
		// orientation: even
		{
			variant: 'xs',
			orientation: 'even',
			className: 'w-8',
		},
		{
			variant: 'small',
			orientation: 'even',
			className: 'w-16',
		},
		{
			variant: 'medium',
			orientation: 'even',
			className: 'md:w-31.5 w-16',
		},
		{
			variant: 'large',
			orientation: 'even',
			className: 'w-50',
		},
		{
			variant: 'xl',
			orientation: 'even',
			className: 'w-objectVideoImgEvenXl',
		},
		// orientation: portrait
		{
			variant: 'xs',
			orientation: 'portrait',
			className: 'w-6',
		},
		{
			variant: 'small',
			orientation: 'portrait',
			className: 'w-12',
		},
		{
			variant: 'medium',
			orientation: 'portrait',
			className: 'md:w-24 w-12',
		},
		{
			variant: 'large',
			orientation: 'portrait',
			className: 'w-objectVideoImgPortraitLarge',
		},
		{
			variant: 'xl',
			orientation: 'portrait',
			className: 'w-objectVideoImgPortraitXl',
		},
		// orientation: landscape
		{
			variant: 'xs',
			orientation: 'landscape',
			className: 'w-objectVideoImgLandscapeXs',
		},
		{
			variant: 'small',
			orientation: 'landscape',
			className: 'w-objectVideoImgLandscapeSmall',
		},
		{
			variant: 'medium',
			orientation: 'landscape',
			className: 'md:w-objectVideoImgLandscapeMedium w-objectVideoImgLandscapeSmall',
		},
		{
			variant: 'large',
			orientation: 'landscape',
			className: 'w-objectVideoImgLandscapeLarge',
		},
		{
			variant: 'xl',
			orientation: 'landscape',
			className: 'w-objectVideoImgLandscapeXl',
		},
	],
});

type ImageStylesProps = VariantProps<typeof imageStyles>;

export interface ObjectVideoImageProps extends Omit<ImageStylesProps, 'orientation'> {
	title: string;
	subtitle: string;
	src: string;
	open?: boolean;
	setOpen?: (open: boolean) => void;
	video?: string | null;
	rounded?: ImageProps['rounded'];
	disablePreview?: boolean;
	fadeIn?: boolean;
	dynamicOrientation?: boolean;
}

function useImageOrientation(
	src: string,
	dynamicOrientation: boolean,
): ImageStylesProps['orientation'] {
	const [orientation, setOrientation] = useState<ImageStylesProps['orientation']>('even');
	const img = new Image();
	img.src = src;
	img.onload = () => {
		if (!dynamicOrientation) {
			return;
		}

		if (img.naturalWidth > img.naturalHeight) {
			setOrientation('landscape');
		} else if (img.naturalWidth < img.naturalHeight) {
			setOrientation('portrait');
		} else {
			setOrientation('even');
		}
	};

	return orientation;
}

export function ObjectVideoImage({
	title,
	subtitle,
	src,
	video,
	variant,
	open,
	setOpen,
	disablePreview,
	fadeIn,
	dynamicOrientation,
}: ObjectVideoImageProps) {
	const orientation = useImageOrientation(src, !!dynamicOrientation);

	const close = () => {
		if (disablePreview) {
			return;
		}

		if (setOpen) {
			setOpen(false);
		}
	};
	const openPreview = () => {
		if (disablePreview) {
			return;
		}

		if (setOpen) {
			setOpen(true);
		}
	};

	return (
		<>
			<ObjectModal
				open={!!open}
				onClose={close}
				title={title}
				subtitle={subtitle}
				src={src}
				video={video}
				alt={title}
			/>
			<div className={imageStyles({ variant, disablePreview, orientation })}>
				<ImageComponent rounded="md" onClick={openPreview} alt={title} src={src} fadeIn={fadeIn} />
				{video && (
					<div className="pointer-events-none absolute bottom-2 right-2 z-10 flex items-center justify-center rounded-full opacity-80">
						<MediaPlay16 className={clsx(variant === 'large' ? 'h-8 w-8' : 'h-5 w-5')} />
					</div>
				)}
			</div>
		</>
	);
}
