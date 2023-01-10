// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { useState } from 'react';

import fallbackImage from "../assets/ImageIconfFallback.svg";

const imageStyle = cva(
    [
        'bg-gradient-to-r from-gradient-blue-start to-gradient-blue-end text-white capitalize overflow-hidden',
    ],
    {
        variants: {
            size: {
                sm: 'w-6 h-6 font-medium text-subtitleSmallExtra',
                md: 'w-8 h-8 font-medium text-body',
                lg: 'md:w-10 md:h-10 w-8 h-8 font-medium text-heading4 md:text-3xl',
                xl: 'md:w-31.5 md:h-31.5 w-16 h-16 font-medium text-heading4 md:text-5xl',
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
    }
);

export interface ImageIconProps extends VariantProps<typeof imageStyle> {
    src?: string | null;
    alt: string;
}
  
export function ImageIcon({ src, alt, ...styleProps }: ImageIconProps) {

    return (
        <div role="img" className={imageStyle(styleProps)} aria-label={alt}>
            <img
                src={src || ''} 
                alt={alt}
                aria-label={alt}
                className="flex h-full w-full items-center justify-center"
                onError={(e) => (e.currentTarget.src= fallbackImage)}
            />
          
        </div>
    );
}
