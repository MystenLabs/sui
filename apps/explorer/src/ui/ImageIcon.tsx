// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';

const imageStyle = cva(['bg-gray-45 text-gray-65 capitalize'], {
    variants: {
        //TODO: verify sizes with design especially for mobile
        size: {
            sm: 'w-6 h-6 font-medium text-bodySmall',
            md: 'w-8 h-8 font-medium text-body',
            lg: 'md:w-10 md:h-10 w-8 h-8 font-medium text-heading1',
            xl: 'md:w-32 md:h-32 w-16 font-medium text-heading1',
        },
        variant: {
            circle: 'rounded-full overflow-hidden',
            square: 'rounded-md',
        },
    },

    defaultVariants: {
        variant: 'circle',
        size: 'md',
    },
});

export interface ImageIconProps extends VariantProps<typeof imageStyle> {
    src?: string | null;
    alt: string;
}

export function ImageIcon({ src, alt, ...styleProps }: ImageIconProps) {
    return (
        <div className={imageStyle(styleProps)}>
            {src ? (
                <img src={src} className="h-full w-full" alt={alt} />
            ) : (
                <div className="flex h-full w-full items-center justify-center">
                    {alt.slice(0, 2)}
                </div>
            )}
        </div>
    );
}
