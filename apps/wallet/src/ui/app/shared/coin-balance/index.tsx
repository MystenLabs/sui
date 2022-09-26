// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';
import { useIntl } from 'react-intl';

import { balanceFormatOptions } from '_shared/formatting';

import st from './CoinBalance.module.scss';

export type CoinBalanceProps = {
    className?: string;
    balance: bigint;
    symbol: string;
    mode?: 'neutral' | 'positive' | 'negative';
    diffSymbol?: boolean;
    title?: string;
};

function CoinBalance({
    balance,
    symbol,
    className,
    mode = 'neutral',
    diffSymbol = false,
    title,
}: CoinBalanceProps) {
    const intl = useIntl();
    return (
        <div className={cl(className, st.container, st[mode])} title={title}>
            <span>{intl.formatNumber(balance, balanceFormatOptions)}</span>
            <span className={cl(st.symbol, { [st.diffSymbol]: diffSymbol })}>
                {symbol}
            </span>
        </div>
    );
}

export default memo(CoinBalance);
