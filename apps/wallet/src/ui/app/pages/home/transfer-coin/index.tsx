// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16, ArrowLeft16 } from '@mysten/icons';
import {
    Coin as CoinAPI,
    getTransactionDigest,
    SUI_TYPE_ARG,
} from '@mysten/sui.js';
import { useMutation } from '@tanstack/react-query';
import { Formik, type FormikHelpers } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { toast } from 'react-hot-toast';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import { PreviewTransfer } from './PreviewTransfer';
import { SendTokenForm } from './SendTokenForm';
import { createValidationSchemaStepOne } from './validation';
import { Button } from '_app/shared/ButtonUI';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import { ActiveCoinsCard } from '_components/active-coins-card';
import { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { parseAmount } from '_helpers';
import {
    useAppSelector,
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
import { useGasBudgetInMist } from '_src/ui/app/hooks/useGasBudgetInMist';

import type { SerializedError } from '@reduxjs/toolkit';

const initialValues = {
    to: '',
    amount: '',
    isPayAllSui: false,
};

const DEFAULT_FORM_STEP = 1;

export type FormValues = typeof initialValues;

// TODO: show out of sync when sui objects locally might be outdated
function TransferCoinPage() {
    const [searchParams] = useSearchParams();
    const [showModal, setShowModal] = useState(true);

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
    const [currentStep, setCurrentStep] = useState<number>(DEFAULT_FORM_STEP);
    const [formData, setFormData] = useState<FormValues>(initialValues);

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

    const navigate = useNavigate();

    const closeSendToken = useCallback(() => {
        navigate('/');
    }, [navigate]);

    const signer = useSigner();

    const executeTransfer = useMutation({
        mutationFn: async () => {
            if (!signer) throw new Error('Signer not found');
            if (coinType === null || !gasBudgetEstimationUnits) {
                return;
            }
            trackEvent('TransferCoins', {
                props: { coinType },
            });

            // Use payAllSui if sendMax is true and the token type is SUI
            if (formData.isPayAllSui && coinType === SUI_TYPE_ARG) {
                return signer.payAllSui({
                    recipient: formData.to,
                    gasBudget: gasBudgetEstimationUnits,
                    inputCoins: allCoins.map((coin) => CoinAPI.getID(coin)),
                });
            } else {
                const bigIntAmount = parseAmount(formData.amount, coinDecimals);
                return signer.signAndExecuteTransaction(
                    await CoinAPI.newPayTransaction(
                        allCoins,
                        coinType,
                        bigIntAmount,
                        formData.to,
                        gasBudgetEstimationUnits
                    )
                );
            }
        },
        onSuccess: (response) => {
            const txDigest = getTransactionDigest(response!);
            const receiptUrl = `/receipt?txdigest=${encodeURIComponent(
                txDigest
            )}&from=transactions`;
            return navigate(receiptUrl);
        },
        onError: (e) => {
            const errorMsg = (e as SerializedError).message || null;
            toast.error(
                <div className="max-w-xs overflow-hidden flex flex-col">
                    {errorMsg ? (
                        <small className="text-ellipsis overflow-hidden">
                            {errorMsg}
                        </small>
                    ) : null}
                </div>
            );
        },
    });

    const loadingBalance = useAppSelector(
        ({ suiObjects }) => suiObjects.loading && !suiObjects.lastSync
    );

    const handleNextStep = useCallback(
        (
            formData: FormValues,
            { setSubmitting }: FormikHelpers<FormValues>
        ) => {
            setCurrentStep((prev) => prev + 1);
            setFormData(formData);
            setSubmitting(false);
        },
        []
    );

    if (!coinType) {
        return <Navigate to="/" replace={true} />;
    }

    const StepOne = (
        <>
            <div className="mb-7">
                <ActiveCoinsCard activeCoinType={coinType} />
            </div>

            <Formik
                initialValues={formData}
                validationSchema={validationSchemaStepOne}
                onSubmit={handleNextStep}
                enableReinitialize={true}
            >
                <SendTokenForm
                    coinType={coinType}
                    balance={coinBalance}
                    gasCostEstimation={gasBudgetEstimation || null}
                    onAmountChanged={(anAmount) => setAmountToSend(anAmount)}
                />
            </Formik>
        </>
    );

    const StepTwoPreview = (
        <BottomMenuLayout>
            <Content>
                <PreviewTransfer
                    coinType={coinType}
                    amount={formData.amount}
                    to={formData.to}
                    gasCostEstimation={gasBudgetEstimation || null}
                />
            </Content>
            <Menu
                stuckClass="sendCoin-cta"
                className="w-full px-0 pb-0 mx-0 gap-2.5"
            >
                <Button
                    type="button"
                    variant="secondary"
                    onClick={() => setCurrentStep(1)}
                    text={'Back'}
                    before={<ArrowLeft16 />}
                />

                <Button
                    type="button"
                    variant="primary"
                    onClick={() => executeTransfer.mutateAsync()}
                    size="tall"
                    text={'Send Now'}
                    after={<ArrowRight16 />}
                    loading={executeTransfer.isLoading}
                />
            </Menu>
        </BottomMenuLayout>
    );

    const steps = [StepOne, StepTwoPreview];

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={currentStep === 1 ? 'Send Coins' : 'Review & Send'}
            closeOverlay={closeSendToken}
            closeIcon={SuiIcons.Close}
        >
            <Loading loading={loadingBalance || isLoading}>
                <div className="flex flex-col w-full mt-2.5">
                    {steps[currentStep - 1]}
                </div>
            </Loading>
        </Overlay>
    );
}

export default TransferCoinPage;
