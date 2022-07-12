// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo, useMemo } from 'react';
import { useIntl } from 'react-intl';
import { Link } from 'react-router-dom';

import { Coin } from '_redux/slices/sui-objects/Coin';
import { balanceFormatOptions } from '_shared/formatting';

import st from './CoinBalance.module.scss';

export type CoinProps = {
    type: string;
    balance: bigint;
    stake: boolean;
};

function CoinBalance({ type, balance, stake }: CoinProps) {
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

    return (
        <div className={st.container}>
            <span className={st.type} title={type}>
                {type}
            </span>
            <span>
                <span className={st.value}>{balanceFormatted}</span>
                <span className={st.symbol}>{symbol}</span>
            </span>
            <Link className={cl('btn', st.send)} to={sendUrl}>
                Send
            </Link>
            {stake ? (
                <Link className={cl('btn', st.send)} to={stakeUrl}>
                    Stake
                </Link>
            ) : null}
        </div>
    );
}

export default memo(CoinBalance);
