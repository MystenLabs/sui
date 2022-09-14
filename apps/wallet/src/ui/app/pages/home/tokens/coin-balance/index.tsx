// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import { useMiddleEllipsis } from '_hooks';
import { useCoinFormat } from '_src/ui/app/shared/coin-balance/coin-format';

import st from './CoinBalance.module.scss';

export type CoinProps = {
    type: string;
    balance: bigint;
    hideStake?: boolean;
    mode?: 'row-item' | 'standalone';
};

function CoinBalance({ type, balance, mode = 'row-item' }: CoinProps) {
    const { displayBalance, symbol } = useCoinFormat(balance, type, 'accurate');
    const shortenType = useMiddleEllipsis(type, 30);
    return (
        <div className={cl(st.container, st[mode])}>
            <div className={cl(st.valuesContainer, st[mode])}>
                <span className={cl(st.value, st[mode])}>{displayBalance}</span>
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
