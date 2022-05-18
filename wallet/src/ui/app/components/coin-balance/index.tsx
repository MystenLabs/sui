// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo, useMemo } from 'react';
import { useIntl } from 'react-intl';

import st from './CoinBalance.module.scss';

export type CoinProps = {
    type: string;
    balance: number;
};

function CoinBalance({ type, balance }: CoinProps) {
    const symbol = useMemo(
        () => type.substring(type.lastIndexOf(':') + 1),
        [type]
    );
    const intl = useIntl();
    const balanceFormatted = useMemo(
        () =>
            intl.formatNumber(balance, {
                minimumFractionDigits: 2,
                maximumFractionDigits: 2,
            }),
        [intl, balance]
    );
    return (
        <div className={st.container}>
            <span className={st.type}>{type}</span>
            <span>
                <span className={st.value}>{balanceFormatted}</span>
                <span className={st.symbol}>{symbol}</span>
            </span>
        </div>
    );
}

export default memo(CoinBalance);
