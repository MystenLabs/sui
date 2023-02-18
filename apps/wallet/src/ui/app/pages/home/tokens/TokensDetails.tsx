// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    SUI_TYPE_ARG,
    type CoinBalance as CoinBalanceType,
} from '@mysten/sui.js';
import cl from 'classnames';
import { useMemo } from 'react';

import { CoinActivitiesCard } from './CoinActivityCard';
import { TokenIconLink } from './TokenIconLink';
import CoinBalance from './coin-balance';
import IconLink from './icon-link';
import Alert from '_components/alert';
import Loading from '_components/loading';
import { SuiIcons } from '_font-icons/output/sui-icons';
import { useAppSelector, useObjectsState, useGetAllBalance } from '_hooks';
import { GAS_TYPE_ARG, Coin } from '_redux/slices/sui-objects/Coin';
import { AccountSelector } from '_src/ui/app/components/AccountSelector';
import PageTitle from '_src/ui/app/shared/PageTitle';
import FaucetRequestButton from '_src/ui/app/shared/faucet/FaucetRequestButton';

import st from './TokensPage.module.scss';

type TokenDetailsProps = {
    coinType?: string;
};

const emptyWalletDescription = (
    <div className={st.emptyWalletDescription}>
        To conduct transactions on the Sui network, you need SUI in your wallet.
    </div>
);

type TokensProps = {
    coinBalance: bigint | number;
    balances: CoinBalanceType[] | [];
    loading: boolean;
};

function MyTokens({ coinBalance, balances, loading }: TokensProps) {
    return (
        <Loading loading={loading}>
            {balances.length ? (
                <>
                    <div className={st.title}>MY COINS</div>
                    <div className={st.otherCoins}>
                        {balances.map(({ coinType, totalBalance }) => (
                            <CoinBalance
                                type={coinType}
                                balance={totalBalance}
                                key={coinType}
                            />
                        ))}
                        {coinBalance <= 0 ? (
                            <div className={st.emptyWallet}>
                                <FaucetRequestButton trackEventSource="home" />
                                {emptyWalletDescription}
                            </div>
                        ) : null}
                    </div>
                </>
            ) : (
                <div className={st.emptyWallet}>
                    <FaucetRequestButton trackEventSource="home" />
                    {emptyWalletDescription}
                </div>
            )}
        </Loading>
    );
}

function TokenDetails({ coinType }: TokenDetailsProps) {
    const { loading, error, showError } = useObjectsState();
    const activeCoinType = coinType || SUI_TYPE_ARG;
    const accountAddress = useAppSelector(({ account }) => account.address);
    const { data: coinBalance, isLoading: loadingBalances } = useGetAllBalance({
        address: accountAddress || '',
    });

    const tokenBalance = useMemo(() => {
        if (!coinBalance) return BigInt(0);
        return (
            coinBalance.find((coin) => coin.coinType === activeCoinType)
                ?.totalBalance || BigInt(0)
        );
    }, [activeCoinType, coinBalance]);

    const allCoinTypes = useMemo(() => {
        if (!coinBalance) return [];
        return coinBalance.map((coin) => coin.coinType);
    }, [coinBalance]);

    const coinTypeWithBalance =
        coinType || tokenBalance > 0 ? activeCoinType : allCoinTypes[0];

    const coinSymbol = useMemo(
        () => Coin.getCoinSymbol(activeCoinType),
        [activeCoinType]
    );

    return (
        <>
            {coinType && <PageTitle title={coinSymbol} back="/tokens" />}

            <div className={st.container} data-testid="coin-page">
                {showError && error ? (
                    <Alert className={st.alert}>
                        <div>
                            <strong>Sync error (data might be outdated)</strong>
                        </div>
                        <small>{error.message}</small>
                    </Alert>
                ) : null}
                {!coinType && <AccountSelector />}
                <div className={st.balanceContainer}>
                    <Loading loading={loading || loadingBalances}>
                        <CoinBalance
                            balance={tokenBalance}
                            type={activeCoinType}
                            mode="standalone"
                        />
                    </Loading>
                </div>
                <div className={st.actions}>
                    <IconLink
                        icon={SuiIcons.Buy}
                        to="/"
                        disabled={true}
                        text="Buy"
                    />
                    <IconLink
                        icon={SuiIcons.ArrowLeft}
                        to={`/send${
                            coinTypeWithBalance
                                ? `?${new URLSearchParams({
                                      type: coinTypeWithBalance,
                                  }).toString()}`
                                : ''
                        }`}
                        disabled={!coinTypeWithBalance}
                        text="Send"
                    />
                    <IconLink
                        icon={SuiIcons.Swap}
                        to="/"
                        disabled={true}
                        text="Swap"
                    />
                </div>

                {activeCoinType === GAS_TYPE_ARG && accountAddress ? (
                    <TokenIconLink accountAddress={accountAddress} />
                ) : null}

                {!coinType ? (
                    <MyTokens
                        coinBalance={tokenBalance}
                        balances={coinBalance || []}
                        loading={loading}
                    />
                ) : (
                    <>
                        <div className={cl([st.title, st.tokenActivities])}>
                            {coinSymbol} activity
                        </div>
                        <div className={st.txContent}>
                            <CoinActivitiesCard coinType={activeCoinType} />
                        </div>
                    </>
                )}
            </div>
        </>
    );
}

export default TokenDetails;
