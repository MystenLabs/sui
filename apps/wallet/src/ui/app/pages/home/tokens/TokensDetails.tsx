// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';
import cl from 'classnames';
import { useMemo } from 'react';

import { CoinActivitiesCard } from './CoinActivityCard';
import { TokenIconLink } from './TokenIconLink';
import CoinBalance from './coin-balance';
import IconLink from './icon-link';
import Alert from '_components/alert';
import Loading from '_components/loading';
import { SuiIcons } from '_font-icons/output/sui-icons';
import {
    useAppSelector,
    useObjectsState,
    useGetAllBalance,
    useGetCoinBalance,
} from '_hooks';
import { Coin } from '_redux/slices/sui-objects/Coin';
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

function MyTokens() {
    const accountAddress = useAppSelector(({ account }) => account.address);
    const { data: balance, isLoading: loadingBalances } =
        useGetAllBalance(accountAddress);
    return (
        <Loading loading={loadingBalances}>
            {balance?.length ? (
                <>
                    <div className={st.title}>MY COINS</div>
                    <div className={st.otherCoins}>
                        {balance.map(({ coinType, totalBalance }) => (
                            <CoinBalance
                                type={coinType}
                                balance={totalBalance}
                                key={coinType}
                            />
                        ))}
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
    const { data: coinBalance, isLoading: loadingBalances } = useGetCoinBalance(
        { address: accountAddress, coinType: activeCoinType }
    );

    const tokenBalance = useMemo(() => {
        return coinBalance?.totalBalance || BigInt(0);
    }, [coinBalance]);

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
                            coinBalance?.coinType
                                ? `?${new URLSearchParams({
                                      type: coinBalance?.coinType,
                                  }).toString()}`
                                : ''
                        }`}
                        disabled={!tokenBalance}
                        text="Send"
                    />
                    <IconLink
                        icon={SuiIcons.Swap}
                        to="/"
                        disabled={true}
                        text="Swap"
                    />
                </div>

                {activeCoinType === SUI_TYPE_ARG && accountAddress ? (
                    <TokenIconLink accountAddress={accountAddress} />
                ) : null}

                {!coinType ? (
                    <MyTokens />
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
