// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useMemo, useCallback } from 'react';
import { useIntl } from 'react-intl';
import { useNavigate, useSearchParams } from 'react-router-dom';

import { Content } from '_app/shared/bottom-menu-layout';
import PageTitle from '_app/shared/page-title';
import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector } from '_hooks';
import { accountAggregateBalancesSelector } from '_redux/slices/account';
import {
    GAS_TYPE_ARG,
    SUPPORTED_COINS_LIST,
} from '_redux/slices/sui-objects/Coin';
import { balanceFormatOptions } from '_shared/formatting';

import st from './CoinSelector.module.scss';

function CoinsSelectorPage() {
    const [searchParams] = useSearchParams();
    const coinType =
        useMemo(() => searchParams.get('type'), [searchParams]) || GAS_TYPE_ARG;
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
    const navigate = useNavigate();
    const changeConType = useCallback(
        (event: React.MouseEvent<HTMLDivElement>) => {
            const cointype = event.currentTarget.dataset.cointype as string;
            navigate(
                `/send?${new URLSearchParams({
                    type: cointype,
                }).toString()}`
            );
        },
        [navigate]
    );

    return (
        <div className={st.container}>
            <PageTitle
                title="Select Coin"
                backLink={`/send?${new URLSearchParams({
                    type: coinType,
                }).toString()}`}
                className={st.pageTitle}
            />
            <Content className={st.selectorContent}>
                <div className={cl(st.searchCoin, 'sui-icons-search')}>
                    <input
                        type="text"
                        name="name"
                        placeholder="Search coins"
                        className={st.searchInput}
                    />
                </div>
                {coins.map((coin, index) => (
                    <div
                        className={st.coinDetail}
                        key={index}
                        onClick={changeConType}
                        data-cointype={coin.coinType}
                    >
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

export default CoinsSelectorPage;
