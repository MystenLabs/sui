// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useMemo, useCallback } from 'react';
import { useNavigate, Link } from 'react-router-dom';

import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector, useFormatCoin } from '_hooks';
import { accountAggregateBalancesSelector } from '_redux/slices/account';
import { GAS_TYPE_ARG, Coin } from '_redux/slices/sui-objects/Coin';

import st from './ActiveCoinsCard.module.scss';

interface CoinObject {
    coinName: string;
    coinSymbol: string;
    coinType: string;
    coinIconName: string;
    balance: bigint;
}

function CoinItem({
    coin,
    iconClassName,
    onClick,
}: {
    coin: CoinObject;
    iconClassName: string;
    onClick(event: React.MouseEvent<HTMLDivElement>): void;
}) {
    const [formatted, symbol] = useFormatCoin(coin.balance, coin.coinType);

    return (
        <div
            className={st.coinDetail}
            onClick={onClick}
            data-cointype={coin.coinType}
        >
            <div className={cl(st.coinIcon, iconClassName)}>
                <Icon icon={coin.coinIconName} />
            </div>
            <div className={st.coinLabel}>
                {coin.coinName} <span>{coin.coinSymbol}</span>
            </div>
            <div className={st.coinAmount}>
                {formatted} <span>{symbol}</span>
            </div>
        </div>
    );
}

function SelectedCoinCard({
    coin,
    iconClassName,
}: {
    coin: CoinObject;
    iconClassName: string;
}) {
    const [formatted, symbol] = useFormatCoin(coin.balance, coin.coinType);
    const IconName = coin.coinIconName || SuiIcons.SuiLogoIcon;

    return (
        <div className={st.selectCoin}>
            <Link
                to={`/send/select?${new URLSearchParams({
                    type: coin.coinType,
                }).toString()}`}
                className={st.coin}
            >
                <div className={cl(st.suiIcon, iconClassName)}>
                    <Icon icon={IconName} />
                </div>
                <div className={st.coinLabel}>
                    {coin.coinName}{' '}
                    <span className={st.coinSymbol}>{coin.coinSymbol}</span>
                </div>
                <div className={st.chevron}>
                    <Icon icon={SuiIcons.SuiChevronRight} />
                </div>
            </Link>
            <div className={st.coinBalance}>
                <div className={st.coinBalanceLabel}>Total Available</div>
                <div className={st.coinBalanceValue}>
                    {formatted} {symbol}
                </div>
            </div>
        </div>
    );
}

// Get all the coins that are available in the account.
// default coin type is GAS_TYPE_ARG unless specified in props
// create a list of coins that are available in the account
function ActiveCoinsCard({
    activeCoinType = GAS_TYPE_ARG,
    showActiveCoin = true,
}: {
    activeCoinType: string;
    showActiveCoin?: boolean;
}) {
    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);

    const coins = useMemo(
        () =>
            Object.entries(aggregateBalances).map((aType) => {
                const name = Coin.getCoinSymbol(aType[0]);
                return {
                    coinName: name,
                    coinSymbol: name,
                    coinType: aType[0],
                    //TODO: default coin icon switch to on chain metadata
                    coinIconName:
                        GAS_TYPE_ARG === aType[0]
                            ? SuiIcons.SuiLogoIcon
                            : SuiIcons.Tokens,
                    balance: aType[1],
                } as CoinObject;
            }),
        [aggregateBalances]
    );

    const activeCoin = useMemo(() => {
        return coins.filter((coin) => coin.coinType === activeCoinType)[0];
    }, [activeCoinType, coins]);

    const defaultIconClass =
        GAS_TYPE_ARG !== activeCoin?.coinSymbol ? st.defaultCoin : '';

    const navigate = useNavigate();

    const changeCoinType = useCallback(
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

    const CoinListCard = (
        <div className={st.coinList}>
            {coins.map((coin, index) => (
                <CoinItem
                    key={index}
                    onClick={changeCoinType}
                    coin={coin}
                    iconClassName={defaultIconClass}
                />
            ))}
        </div>
    );

    return (
        <div className={st.content}>
            {showActiveCoin
                ? activeCoin && (
                      <SelectedCoinCard
                          coin={activeCoin}
                          iconClassName={defaultIconClass}
                      />
                  )
                : CoinListCard}
        </div>
    );
}

export default ActiveCoinsCard;
