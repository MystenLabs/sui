// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin as CoinAPI, getTransactionDigest } from '@mysten/sui.js';
import { useCallback, useMemo, useState } from 'react';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import { FormOverlayStepper, FormStep } from './FormOverlayStepper';
import { StepOne } from './TransferCoinForm/StepOne';
import { StepTwo } from './TransferCoinForm/StepTwo';
import { createValidationSchema } from './validation';
import ActiveCoinsCard from '_components/active-coins-card';
import { parseAmount } from '_helpers';
import { useAppSelector, useAppDispatch, useCoinDecimals } from '_hooks';
import {
    accountAggregateBalancesSelector,
    accountCoinsSelector,
} from '_redux/slices/account';
import { Coin } from '_redux/slices/sui-objects/Coin';
import { sendTokens } from '_redux/slices/transactions';
import { trackEvent } from '_src/shared/plausible';
import { useGasBudgetInMist } from '_src/ui/app/hooks/useGasBudgetInMist';

import type { SerializedError } from '@reduxjs/toolkit';

const initialValues = {
    to: '',
    amount: '',
    gasBudget: '',
    sendMaxToken: false,
};

export type FormValues = typeof initialValues;

// TODO: show out of sync when sui objects locally might be outdated
function TransferCoinPage() {
    const [searchParams] = useSearchParams();
    const coinType = searchParams.get('type');
    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);
    const coinBalance = useMemo(
        () => (coinType && aggregateBalances[coinType]) || BigInt(0),
        [coinType, aggregateBalances]
    );

    const coinSymbol = useMemo(
        () => (coinType && CoinAPI.getCoinSymbol(coinType)) || '',
        [coinType]
    );
    const allCoins = useAppSelector(accountCoinsSelector);
    const allCoinsOfTransferType = useMemo(
        () => allCoins.filter((aCoin) => aCoin.type === coinType),
        [allCoins, coinType]
    );

    const [sendError, setSendError] = useState<string | null>(null);

    const [formData] = useState<FormValues>(initialValues);
    const [coinDecimals] = useCoinDecimals(coinType);

    const [amountToSend, setAmountToSend] = useState(BigInt(0));

    const gasBudgetEstimationUnits = useMemo(
        () => Coin.computeGasBudgetForPay(allCoinsOfTransferType, amountToSend),
        [allCoinsOfTransferType, amountToSend]
    );
    const { gasBudget: gasBudgetEstimation, isLoading } = useGasBudgetInMist(
        gasBudgetEstimationUnits
    );

    const validationSchema = useMemo(
        () => createValidationSchema(coinBalance, coinSymbol, coinDecimals),
        [coinBalance, coinSymbol, coinDecimals]
    );

    const dispatch = useAppDispatch();
    const navigate = useNavigate();

    const onHandleSubmit = useCallback(
        async ({ to, amount, sendMaxToken }: FormValues) =>
            //  { resetForm }: FormikHelpers<FormValues>
            {
                if (coinType === null || !gasBudgetEstimationUnits) {
                    return;
                }

                setSendError(null);
                trackEvent('TransferCoins', {
                    props: { coinType },
                });
                try {
                    const bigIntAmount = parseAmount(amount, coinDecimals);
                    const response = await dispatch(
                        sendTokens({
                            amount: bigIntAmount,
                            recipientAddress: to,
                            tokenTypeArg: coinType,
                            gasBudget: gasBudgetEstimationUnits,
                            // sendMax: sendMaxToken,
                        })
                    ).unwrap();

                    // resetForm();
                    const txDigest = getTransactionDigest(response);
                    const receiptUrl = `/receipt?txdigest=${encodeURIComponent(
                        txDigest
                    )}&from=transactions`;

                    navigate(receiptUrl);
                } catch (e) {
                    setSendError((e as SerializedError).message || null);
                }
            },
        [dispatch, navigate, coinType, coinDecimals, gasBudgetEstimationUnits]
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
        <FormOverlayStepper
            validationSchema={validationSchema}
            initialValues={formData}
            onSubmit={async (values) => onHandleSubmit(values as FormValues)}
        >
            <FormStep
                label="Send Coins"
                validationSchema={validationSchema}
                loading={loadingBalance || isLoading || loadingBalance}
            >
                <ActiveCoinsCard activeCoinType={coinType} />
                <StepOne
                    submitError={sendError}
                    coinType={coinType}
                    balance={coinBalance}
                    gasCostEstimation={gasBudgetEstimation || null}
                    onClearSubmitError={handleOnClearSubmitError}
                    onAmountChanged={(anAmount) => setAmountToSend(anAmount)}
                />
            </FormStep>
            <FormStep label="Review & Send" validationSchema={validationSchema}>
                <StepTwo
                    coinType={coinType}
                    gasCostEstimation={gasBudgetEstimation || null}
                    onClearSubmitError={handleOnClearSubmitError}
                />
            </FormStep>
        </FormOverlayStepper>
    );
}

export default TransferCoinPage;
