// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { EyeClose16, NftTypeImage24 } from '@mysten/icons';
import { cva, cx, type VariantProps } from 'class-variance-authority';

import { LoadingSpinner } from '../LoadingSpinner';

import useImage from '~/hooks/useImage';

const imageStyles = cva(null, {
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
    moderate?: boolean;
    src: string;
    blur?: boolean;
}

function BaseImage({
    status,
    size,
    rounded,
    alt,
    src,
    srcSet,
    fit,
    blur,
    onClick,
    ...imgProps
}: ImageProps & { status: string }) {
    return (
        <div
            className={cx(
                imageStyles({ size, rounded }),
                'relative flex items-center justify-center bg-gray-40 text-gray-65'
            )}
        >
            {status === 'loading' ? (
                <LoadingSpinner />
            ) : blur && status === 'loaded' ? (
                <div className="absolute z-20 flex h-full w-full cursor-not-allowed items-center justify-center rounded-md bg-gray-100/30 text-center text-white backdrop-blur-md">
                    <EyeClose16 />
                </div>
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
    const { status, url, nsfw } = useImage({ src, moderate });
    return <BaseImage blur={nsfw} status={status} src={url} {...props} />;
}
