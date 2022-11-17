// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { CoinFormat, formatBalance } from '~/hooks/useFormatCoin';
import { Heading } from '~/ui/Heading';

const SIZE_FORMAT = {
    lg: 'heading2',
    md: 'heading6',
} as const;

export type AmountProps = {
    amount: number | string | bigint;
    symbol?: string | null;
    size?: 'lg' | 'md';
    coinFormat?: keyof typeof CoinFormat;
};

const DECIMALS = 1;

export function Amount({
    amount,
    symbol,
    size = 'md',
    coinFormat,
}: AmountProps) {
    const isLarge = size === 'lg';

    // in stance where getCoinDenominationInfo is not available or amount component is used directly without useFormatCoin hook
    const formattedAmount =
        !symbol || typeof amount === 'bigint'
            ? formatBalance(
                  amount,
                  DECIMALS,
                  CoinFormat[coinFormat ?? CoinFormat.FULL]
              )
            : amount;

    return (
        <div className="flex items-end gap-1 text-sui-grey-100 break-words">
            <Heading
                variant={SIZE_FORMAT[size]}
                weight={isLarge ? 'bold' : 'semibold'}
            >
                {formattedAmount}
            </Heading>
            {symbol && (
                <div className="text-sui-grey-80 text-bodySmall font-medium leading-4">
                    {isLarge ? (
                        <sup className="text-bodySmall">{symbol}</sup>
                    ) : (
                        symbol
                    )}
                </div>
            )}
        </div>
    );
}
