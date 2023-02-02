// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    Coin as CoinAPI,
    SUI_TYPE_ARG,
    getTransactionDigest,
} from '@mysten/sui.js';
import { useMutation } from '@tanstack/react-query';
import { Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
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
import { parseAmount } from '_helpers';
import {
    useAppSelector,
    useAppDispatch,
    useCoinDecimals,
    useIndividualCoinMaxBalance,
    useSigner,
} from '_hooks';
import {
    accountAggregateBalancesSelector,
    accountCoinsSelector,
} from '_redux/slices/account';
import { Coin } from '_redux/slices/sui-objects/Coin';
import { trackEvent } from '_src/shared/plausible';
import { useActiveAddress } from '_src/ui/app/hooks/useActiveAddress';
import { useGasBudgetInMist } from '_src/ui/app/hooks/useGasBudgetInMist';
import { fetchAllOwnedAndRequiredObjects } from '_src/ui/app/redux/slices/sui-objects';

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
    const address = useActiveAddress();
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
    const [currentStep, setCurrentStep] = useState<number>(DEFAULT_FORM_STEP);
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
    const validationSchemaStepTwo = useMemo(
        () => createValidationSchemaStepTwo(),
        []
    );

    const signer = useSigner();

    const sendTokens = useMutation(
        ['send-tokens'],
        async ({
            values: { to, amount },
            helpers: { resetForm },
        }: {
            values: FormValues;
            helpers: FormikHelpers<FormValues>;
        }) => {
            if (coinType === null || !gasBudgetEstimationUnits) {
                return;
            }

            if (!address) {
                throw new Error('Error, active address is not defined');
            }

            if (!signer) {
                throw new Error('Missing signer.');
            }

            trackEvent('TransferCoins', {
                props: { coinType },
            });

            const bigIntAmount = parseAmount(amount, coinDecimals);

            const response = await signer.signAndExecuteTransaction(
                await CoinAPI.newPayTransaction(
                    allCoins,
                    coinType,
                    bigIntAmount,
                    to,
                    gasBudgetEstimationUnits
                )
            );

            resetForm();

            return response;
        },
        {
            onSuccess(data) {
                // TODO: Move this to a cache invalidation once we move this to react query.
                dispatch(fetchAllOwnedAndRequiredObjects());

                const txDigest = getTransactionDigest(data!);
                const receiptUrl = `/receipt?txdigest=${encodeURIComponent(
                    txDigest
                )}&transfer=coin`;

                navigate(receiptUrl);
            },
        }
    );

    const dispatch = useAppDispatch();
    const navigate = useNavigate();

    const handleNextStep = useCallback(
        (_: FormValues, { setSubmitting }: FormikHelpers<FormValues>) => {
            setCurrentStep((prev) => prev + 1);
            setSubmitting(false);
        },
        []
    );

    const handleBackStep = useCallback(() => {
        setCurrentStep(DEFAULT_FORM_STEP);
    }, []);

    const handleOnClearSubmitError = useCallback(() => {
        sendTokens.reset();
    }, [sendTokens]);
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
                coinSymbol={coinSymbol}
                coinType={coinType}
                onClearSubmitError={handleOnClearSubmitError}
                onAmountChanged={(anAmount) => setAmountToSend(anAmount)}
            />
        </Formik>
    );

    const StepTwoForm = (
        <Formik
            initialValues={formData}
            validateOnMount={true}
            validationSchema={validationSchemaStepTwo}
            onSubmit={(values, helpers) =>
                sendTokens.mutate({ values, helpers })
            }
        >
            <StepTwo
                submitError={
                    sendTokens.isError ? String(sendTokens.error) : null
                }
                coinSymbol={coinSymbol}
                coinType={coinType}
                gasBudgetEstimation={gasBudgetEstimation || null}
                gasCostEstimation={gasBudgetEstimation || null}
                gasEstimationLoading={isLoading}
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
