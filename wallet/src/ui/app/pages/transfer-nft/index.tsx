// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { useIntl } from 'react-intl';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import TransferCoinForm from './TransferCoinForm';
import { createValidationSchema } from './validation';
import Loading from '_components/loading';
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

const initialValues = {
    to: '',
    amount: '',
};

export type FormValues = typeof initialValues;

function TransferNFTPage() {
    const [searchParams] = useSearchParams();
    const objectId = useMemo(
        () => searchParams.get('objectId'),
        [searchParams]
    );

    const balances = useAppSelector(accountItemizedBalancesSelector);
    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);
    const coinBalance = useMemo(
        () => (objectId && aggregateBalances[objectId]) || BigInt(0),
        [objectId, aggregateBalances]
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
        () => (objectId && Coin.getCoinSymbol(objectId)) || '',
        [objectId]
    );
    const [sendError, setSendError] = useState<string | null>(null);
    const intl = useIntl();
    const validationSchema = useMemo(
        () =>
            createValidationSchema(
                objectId || '',
                coinBalance,
                coinSymbol,
                gasAggregateBalance,
                totalGasCoins,
                intl,
                balanceFormatOptions
            ),
        [
            objectId,
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
            if (objectId === null) {
                return;
            }
            setSendError(null);
            try {
                const response = await dispatch(
                    sendTokens({
                        amount: BigInt(amount),
                        recipientAddress: to,
                        tokenTypeArg: objectId,
                    })
                ).unwrap();
                const txDigest =
                    response.EffectResponse.certificate.transactionDigest;
                resetForm();
                navigate(`/tx/${encodeURIComponent(txDigest)}`);
            } catch (e) {
                setSendError((e as SerializedError).message || null);
            }
        },
        [dispatch, navigate, objectId]
    );
    const handleOnClearSubmitError = useCallback(() => {
        setSendError(null);
    }, []);
    const loadingBalance = useAppSelector(
        ({ suiObjects }) => suiObjects.loading && !suiObjects.lastSync
    );
    if (!objectId) {
        return <Navigate to="/nfts" replace={true} />;
    }
    return (
        <>
            <h3>Send {coinSymbol}</h3>
            <Loading loading={loadingBalance}>
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
            </Loading>
        </>
    );
}

export default TransferNFTPage;
