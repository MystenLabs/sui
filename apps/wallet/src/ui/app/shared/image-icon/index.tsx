// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { useState } from 'react';

const imageStyle = cva(['text-white capitalize overflow-hidden bg-gray-40  shrink-0'], {
	variants: {
		size: {
			sm: 'w-6 h-6 font-medium text-subtitleSmallExtra',
			md: 'w-7.5 h-7.5 font-medium text-body',
			lg: 'w-10 h-10 font-medium text-heading4',
			xl: 'w-15 h-15 font-medium text-heading4',
		},
		circle: {
			true: 'rounded-full',
			false: 'rounded-md',
		},
	},

	defaultVariants: {
		circle: false,
		size: 'md',
	},
});

export interface ImageIconProps extends VariantProps<typeof imageStyle> {
	src: string | null;
	label: string;
	fallback: string;
	alt?: string;
}

function FallBackAvatar({ str }: { str: string }) {
	return (
		<div className="flex h-full w-full items-center justify-center bg-gradient-to-r from-gradient-blue-start to-gradient-blue-end">
			{str?.slice(0, 2)}
		</div>
	);
}

export function ImageIcon({ src, label, alt = label, fallback, ...styleProps }: ImageIconProps) {
	const [error, setError] = useState(false);
	return (
		<div role="img" className={imageStyle(styleProps)} aria-label={label}>
			{error || !src ? (
				<FallBackAvatar str={fallback} />
			) : (
				<img
					src={src}
					alt={alt}
					className="flex h-full w-full items-center justify-center object-cover"
					onError={() => setError(true)}
				/>
			)}
		</div>
	);
}
