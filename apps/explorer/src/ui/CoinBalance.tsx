// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';

import { useFormatCoin, CoinFormat, formatBalance } from '~/hooks/useFormatCoin';
import { Heading } from '~/ui/Heading';

const coinBalanceStyles = cva('', {
    variants: {
        format: {
            large: 'heading2',
            medium: 'heading6',
        },
    },
    defaultVariants: {
        format: 'medium',
    },
});
export interface CoinBalanceProps
    extends VariantProps<typeof coinBalanceStyles> {
    amount: number | string | bigint;
    symbol?: string | null;
}

const DECIMALS = 0;

// Passing amount as a string or number for optional number suffix
export function CoinBalance({ amount, symbol, format }: CoinBalanceProps) {
    const [formattedAmount, suffix] = useFormatCoin(
        amount,
        symbol,
        CoinFormat.FULL
    );


    const headingSize = coinBalanceStyles({ format }) as 'heading2' | 'heading6';
    const isLarge = format === 'large';
    const weight = isLarge ? 'bold' : 'semibold';
    return (
        <div className="flex items-end gap-1 text-sui-grey-100 break-words">
            <Heading variant={headingSize} weight={weight}>
                {symbol ? formattedAmount : formatBalance(amount, DECIMALS, CoinFormat.ROUNDED)}
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
