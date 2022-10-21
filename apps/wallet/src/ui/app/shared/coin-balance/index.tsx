// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import { useFormatCoin } from '_hooks';

import st from './CoinBalance.module.scss';

export type CoinBalanceProps = {
    className?: string;
    balance: bigint;
    type: string;
    mode?: 'neutral' | 'positive' | 'negative';
    diffSymbol?: boolean;
    title?: string;
};

function CoinBalance({
    balance,
    type,
    className,
    mode = 'neutral',
    diffSymbol = false,
    title,
}: CoinBalanceProps) {
    const [formatted, symbol] = useFormatCoin(balance, type);

    return (
        <div className={cl(className, st.container, st[mode])} title={title}>
            <span>{formatted}</span>
            <span className={cl(st.symbol, { [st.diffSymbol]: diffSymbol })}>
                {symbol}
            </span>
        </div>
    );
}

export default memo(CoinBalance);
