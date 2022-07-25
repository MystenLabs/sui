// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';
import { useIntl } from 'react-intl';

import CoinBalance from '_app/shared/coin-balance';
import Icon, { SuiIcons } from '_components/icon';
import { percentageFormatOptions } from '_shared/formatting';

import st from './StakedCard.module.scss';

export type StakedCardProps = {
    className?: string;
    validator: string;
    rewards?: boolean;
    apy: number;
    balance: bigint;
    symbol: string;
};

function StakedCard({
    className,
    validator,
    rewards = false,
    apy,
    balance,
    symbol,
}: StakedCardProps) {
    const intl = useIntl();
    return (
        <div className={cl(st.container, className)}>
            <div className={st.iconRow}>
                <Icon icon="columns-gap" />
            </div>
            <div className={st.validator}>{validator}</div>
            <div className={st.apy}>
                {intl.formatNumber(apy / 100, percentageFormatOptions)} APY
            </div>
            <div className={st.balance}>
                <CoinBalance
                    balance={balance}
                    symbol={symbol}
                    className={st.balance}
                />
            </div>
            {rewards ? (
                <Icon icon="circle-fill" className={st.rewards} />
            ) : null}
            <Icon icon={SuiIcons.ArrowRight} className={st.arrow} />
        </div>
    );
}

export default memo(StakedCard);
