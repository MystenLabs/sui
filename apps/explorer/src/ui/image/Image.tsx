// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { EyeClose16, NftTypeImage24 } from '@mysten/icons';
import { LoadingIndicator } from '@mysten/ui';
import { cva, cx, type VariantProps } from 'class-variance-authority';
import clsx from 'clsx';
import { useAnimate } from 'framer-motion';
import { type ImgHTMLAttributes, useEffect, useState } from 'react';

import useImage from '~/hooks/useImage';
import { VISIBILITY } from '~/hooks/useImageMod';

const imageStyles = cva(null, {
	variants: {
		rounded: {
			full: 'rounded-full',
			'2xl': 'rounded-2xl',
			lg: 'rounded-lg',
			xl: 'rounded-xl',
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
		aspect: {
			square: 'aspect-square',
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

function BaseImage({
	status,
	size,
	rounded,
	alt,
	src,
	srcSet,
	fit,
	visibility,
	onClick,
	fadeIn,
	aspect,
	...imgProps
}: ImageProps & { status: string }) {
	const [scope, animate] = useAnimate();
	const [isBlurred, setIsBlurred] = useState(false);
	useEffect(() => {
		if (visibility && visibility !== VISIBILITY.PASS) {
			setIsBlurred(true);
		}
	}, [visibility]);

	const animateFadeIn = fadeIn && status === 'loaded';

	useEffect(() => {
		if (animateFadeIn) {
			animate(scope.current, { opacity: 1 }, { duration: 0.3 });
		}
	}, [animate, animateFadeIn, scope]);

	return (
		<div
			ref={scope}
			className={cx(
				imageStyles({ size, rounded, aspect }),
				'relative flex items-center justify-center bg-gray-40 text-gray-65',
				animateFadeIn && 'opacity-0',
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
				<img
					alt={alt}
					src={src}
					srcSet={srcSet}
					className={imageStyles({
						rounded,
						fit,
						size,
					})}
					onClick={onClick}
					{...imgProps}
				/>
			)}
		</div>
	);
}

export function Image({ src, moderate = true, ...props }: ImageProps) {
	const { status, url, moderation } = useImage({ src, moderate });
	return <BaseImage visibility={moderation?.visibility} status={status} src={url} {...props} />;
}
