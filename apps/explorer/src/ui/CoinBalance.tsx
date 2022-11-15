// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    useFormatCoin,
    CoinFormat,
    formatBalance,
} from '~/hooks/useFormatCoin';
import { Amount } from '~/ui/Amount';

export interface CoinBalanceProps {
    amount: number | string | bigint;
    symbol?: string | null;
    size?: 'lg' | 'md';
    coinFormat?: keyof typeof CoinFormat;
}

const DECIMALS = 0;

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

    const formattedBalance = symbol
        ? formattedAmount
        : formatBalance(amount, DECIMALS, CoinFormat[coinFormat]);

    return <Amount amount={formattedBalance} symbol={suffix} size={size} />;
}
