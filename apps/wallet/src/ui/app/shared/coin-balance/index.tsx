// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import { useCoinFormat } from './coin-format';

import st from './CoinBalance.module.scss';

export type CoinBalanceProps = {
    className?: string;
    balance: bigint;
    coinTypeArg: string;
    mode?: 'neutral' | 'positive' | 'negative';
    diffSymbol?: boolean;
    title?: string;
};

function CoinBalance({
    balance,
    coinTypeArg,
    className,
    mode = 'neutral',
    diffSymbol = false,
    title,
}: CoinBalanceProps) {
    const { displayBalance, symbol } = useCoinFormat(
        balance,
        coinTypeArg,
        'accurate'
    );
    return (
        <div className={cl(className, st.container, st[mode])} title={title}>
            <span>{displayBalance}</span>
            <span className={cl(st.symbol, { [st.diffSymbol]: diffSymbol })}>
                {symbol}
            </span>
        </div>
    );
}

export default memo(CoinBalance);
