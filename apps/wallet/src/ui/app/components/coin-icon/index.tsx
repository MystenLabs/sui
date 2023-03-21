// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coins16 } from '@mysten/icons';
import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { cva, type VariantProps } from 'class-variance-authority';

import Icon, { SuiIcons } from '_components/icon';

const imageStyle = cva(['rounded-full flex bg-sui'], {
    variants: {
        size: {
            sm: 'w-6 h-6',
            md: 'w-7.5 h-7.5',
            lg: 'md:w-10 md:h-10 w-8 h-8',
            xl: 'md:w-31.5 md:h-31.5 w-16 h-16 ',
        },
    },

    defaultVariants: {
        size: 'md',
    },
});

export interface CoinIconProps extends VariantProps<typeof imageStyle> {
    coinType: string;
}

// fetch the coin metadata
// show the coin icon
// fallback to the coin name if the icon is not available
// TODO: (jibz) use getCoinMetadata to get the coin metadata and use sui icons

export function CoinIcon({ coinType, ...styleProps }: CoinIconProps) {
    return (
        <div className={imageStyle(styleProps)}>
            {coinType === SUI_TYPE_ARG ? (
                <Icon
                    icon={SuiIcons.SuiLogoIcon}
                    className="flex h-full w-full items-center justify-center text-white text-body"
                />
            ) : (
                <div className="flex h-full w-full items-center justify-center text-white">
                    <Coins16 />
                </div>
            )}
        </div>
    );
}
