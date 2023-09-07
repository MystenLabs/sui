// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { EyeClose16, NftTypeImage24 } from '@mysten/icons';
import { LoadingIndicator } from '@mysten/ui';
import { cva, cx, type VariantProps } from 'class-variance-authority';
import clsx from 'clsx';
import { motion } from 'framer-motion';
import { type ImgHTMLAttributes, useEffect, useState } from 'react';

import useImage from '~/hooks/useImage';
import { VISIBILITY } from '~/hooks/useImageMod';

const imageStyles = cva(null, {
	variants: {
		rounded: {
			full: 'rounded-full',
			'2xl': 'rounded-2xl',
			lg: 'rounded-lg',
			md: 'rounded-md',
			sm: 'rounded-sm',
			none: 'rounded-none',
		},
		fit: {
			cover: 'object-cover',
			contain: 'object-contain',
			fill: 'object-fill',
			none: 'object-none',
			scaleDown: 'object-scale-down',
		},
		size: {
			sm: 'h-16 w-16',
			md: 'h-24 w-24',
			lg: 'h-32 w-32',
			full: 'h-full w-full',
		},
	},
	defaultVariants: {
		size: 'full',
		rounded: 'none',
		fit: 'cover',
	},
});

type ImageStyleProps = VariantProps<typeof imageStyles>;

export interface ImageProps extends ImageStyleProps, ImgHTMLAttributes<HTMLImageElement> {
	onClick?: () => void;
	moderate?: boolean;
	src: string;
	visibility?: VISIBILITY;
	fadeIn?: boolean;
}

function BaseImageContent({
	alt,
	src,
	srcSet,
	rounded,
	fit,
	size,
	fadeIn,
	...imgProps
}: Omit<ImageProps, 'moderate' | 'visibility' | 'status'>) {
	const content = (
		<img
			alt={alt}
			src={src}
			srcSet={srcSet}
			className={imageStyles({
				rounded,
				fit,
				size,
			})}
			{...imgProps}
		/>
	);

	return fadeIn ? (
		<motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.3 }}>
			{content}
		</motion.div>
	) : (
		content
	);
}

function BaseImage({
	status,
	size,
	rounded,
	visibility,
	onClick,
	...imgProps
}: ImageProps & { status: string }) {
	const [isBlurred, setIsBlurred] = useState(false);
	useEffect(() => {
		if (visibility && visibility !== VISIBILITY.PASS) {
			setIsBlurred(true);
		}
	}, [visibility]);
	return (
		<div
			className={cx(
				imageStyles({ size, rounded }),
				'relative flex items-center justify-center bg-gray-40 text-gray-65',
			)}
		>
			{status === 'loading' ? (
				<LoadingIndicator />
			) : status === 'loaded' ? (
				isBlurred && (
					<div
						className={clsx(
							'absolute z-20 flex h-full w-full items-center justify-center rounded-md bg-gray-100/30 text-center text-white backdrop-blur-md',
							visibility === VISIBILITY.HIDE && 'pointer-events-none cursor-not-allowed',
						)}
						onClick={() => setIsBlurred(!isBlurred)}
					>
						<EyeClose16 />
					</div>
				)
			) : status === 'failed' ? (
				<NftTypeImage24 />
			) : null}
			{status === 'loaded' && (
				<BaseImageContent onClick={onClick} rounded={rounded} size={size} {...imgProps} />
			)}
		</div>
	);
}

export function Image({ src, moderate = true, ...props }: ImageProps) {
	const { status, url, moderation } = useImage({ src, moderate });
	return <BaseImage visibility={moderation?.visibility} status={status} src={url} {...props} />;
}
