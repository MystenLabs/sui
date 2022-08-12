// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';
import { useIntl } from 'react-intl';

import { Content } from '_app/shared/bottom-menu-layout';
import PageTitle from '_app/shared/page-title';
import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector } from '_hooks';
import { accountAggregateBalancesSelector } from '_redux/slices/account';
import { SUPPORTED_COINS_LIST } from '_redux/slices/sui-objects/Coin';
import { balanceFormatOptions } from '_shared/formatting';

import st from './CoinSelection.module.scss';

function CoinsCard() {
    const intl = useIntl();
    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);

    const coins = useMemo(() => {
        return SUPPORTED_COINS_LIST.map((coin) => {
            const balance = intl.formatNumber(
                BigInt(aggregateBalances[coin.coinType] || 0),
                balanceFormatOptions
            );
            return {
                ...coin,
                balance,
            };
        });
    }, [aggregateBalances, intl]);

    return (
        <div className={st.container}>
            <PageTitle
                title="Select Coin"
                backLink="/send"
                className={st.pageTitle}
            />
            <Content className={st.selectorContent}>
                <div className={st.searchCoin}>
                    <input
                        type="text"
                        name="name"
                        placeholder="Search coins"
                        className={st.searchInput}
                    />
                </div>
                {coins.map((coin, index) => (
                    <div className={st.coinDetail} key={index}>
                        <div className={st.coinIcon}>
                            <Icon
                                icon={
                                    SuiIcons[
                                        coin.coinIconName as keyof typeof SuiIcons
                                    ]
                                }
                            />
                        </div>
                        <div className={st.coinLabel}>
                            {coin.coinName} <span>{coin.coinSymbol}</span>
                        </div>
                        <div className={st.coinAmount}>
                            {coin.balance} <span>{coin.coinSymbol}</span>
                        </div>
                    </div>
                ))}
            </Content>
        </div>
    );
}

export default CoinsCard;
