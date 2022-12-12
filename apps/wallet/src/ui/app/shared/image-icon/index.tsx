// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';

const imageStyle = cva([], {
    variants: {
        size: {
            none: 'w-0 h-0',
            small: 'w-6 h-6',
            medium: 'w-icon h-icon',
            large: 'w-10 h-10',
        },
        variant: {
            rounded: 'rounded-full overflow-hidden',
            square: 'rounded-none',
        },
        fillers: {
            true: 'bg-gray-45',
        },
    },
    compoundVariants: [
        {
            fillers: false,
            variant: 'rounded',
            class: 'w-0 h-0',
        },
    ],
    defaultVariants: {
        variant: 'rounded',
        size: 'medium',
        fillers: true,
    },
});

export interface IconProps extends VariantProps<typeof imageStyle> {
    src?: string | null;
    alt: string;
}

export function ImageIcon({ src, alt, ...styleProps }: IconProps) {
    return (
        <div className={imageStyle(styleProps)}>
            {src ? (
                <img src={src} className="h-full w-full" alt={alt} />
            ) : (
                <div className="h-full w-full flex items-center justify-center font-semibold text-body text-hero-dark capitalize">
                    {alt.charAt(0)}
                </div>
            )}
        </div>
    );
}
