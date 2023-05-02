// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCoinMetadata } from '@mysten/core';
import { Sui, Unstaked } from '@mysten/icons';
import { type CoinMetadata } from '@mysten/sui.js';
import clsx from 'clsx';

function CoinIcon({ coinMetadata }: { coinMetadata?: CoinMetadata | null }) {
    if (coinMetadata?.symbol === 'SUI') {
        return <Sui className="h-2.5 w-2.5" />;
    }

    if (coinMetadata?.iconUrl) {
        return (
            <img
                src={coinMetadata?.iconUrl}
                alt={coinMetadata?.description}
                className="flex h-full w-full items-center justify-center object-contain"
            />
        );
    }

    return <Unstaked className="h-2.5 w-2.5" />;
}

export function Coin({ type }: { type: string }) {
    const { data: coinMetadata } = useCoinMetadata(type);

    return (
        <span
            className={clsx(
                'flex h-5 w-5 items-center justify-center rounded-xl text-white',
                !coinMetadata &&
                    'bg-gradient-to-r from-gradient-blue-start to-gradient-blue-end',
                coinMetadata?.symbol === 'SUI' && 'bg-sui',
                coinMetadata?.iconUrl && 'bg-gray-40'
            )}
        >
            <CoinIcon coinMetadata={coinMetadata} />
        </span>
    );
}

export interface CoinsStackProps {
    coinTypes: string[];
}

export function CoinsStack({ coinTypes }: CoinsStackProps) {
    return (
        <div className="flex">
            {coinTypes.map((coinType, index) => (
                <div key={index} className="-ml-1">
                    <Coin type={coinType} />
                </div>
            ))}
        </div>
    );
}
