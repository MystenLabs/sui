// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import cl from 'classnames';
import { useMemo } from 'react';

import CoinBalance from './coin-balance';
import IconLink from './icon-link';
import FaucetMessageInfo from '_app/shared/faucet/message-info';
import FaucetRequestButton from '_app/shared/faucet/request-button';
import PageTitle from '_app/shared/page-title';
import AccountAddress from '_components/account-address';
import Alert from '_components/alert';
import Loading from '_components/loading';
import RecentTransactions from '_components/transactions-card/RecentTransactions';
import { SuiIcons } from '_font-icons/output/sui-icons';
import { useAppSelector, useObjectsState } from '_hooks';
import { accountAggregateBalancesSelector } from '_redux/slices/account';
import { GAS_TYPE_ARG, Coin } from '_redux/slices/sui-objects/Coin';
import { FEATURES } from '_src/ui/app/experimentation/features';

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
    allCoinTypes: string[];
    coinBalance: bigint;
    balances: Record<string, bigint>;
    loading: boolean;
};

function MyTokens({
    allCoinTypes,
    coinBalance,
    balances,
    loading,
}: TokensProps) {
    return (
        <Loading loading={loading} className={st.othersLoader}>
            {allCoinTypes.length ? (
                <>
                    <div className={st.title}>MY COINS</div>
                    <div className={st.otherCoins}>
                        {allCoinTypes.map((aCoinType) => (
                            <CoinBalance
                                type={aCoinType}
                                balance={balances[aCoinType] || BigInt(0)}
                                key={aCoinType}
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
                    <div className={st.emptyWalletIcon} />
                    <div className={st.emptyWalletTitle}>
                        Your wallet contains no SUI.
                    </div>
                    {emptyWalletDescription}
                    <FaucetRequestButton trackEventSource="home" />
                </div>
            )}
            <FaucetMessageInfo className={st.gasRequestInfo} />
        </Loading>
    );
}

function TokenDetails({ coinType }: TokenDetailsProps) {
    const { loading, error, showError } = useObjectsState();
    const activeCoinType = coinType || GAS_TYPE_ARG;
    const balances = useAppSelector(accountAggregateBalancesSelector);
    const tokenBalance = balances[activeCoinType] || BigInt(0);
    const allCoinTypes = useMemo(() => Object.keys(balances), [balances]);
    const coinTypeWithBalance =
        coinType || tokenBalance > 0 ? activeCoinType : allCoinTypes[0];

    const coinSymbol = useMemo(
        () => Coin.getCoinSymbol(activeCoinType),
        [activeCoinType]
    );

    const stakingEnabled = useFeature(FEATURES.STAKING_ENABLED).on;

    return (
        <>
            {coinType && (
                <PageTitle
                    title={coinSymbol}
                    backLink="/tokens"
                    hideBackLabel={true}
                />
            )}

            <div className={st.container}>
                {showError && error ? (
                    <Alert className={st.alert}>
                        <strong>Sync error (data might be outdated).</strong>{' '}
                        <small>{error.message}</small>
                    </Alert>
                ) : null}
                {!coinType && <AccountAddress showLink={false} mode="faded" />}
                <div className={st.balanceContainer}>
                    <Loading loading={loading}>
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

                {activeCoinType === GAS_TYPE_ARG ? (
                    <div className={st.staking}>
                        <IconLink
                            icon={SuiIcons.Union}
                            to="/stake"
                            disabled={!stakingEnabled}
                            text="Stake & Earn SUI"
                        />
                    </div>
                ) : null}

                {!coinType ? (
                    <MyTokens
                        allCoinTypes={allCoinTypes}
                        coinBalance={tokenBalance}
                        balances={balances}
                        loading={loading}
                    />
                ) : (
                    <>
                        <div className={cl([st.title, st.tokenActivities])}>
                            {coinSymbol} activity
                        </div>
                        <div className={st.txContent}>
                            <RecentTransactions coinType={activeCoinType} />
                        </div>
                    </>
                )}
            </div>
        </>
    );
}

export default TokenDetails;
