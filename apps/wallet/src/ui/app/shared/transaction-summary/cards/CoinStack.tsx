// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCoinMetadata } from '@mysten/core';
import { Sui, Unstaked } from '@mysten/icons';
import { type CoinMetadata } from '@mysten/sui.js';
import clsx from 'classnames';

import { Text } from '../../text';

function CoinIcon({ coinMetadata }: { coinMetadata?: CoinMetadata | null }) {
    if (coinMetadata?.symbol === 'SUI') {
        return <Sui className="h-2.5 w-2.5" />;
    }

    if (coinMetadata?.iconUrl) {
        return (
            <img alt={coinMetadata?.description} src={coinMetadata?.iconUrl} />
        );
    }

    return <Unstaked className="h-2.5 w-2.5" />;
}

export function Coin({ type }: { type: string }) {
    const { data: coinMetadata } = useCoinMetadata(type);
    const { symbol, iconUrl } = coinMetadata || {};

    return (
        <span
            className={clsx(
                'relative flex h-5 w-5 items-center justify-center rounded-xl text-white',
                (!coinMetadata || symbol !== 'SUI') &&
                    'bg-gradient-to-r from-gradient-blue-start to-gradient-blue-end',
                symbol === 'SUI' && 'bg-sui',
                iconUrl && 'bg-gray-40'
            )}
        >
            <CoinIcon coinMetadata={coinMetadata} />
        </span>
    );
}

export interface CoinsStackProps {
    coinTypes: string[];
}

const MAX_COINS_TO_DISPLAY = 4;

export function CoinsStack({ coinTypes }: CoinsStackProps) {
    return (
        <div className="flex">
            {coinTypes.length > MAX_COINS_TO_DISPLAY && (
                <Text variant="bodySmall" weight="medium" color="steel-dark">
                    +{coinTypes.length - MAX_COINS_TO_DISPLAY}
                </Text>
            )}
            {coinTypes.slice(0, MAX_COINS_TO_DISPLAY).map((coinType, i) => (
                <div key={coinType} className={i === 0 ? '' : '-ml-1'}>
                    <Coin type={coinType} />
                </div>
            ))}
        </div>
    );
}
