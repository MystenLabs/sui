// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { EyeClose16, NftTypeImage24 } from '@mysten/icons';
import { cva, cx, type VariantProps } from 'class-variance-authority';
import { useEffect, useState } from 'react';

type Status = 'loading' | 'failed' | 'loaded';

const imageStyles = cva([], {
    variants: {
        rounded: {
            full: 'rounded-full',
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

export interface ImageProps
    extends ImageStyleProps,
        React.ImgHTMLAttributes<HTMLImageElement> {
    onClick?: () => void;
    src: string;
    blur?: boolean;
}

export function Image({
    alt,
    src,
    srcSet,
    blur = false,
    onClick,
    rounded,
    fit,
    size,
    ...imgProps
}: ImageProps) {
    const [status, setStatus] = useState<Status>('loading');

    useEffect(() => {
        const img = new global.Image();
        img.src = src;
        img.onload = () => setStatus('loaded');
        img.onerror = () => setStatus('failed');
    });

    return (
        <div
            className={cx(
                imageStyles({ size, rounded }),
                'relative flex items-center justify-center bg-gray-45 text-gray-65'
            )}
        >
            {blur ? (
                <div className="pointer-events-none absolute z-20 flex h-full w-full items-center justify-center rounded-md bg-gray-100/30 text-center backdrop-blur-md">
                    <EyeClose16 className="text-white" />
                </div>
            ) : null}
            {status === 'failed' || status === 'loading' ? (
                <NftTypeImage24 />
            ) : (
                <img
                    alt={alt}
                    src={src}
                    srcSet={srcSet}
                    className={cx(
                        imageStyles({
                            rounded,
                            fit,
                            size,
                        })
                    )}
                    onClick={onClick}
                    {...imgProps}
                />
            )}
        </div>
    );
}
