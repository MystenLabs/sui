// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { useIntl } from 'react-intl';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import TransferCoinForm from './TransferCoinForm';
import { createValidationSchema } from './validation';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import PageTitle from '_app/shared/page-title';
import CoinSelection, { CoinsCard } from '_components/coin-selection';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import ProgressBar from '_components/progress-bar';
import { useAppSelector, useAppDispatch } from '_hooks';
import {
    accountAggregateBalancesSelector,
    accountItemizedBalancesSelector,
} from '_redux/slices/account';
import { Coin, GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import { sendTokens } from '_redux/slices/transactions';
import { balanceFormatOptions } from '_shared/formatting';

import type { SerializedError } from '@reduxjs/toolkit';
import type { FormikHelpers } from 'formik';

import st from './TransferCoinPage.module.scss';

const initialValues = {
    to: '',
    amount: '',
};

export type FormValues = typeof initialValues;

// TODO: show out of sync when sui objects locally might be outdated
function TransferCoinPage() {
    const [searchParams] = useSearchParams();
    const [changeSelectedCoin, setChangeSelectedCoin] = useState(true);
    const coinType = useMemo(() => searchParams.get('type'), [searchParams]);
    const balances = useAppSelector(accountItemizedBalancesSelector);
    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);
    const coinBalance = useMemo(
        () => (coinType && aggregateBalances[coinType]) || BigInt(0),
        [coinType, aggregateBalances]
    );
    const totalGasCoins = useMemo(
        () => balances[GAS_TYPE_ARG]?.length || 0,
        [balances]
    );
    const gasAggregateBalance = useMemo(
        () => aggregateBalances[GAS_TYPE_ARG] || BigInt(0),
        [aggregateBalances]
    );

    const coinSymbol = useMemo(
        () => (coinType && Coin.getCoinSymbol(coinType)) || '',
        [coinType]
    );

    const [sendError, setSendError] = useState<string | null>(null);
    const intl = useIntl();
    const validationSchema = useMemo(
        () =>
            createValidationSchema(
                coinType || '',
                coinBalance,
                coinSymbol,
                gasAggregateBalance,
                totalGasCoins,
                intl,
                balanceFormatOptions
            ),
        [
            coinType,
            coinBalance,
            coinSymbol,
            gasAggregateBalance,
            totalGasCoins,
            intl,
        ]
    );
    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    const onHandleSubmit = useCallback(
        async (
            { to, amount }: FormValues,
            { resetForm }: FormikHelpers<FormValues>
        ) => {
            if (coinType === null) {
                return;
            }
            setSendError(null);
            try {
                const response = await dispatch(
                    sendTokens({
                        amount: BigInt(amount),
                        recipientAddress: to,
                        tokenTypeArg: coinType,
                    })
                ).unwrap();
                const txDigest = response.certificate.transactionDigest;
                resetForm();
                navigate(`/tx/${encodeURIComponent(txDigest)}`);
            } catch (e) {
                setSendError((e as SerializedError).message || null);
            }
        },
        [dispatch, navigate, coinType]
    );
    const handleOnClearSubmitError = useCallback(() => {
        setSendError(null);
    }, []);
    const loadingBalance = useAppSelector(
        ({ suiObjects }) => suiObjects.loading && !suiObjects.lastSync
    );
    if (!coinType) {
        return <Navigate to="/" replace={true} />;
    }

    const SendCoin = (
        <div className={st.container}>
            <PageTitle
                title="Send Coins"
                backLink="/"
                className={st.pageTitle}
            />
            <BottomMenuLayout>
                <Content>
                    <Loading loading={loadingBalance}>
                        <ProgressBar
                            currentStep={1}
                            stepsName={['Amount', 'Address']}
                        />
                        <Formik
                            initialValues={initialValues}
                            validateOnMount={true}
                            validationSchema={validationSchema}
                            onSubmit={onHandleSubmit}
                        >
                            <TransferCoinForm
                                submitError={sendError}
                                coinBalance={coinBalance.toString()}
                                coinSymbol={coinSymbol}
                                onClearSubmitError={handleOnClearSubmitError}
                            />
                        </Formik>

                        <CoinSelection />
                    </Loading>
                </Content>
                <Menu stuckClass={st.shadow} className={st.shadow}>
                    <button className={cl('btn', st.btn, 'primary')}>
                        Continue
                        <Icon
                            icon={SuiIcons.ArrowLeft}
                            className={cl(st.arrowLeft)}
                        />
                    </button>
                </Menu>
            </BottomMenuLayout>
        </div>
    );

    return <>{changeSelectedCoin ? <CoinsCard /> : SendCoin}</>;
}

export default TransferCoinPage;
