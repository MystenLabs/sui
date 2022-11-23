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
    size?: keyof typeof SIZE_FORMAT;
    format?: CoinFormat;
};

const DECIMALS = 0;

export function Amount({ amount, symbol, size = 'md', format }: AmountProps) {
    const isLarge = size === 'lg';

    // TODO: Remove this use-case, we should just enforce usage of this component in a specific way.
    // Instance where getCoinDenominationInfo is not available or amount component is used directly without useFormatCoin hook
    const formattedAmount =
        !symbol || typeof amount === 'bigint' || typeof amount === 'number'
            ? formatBalance(amount, DECIMALS, format ?? CoinFormat.FULL)
            : amount;

    return (
        <div className="flex items-end gap-1 break-words">
            <Heading
                variant={SIZE_FORMAT[size]}
                weight={isLarge ? 'bold' : 'semibold'}
                color="gray-90"
            >
                {formattedAmount}
            </Heading>
            {symbol && (
                <div className="text-steel-dark text-bodySmall font-medium leading-4">
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
