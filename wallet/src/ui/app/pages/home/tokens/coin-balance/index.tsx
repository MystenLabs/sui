// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo, useMemo } from 'react';
import { useIntl } from 'react-intl';
import { Link } from 'react-router-dom';

import { useMiddleEllipsis } from '_hooks';
import { Coin } from '_redux/slices/sui-objects/Coin';
import { balanceFormatOptions } from '_shared/formatting';

import st from './CoinBalance.module.scss';

export type CoinProps = {
    type: string;
    balance: bigint;
    hideStake?: boolean;
    mode?: 'row-item' | 'standalone';
};

function CoinBalance({
    type,
    balance,
    hideStake = false,
    mode = 'row-item',
}: CoinProps) {
    const symbol = useMemo(() => Coin.getCoinSymbol(type), [type]);
    const intl = useIntl();
    const balanceFormatted = useMemo(
        () => intl.formatNumber(balance, balanceFormatOptions),
        [intl, balance]
    );
    const sendUrl = useMemo(
        () => `/send?${new URLSearchParams({ type }).toString()}`,
        [type]
    );
    const stakeUrl = useMemo(
        () => `/stake?${new URLSearchParams({ type }).toString()}`,
        [type]
    );
    // TODO: turn stake feature back on when fix is ready on next release.
    // const showStake = !hideStake && GAS_TYPE_ARG === type;
    const showStake = false;
    const shortenType = useMiddleEllipsis(type, 30);
    return (
        <div className={cl(st.container, st[mode])}>
            <div className={cl(st.valuesContainer, st[mode])}>
                <span className={cl(st.value, st[mode])}>
                    {balanceFormatted}
                </span>
                <span className={cl(st.symbol, st[mode])}>{symbol}</span>
            </div>
            <div className={cl(st.typeActionsContainer, st[mode])}>
                {mode === 'row-item' ? (
                    <span className={st.type} title={type}>
                        {shortenType}
                    </span>
                ) : null}
                <Link className={cl('btn', st.action)} to={sendUrl}>
                    Send
                </Link>
                {showStake ? (
                    <Link className={cl('btn', st.action)} to={stakeUrl}>
                        Stake
                    </Link>
                ) : null}
            </div>
        </div>
    );
}

export default memo(CoinBalance);
