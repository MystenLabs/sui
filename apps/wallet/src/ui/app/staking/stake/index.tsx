// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getTransactionDigest } from '@mysten/sui.js';
import BigNumber from 'bignumber.js';
import { Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { Navigate, useNavigate } from 'react-router-dom';

import StakeForm from './StakeForm';
import { createValidationSchema } from './validation';
import Loading from '_components/loading';
import { useAppSelector, useAppDispatch, useCoinDecimals } from '_hooks';
import {
    accountAggregateBalancesSelector,
    accountItemizedBalancesSelector,
} from '_redux/slices/account';
import { Coin, GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import { StakeTokens } from '_redux/slices/transactions';

import type { SerializedError } from '@reduxjs/toolkit';
import type { FormikHelpers } from 'formik';

const initialValues = {
    amount: '',
};

export type FormValues = typeof initialValues;

function StakePage() {
    const coinType = GAS_TYPE_ARG;
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
    const [coinDecimals] = useCoinDecimals(coinType);
    const [gasDecimals] = useCoinDecimals(GAS_TYPE_ARG);
    const validationSchema = useMemo(
        () =>
            createValidationSchema(
                coinType || '',
                coinBalance,
                coinSymbol,
                gasAggregateBalance,
                totalGasCoins,
                coinDecimals,
                gasDecimals
            ),
        [
            coinType,
            coinBalance,
            coinSymbol,
            gasAggregateBalance,
            totalGasCoins,
            coinDecimals,
            gasDecimals,
        ]
    );

    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    const onHandleSubmit = useCallback(
        async (
            { amount }: FormValues,
            { resetForm }: FormikHelpers<FormValues>
        ) => {
            if (coinType === null) {
                return;
            }
            setSendError(null);
            try {
                const bigIntAmount = BigInt(
                    new BigNumber(amount)
                        .shiftedBy(coinDecimals)
                        .integerValue()
                        .toString()
                );

                const response = await dispatch(
                    StakeTokens({
                        amount: bigIntAmount,
                        tokenTypeArg: coinType,
                    })
                ).unwrap();
                const txDigest = getTransactionDigest(response);
                resetForm();
                navigate(`/tx/${encodeURIComponent(txDigest)}`);
            } catch (e) {
                setSendError((e as SerializedError).message || null);
            }
        },
        [dispatch, navigate, coinType, coinDecimals]
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

    return (
        <>
            <h3>Stake {coinSymbol}</h3>
            <Loading loading={loadingBalance}>
                <Formik
                    initialValues={initialValues}
                    validateOnMount={false}
                    validationSchema={validationSchema}
                    onSubmit={onHandleSubmit}
                >
                    <StakeForm
                        submitError={sendError}
                        coinBalance={coinBalance.toString()}
                        coinType={coinType}
                        onClearSubmitError={handleOnClearSubmitError}
                    />
                </Formik>
            </Loading>
        </>
    );
}

export default StakePage;
