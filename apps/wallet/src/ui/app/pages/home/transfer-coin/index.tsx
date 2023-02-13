// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    Coin as CoinAPI,
    SUI_TYPE_ARG,
    getTransactionDigest,
} from '@mysten/sui.js';
import { Field, Form, useFormikContext, Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import StepOne from './TransferCoinForm/StepOne';
import StepTwo from './TransferCoinForm/StepTwo';
import {
    createValidationSchemaStepOne,
    createValidationSchemaStepTwo,
} from './validation';
import { Content } from '_app/shared/bottom-menu-layout';
import { Text } from '_app/shared/text';
import { ActiveCoinsCard } from '_components/active-coins-card';
import AddressInput from '_components/address-input';
import { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import NumberInput from '_components/number-input';
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

    const handleNextStep = useCallback(
        (_: FormValues, { setSubmitting }: FormikHelpers<FormValues>) => {
            setCurrentStep((prev) => prev + 1);
            setSubmitting(false);
        },
        []
    );

    const closeSendToken = useCallback(() => {
        navigate('/');
    }, [navigate]);

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
            onSubmit={onHandleSubmit}
        >
            <StepTwo
                submitError={sendError}
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
            <Content className={st.content}>
                <Loading loading={loadingBalance}>
                    {steps[currentStep - 1]}
                </Loading>
            </Content>
        </div>
    );

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title="Send Coins"
            closeOverlay={closeSendToken}
            closeIcon={SuiIcons.Close}
        >
            <div className="flex flex-col gap-7.5 mt-3.75">
                <ActiveCoinsCard activeCoinType={coinType} />

                <Formik
                    initialValues={formData}
                    validateOnMount={true}
                    validationSchema={validationSchemaStepOne}
                    onSubmit={onHandleSubmit}
                >
                    <Form autoComplete="off" noValidate>
                        <div className="w-full flex gap-2.5 flex-col">
                            <div className="px-2">
                                <Text
                                    variant="captionSmall"
                                    color="steel-dark"
                                    weight="semibold"
                                >
                                    Select SUI Amount to Send
                                </Text>
                            </div>
                            <div className="w-full flex relative items-center">
                                <Field
                                    component={NumberInput}
                                    allowNegative={false}
                                    name="amount"
                                    placeholder="0.00"
                                    suffix={coinSymbol}
                                    className="w-full py-3.5 px-3 pr-14 flex items-center rounded-2lg text-steel-dark text-body font-semibold bg-white placeholder:text-steel placeholder:font-semibold border border-solid border-gray-45 box-border focus:border-steel transition-all"
                                    decimals
                                />
                                <button
                                    className="absolute right-3 bg-white border border-solid border-gray-60 hover:border-steel-dark rounded-2xl h-6 w-11 flex justify-center items-center cursor-pointer text-steel-darker hover:text-steel-darker text-bodySmall font-medium disabled:opacity-50 disabled:cursor-auto"
                                    type="button"
                                >
                                    Max
                                </button>
                            </div>
                            <div className="w-full flex relative items-center">
                                <Field
                                    component={AddressInput}
                                    allowNegative={false}
                                    name="to"
                                    className="w-full py-3.5 px-3  flex items-center rounded-2lg text-steel-dark text-body font-semibold bg-white placeholder:text-steel placeholder:font-semibold border border-solid border-gray-45 box-border focus:border-steel transition-all"
                                />
                            </div>
                            <StepTwo
                                submitError={sendError}
                                coinSymbol={coinSymbol}
                                coinType={coinType}
                                gasBudgetEstimation={
                                    gasBudgetEstimation || null
                                }
                                gasCostEstimation={gasBudgetEstimation || null}
                                gasEstimationLoading={isLoading}
                                onClearSubmitError={handleOnClearSubmitError}
                            />
                        </div>
                    </Form>
                </Formik>
                {StepOneForm}
            </div>
        </Overlay>
    );
}

export default TransferCoinPage;
