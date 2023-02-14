// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16 } from '@mysten/icons';
import {
    Coin as CoinAPI,
    SUI_TYPE_ARG,
    getTransactionDigest,
} from '@mysten/sui.js';
import { Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import { StepOne } from './TransferCoinForm/StepOne';
import { StepTwo } from './TransferCoinForm/StepTwo';
import { createValidationSchemaStepOne } from './validation';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { ActiveCoinsCard } from '_components/active-coins-card';
import { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { parseAmount } from '_helpers';
import {
    useAppSelector,
    useAppDispatch,
    useCoinDecimals,
    useIndividualCoinMaxBalance,
} from '_hooks';
import {
    accountAggregateBalancesSelector,
    accountCoinsSelector,
} from '_redux/slices/account';
import { Coin } from '_redux/slices/sui-objects/Coin';
import { sendTokens } from '_redux/slices/transactions';
import { trackEvent } from '_src/shared/plausible';
import { useGasBudgetInMist } from '_src/ui/app/hooks/useGasBudgetInMist';

import type { SerializedError } from '@reduxjs/toolkit';
import type { FormikHelpers } from 'formik';

const initialValues = {
    to: '',
    amount: '',
};

export type FormValues = typeof initialValues;
const DEFAULT_SEND_STEP = 1;

// TODO: show out of sync when sui objects locally might be outdated
function TransferCoinPage() {
    const [searchParams] = useSearchParams();
    const coinType = searchParams.get('type');
    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);
    const coinBalance = useMemo(
        () => (coinType && aggregateBalances[coinType]) || BigInt(0),
        [coinType, aggregateBalances]
    );
    const gasAggregateBalance = useMemo(
        () => aggregateBalances[SUI_TYPE_ARG] || BigInt(0),
        [aggregateBalances]
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
    const [showModal, setShowModal] = useState(true);
    const [sendError, setSendError] = useState<string | null>(null);

    const [formData] = useState<FormValues>(initialValues);
    const [coinDecimals] = useCoinDecimals(coinType);
    const [gasDecimals] = useCoinDecimals(SUI_TYPE_ARG);
    const [amountToSend, setAmountToSend] = useState(BigInt(0));
    const maxSuiSingleCoinBalance = useIndividualCoinMaxBalance(SUI_TYPE_ARG);
    const gasBudgetEstimationUnits = useMemo(
        () => Coin.computeGasBudgetForPay(allCoinsOfTransferType, amountToSend),
        [allCoinsOfTransferType, amountToSend]
    );
    const { gasBudget: gasBudgetEstimation, isLoading } = useGasBudgetInMist(
        gasBudgetEstimationUnits
    );
    const validationSchemaStepOne = useMemo(
        () =>
            createValidationSchemaStepOne(
                coinType || '',
                coinBalance,
                coinSymbol,
                gasAggregateBalance,
                coinDecimals,
                gasDecimals,
                gasBudgetEstimation || 0,
                maxSuiSingleCoinBalance
            ),
        [
            coinType,
            coinBalance,
            coinSymbol,
            coinDecimals,
            gasDecimals,
            gasAggregateBalance,
            gasBudgetEstimation,
            maxSuiSingleCoinBalance,
        ]
    );

    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    const onHandleSubmit = useCallback(
        async (
            { to, amount }: FormValues,
            { resetForm }: FormikHelpers<FormValues>
        ) => {
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
                    })
                ).unwrap();

                resetForm();
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

    const closeSendToken = useCallback(() => {
        navigate('/');
    }, [navigate]);

    const handleOnClearSubmitError = useCallback(() => {
        setSendError(null);
    }, []);
    const loadingBalance = useAppSelector(
        ({ suiObjects }) => suiObjects.loading && !suiObjects.lastSync
    );

    const [currentStep, setCurrentStep] = useState<number>(DEFAULT_SEND_STEP);
    const handleNextStep = useCallback(
        (_: FormValues, { setSubmitting }: FormikHelpers<FormValues>) => {
            setCurrentStep((prev) => prev + 1);
            setSubmitting(false);
        },
        []
    );

    if (!coinType) {
        return <Navigate to="/" replace={true} />;
    }

    const StepOneForm = (
        <Formik
            initialValues={formData}
            validateOnMount={true}
            validationSchema={validationSchemaStepOne}
            onSubmit={onHandleSubmit}
        >
            {({ isSubmitting, isValid, submitForm, errors, touched }) => (
                <BottomMenuLayout>
                    <Content>
                        <StepOne
                            submitError={sendError}
                            coinType={coinType}
                            balance={coinBalance}
                            gasCostEstimation={gasBudgetEstimation || null}
                            onClearSubmitError={handleOnClearSubmitError}
                            onAmountChanged={(anAmount) =>
                                setAmountToSend(anAmount)
                            }
                        />
                    </Content>
                    <Menu
                        stuckClass="sendCoin-cta"
                        className="w-full px-0 pb-0 mx-0"
                    >
                        <Button
                            type="submit"
                            mode="primary"
                            className="w-full"
                            disabled={
                                !isValid || isSubmitting || !gasBudgetEstimation
                            }
                        >
                            Review <ArrowRight16 />
                        </Button>
                    </Menu>
                </BottomMenuLayout>
            )}
        </Formik>
    );

    const StepTwoForm = (
        <Formik
            initialValues={formData}
            validateOnMount={true}
            onSubmit={onHandleSubmit}
        >
            <StepTwo
                coinType={coinType}
                gasCostEstimation={gasBudgetEstimation || null}
            />
        </Formik>
    );

    const steps = [StepOneForm, StepTwoForm];

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title="Send Coins"
            closeOverlay={closeSendToken}
            closeIcon={SuiIcons.Close}
        >
            <Loading loading={loadingBalance || isLoading || loadingBalance}>
                <div className="flex flex-col gap-7.5 mt-3.75 w-full">
                    <ActiveCoinsCard activeCoinType={coinType} />
                    {steps[1]}
                </div>
            </Loading>
        </Overlay>
    );
}

export default TransferCoinPage;
