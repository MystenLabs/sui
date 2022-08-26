// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { useIntl } from 'react-intl';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import StepOne from './TransferCoinForm/StepOne';
import StepTwo from './TransferCoinForm/StepTwo';
import {
    createValidationSchemaStepOne,
    createValidationSchemaStepTwo,
} from './validation';
import { Content } from '_app/shared/bottom-menu-layout';
import PageTitle from '_app/shared/page-title';
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

const DEFAULT_FORM_STEP = 1;

// TODO: show out of sync when sui objects locally might be outdated
function TransferCoinPage() {
    const [searchParams] = useSearchParams();
    const coinType = searchParams.get('type');

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
    const [currentStep, setCurrentStep] = useState<number>(DEFAULT_FORM_STEP);
    const [formData] = useState<FormValues>(initialValues);

    const intl = useIntl();
    const validationSchemaStepOne = useMemo(
        () =>
            createValidationSchemaStepOne(
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
    const validationSchemaStepTwo = useMemo(
        () => createValidationSchemaStepTwo(),
        []
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

                resetForm();
                const txDigest = response.certificate.transactionDigest;
                const recieptUrl = `/receipt?txdigest=${encodeURIComponent(
                    txDigest
                )}&transfer=coin`;

                navigate(recieptUrl);
            } catch (e) {
                setSendError((e as SerializedError).message || null);
            }
        },
        [dispatch, navigate, coinType]
    );

    const handleNextStep = useCallback(
        (
            { amount }: FormValues,
            { setSubmitting, setFieldValue }: FormikHelpers<FormValues>
        ) => {
            setCurrentStep((prev) => prev + 1);
            setSubmitting(false);
        },
        []
    );

    const handleBackStep = useCallback(() => {
        setCurrentStep(DEFAULT_FORM_STEP);
    }, []);

    const handleOnClearSubmitError = useCallback(() => {
        setSendError(null);
    }, []);
    const loadingBalance = useAppSelector(
        ({ suiObjects }) => suiObjects.loading && !suiObjects.lastSync
    );

    if (!coinType) {
        return <Navigate to="/" replace={true} />;
    }

    const StepOneForm = (
        <Formik
            initialValues={formData}
            validateOnMount={true}
            validationSchema={validationSchemaStepOne}
            onSubmit={handleNextStep}
        >
            <StepOne
                submitError={sendError}
                coinBalance={coinBalance.toString()}
                coinSymbol={coinSymbol}
                coinType={coinType}
                onClearSubmitError={handleOnClearSubmitError}
            />
        </Formik>
    );

    const StepTwoForm = (
        <Formik
            initialValues={formData}
            validateOnMount={true}
            validationSchema={validationSchemaStepTwo}
            onSubmit={onHandleSubmit}
        >
            <StepTwo
                submitError={sendError}
                coinBalance={coinBalance.toString()}
                coinSymbol={coinSymbol}
                coinType={coinType}
                onClearSubmitError={handleOnClearSubmitError}
            />
        </Formik>
    );

    const steps = [StepOneForm, StepTwoForm];

    const SendCoin = (
        <div className={st.container}>
            <PageTitle
                title="Send Coins"
                backLink={'/'}
                className={st.pageTitle}
                {...(currentStep > 1 && { onClick: handleBackStep })}
            />

            <Content className={st.content}>
                <Loading loading={loadingBalance}>
                    <ProgressBar
                        currentStep={currentStep}
                        stepsName={['Amount', 'Address']}
                    />
                    {steps[currentStep - 1]}
                </Loading>
            </Content>
        </div>
    );

    return <>{SendCoin}</>;
}

export default TransferCoinPage;
