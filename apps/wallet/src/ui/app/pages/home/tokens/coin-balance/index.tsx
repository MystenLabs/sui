// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import { useMiddleEllipsis, useFormatCoin } from '_hooks';

import st from './CoinBalance.module.scss';

export type CoinProps = {
    type: string;
    balance: bigint;
    hideStake?: boolean;
    mode?: 'row-item' | 'standalone';
};

function CoinBalance({ type, balance, mode = 'row-item' }: CoinProps) {
    const [formatted, symbol] = useFormatCoin(balance, type);

    const shortenType = useMiddleEllipsis(type, 30);
    return (
        <div className={cl(st.container, st[mode])}>
            <div className={cl(st.valuesContainer, st[mode])}>
                <span className={cl(st.value, st[mode])}>{formatted}</span>
                <span className={cl(st.symbol, st[mode])}>{symbol}</span>
            </div>
            <div className={cl(st.typeActionsContainer, st[mode])}>
                {mode === 'row-item' ? (
                    <span className={st.type} title={type}>
                        {shortenType}
                    </span>
                ) : null}
            </div>
        </div>
    );
}

export default memo(CoinBalance);
