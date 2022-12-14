// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';

const imageStyle = cva(
    [
        'bg-gradient-to-r from-gradient-blue-start to-gradient-blue-end text-white capitalize',
    ],
    {
        variants: {
            size: {
                small: 'w-6 h-6',
                medium: 'w-7.5 h-7.5',
                large: 'w-10 h-10',
            },
            variant: {
                rounded: 'rounded-full overflow-hidden',
                square: 'rounded-none',
            },
        },

        defaultVariants: {
            variant: 'rounded',
            size: 'medium',
        },
    }
);

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
                <div className="h-full w-full flex items-center justify-center font-medium text-bodySmall ">
                    {alt.slice(0, 2)}
                </div>
            )}
        </div>
    );
}
