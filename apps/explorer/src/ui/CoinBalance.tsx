// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin, CoinFormat } from '~/hooks/useFormatCoin';
import { Amount, type AmountProps } from '~/ui/Amount';

export interface CoinBalanceProps extends AmountProps {
    coinFormat?: keyof typeof CoinFormat;
}

// Passing amount as a string or number for optional number suffix
export function CoinBalance({
    amount,
    symbol,
    size,
    coinFormat,
}: CoinBalanceProps) {
    const [formattedAmount, suffix] = useFormatCoin(
        amount,
        symbol,
        CoinFormat[coinFormat || CoinFormat.FULL]
    );

    const formattedBalance = symbol ? formattedAmount : amount;

    return (
        <Amount
            amount={formattedBalance}
            symbol={suffix}
            size={size}
            coinFormat={coinFormat}
        />
    );
}
