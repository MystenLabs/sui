// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';
import { useIntl } from 'react-intl';

import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector } from '_hooks';
import { accountAggregateBalancesSelector } from '_redux/slices/account';
import {
    GAS_TYPE_ARG,
    SUPPORTED_COINS_LIST,
} from '_redux/slices/sui-objects/Coin';
import { balanceFormatOptions } from '_shared/formatting';

import st from './CoinSelection.module.scss';

// Get all the coins that are available in the account.
// default coin type is GAS_TYPE_ARG unless specified in props
// create a list of coins that are available in the account
function CoinSelection({
    activeCoinType = GAS_TYPE_ARG,
}: {
    activeCoinType?: string;
}) {
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

    const activeCoin = useMemo(() => {
        return coins.filter((coin) => coin.coinType === activeCoinType)[0];
    }, [activeCoinType, coins]);

    const IconName = activeCoin.coinIconName as keyof typeof SuiIcons;

    return (
        <div className={st.content}>
            <div className={st.selectCoin}>
                <div className={st.coin}>
                    <div className={st.suiIcon}>
                        <Icon icon={SuiIcons[IconName]} />
                    </div>
                    <div className={st.coinLabel}>
                        {activeCoin.coinName}{' '}
                        <span className={st.coinSymbol}>
                            {activeCoin.coinSymbol}
                        </span>
                    </div>
                    <div className={st.chevron}>
                        <Icon icon={SuiIcons.SuiChevronRight} />
                    </div>
                </div>
                <div className={st.coinBalance}>
                    <div className={st.coinBalanceLabel}>Total Available</div>
                    <div className={st.coinBalanceValue}>
                        {activeCoin.balance} {activeCoin.coinSymbol}
                    </div>
                </div>
            </div>
        </div>
    );
}

export default CoinSelection;
export { default as CoinsCard } from './CoinCard';
