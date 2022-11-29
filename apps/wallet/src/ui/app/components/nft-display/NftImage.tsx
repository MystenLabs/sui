// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva } from 'class-variance-authority';

import Icon, { SuiIcons } from '../icon';

import type { VariantProps } from 'class-variance-authority';

const containerStyles = cva('overflow-hidden', {
    variants: {
        animateHover: {
            true: [
                'ease-ease-out-cubic duration-400',
                'group-hover:shadow-blurXl group-hover:shadow-steel/50',
            ],
        },
        borderRadius: {
            md: 'rounded-md',
            xl: 'rounded-xl',
        },
        size: {
            sm: 'w-12 h-12',
            md: 'w-36 h-36',
            lg: 'w-44 h-44',
        },
    },
    compoundVariants: [
        {
            animateHover: true,
            borderRadius: 'xl',
            class: 'group-hover:rounded-md',
        },
    ],
    defaultVariants: {
        borderRadius: 'md',
    },
});

const imageStyles = cva('w-full h-full object-cover', {
    variants: {
        animateHover: {
            true: 'group-hover:scale-110 duration-500 ease-ease-out-cubic',
        },
    },
});

export interface NftImageProps extends VariantProps<typeof containerStyles> {
    src: string | null;
    name: string | null;
    title?: string;
    showLabel?: boolean;
}

export function NftImage({
    src,
    name,
    title,
    showLabel,
    animateHover,
    borderRadius,
    size,
}: NftImageProps) {
    return (
        <div className={containerStyles({ animateHover, borderRadius, size })}>
            {src ? (
                <img
                    className={imageStyles({ animateHover })}
                    src={src}
                    alt={name || 'NFT'}
                    title={title}
                />
            ) : (
                <div
                    className={imageStyles({
                        animateHover,
                        class: [
                            'flex flex-col flex-nowrap items-center justify-center',
                            'select-none uppercase text-steel-dark bg-noMedia gap-2',
                        ],
                    })}
                    title={title}
                >
                    <Icon className="text-xl" icon={SuiIcons.NftTypeImage} />
                    {showLabel ? (
                        <span className="text-captionSmall font-medium">
                            No media
                        </span>
                    ) : null}
                </div>
            )}
        </div>
    );
}
