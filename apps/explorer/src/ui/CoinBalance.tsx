// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    useFormatCoin,
    CoinFormat,
    formatBalance,
} from '~/hooks/useFormatCoin';
import { Heading } from '~/ui/Heading';

export interface CoinBalanceProps {
    amount: number | string | bigint;
    symbol?: string | null;
    size?: 'lg' | 'md';
    coinFormat?: keyof typeof CoinFormat;
}

const DECIMALS = 0;

const SIZE_FORMAT = {
    lg: 'heading2',
    md: 'heading6',
} as const;

// Passing amount as a string or number for optional number suffix
export function CoinBalance({
    amount,
    symbol,
    size = 'md',
    coinFormat = CoinFormat.FULL,
}: CoinBalanceProps) {
    const [formattedAmount, suffix] = useFormatCoin(
        amount,
        symbol,
        CoinFormat[coinFormat]
    );

    const isLarge = size === 'lg';

    return (
        <div className="flex items-end gap-1 text-sui-grey-100 break-words">
            <Heading
                variant={SIZE_FORMAT[size]}
                weight={isLarge ? 'bold' : 'semibold'}
            >
                {symbol
                    ? formattedAmount
                    : formatBalance(amount, DECIMALS, CoinFormat[coinFormat])}
            </Heading>
            {symbol && (
                <div className="text-sui-grey-80 text-bodySmall font-medium leading-4">
                    {isLarge ? (
                        <sup className="text-bodySmall">{suffix}</sup>
                    ) : (
                        suffix
                    )}
                </div>
            )}
        </div>
    );
}
