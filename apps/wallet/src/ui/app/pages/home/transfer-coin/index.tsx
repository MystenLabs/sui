// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCoinDecimals } from '@mysten/core';
import { ArrowRight16, ArrowLeft16 } from '@mysten/icons';
import {
    getTransactionDigest,
    SUI_TYPE_ARG,
    Transaction,
} from '@mysten/sui.js';
import * as Sentry from '@sentry/react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { toast } from 'react-hot-toast';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import { PreviewTransfer } from './PreviewTransfer';
import { SendTokenForm } from './SendTokenForm';
import { Button } from '_app/shared/ButtonUI';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import { ActiveCoinsCard } from '_components/active-coins-card';
import Overlay from '_components/overlay';
import { parseAmount } from '_helpers';
import { useSigner } from '_hooks';
import { trackEvent } from '_src/shared/plausible';

import type { SubmitProps } from './SendTokenForm';

function TransferCoinPage() {
    const [searchParams] = useSearchParams();
    const coinType = searchParams.get('type');
    const [showTransactionPreview, setShowTransactionPreview] =
        useState<boolean>(false);
    const [formData, setFormData] = useState<SubmitProps>();
    const navigate = useNavigate();

    const [coinDecimals] = useCoinDecimals(coinType);

    const signer = useSigner();
    const queryClient = useQueryClient();
    const executeTransfer = useMutation({
        mutationFn: async () => {
            if (coinType === null || !signer || !formData) {
                throw new Error('Missing data');
            }

            const transaction = Sentry.startTransaction({
                name: 'send-tokens',
            });
            try {
                trackEvent('TransferCoins', {
                    props: { coinType },
                });

                const tx = new Transaction();
                tx.setGasBudget(formData.gasBudget);

                if (formData.isPayAllSui && coinType === SUI_TYPE_ARG) {
                    tx.transferObjects([tx.gas], tx.pure(formData.to));
                    tx.setGasPayment(
                        formData.coins
                            .filter((coin) => coin.coinType === coinType)
                            .map((coin) => ({
                                objectId: coin.coinObjectId,
                                digest: coin.digest,
                                version: coin.version,
                            }))
                    );

                    return signer.signAndExecuteTransaction(tx, {
                        showInput: true,
                        showEffects: true,
                        showEvents: true,
                    });
                }

                const bigIntAmount = parseAmount(formData.amount, coinDecimals);
                const [primaryCoin, ...coins] = formData.coins.filter(
                    (coin) => coin.coinType === coinType
                );

                if (coinType === SUI_TYPE_ARG) {
                    const coin = tx.splitCoin(tx.gas, tx.pure(bigIntAmount));
                    tx.transferObjects([coin], tx.pure(formData.to));
                } else {
                    const primaryCoinInput = tx.object(
                        primaryCoin.coinObjectId
                    );
                    if (coins.length) {
                        // TODO: This could just merge a subset of coins that meet the balance requirements instead of all of them.
                        tx.mergeCoins(
                            primaryCoinInput,
                            coins.map((coin) => tx.object(coin.coinObjectId))
                        );
                    }
                    const coin = tx.splitCoin(
                        primaryCoinInput,
                        tx.pure(bigIntAmount)
                    );
                    tx.transferObjects([coin], tx.pure(formData.to));
                }

                return signer.signAndExecuteTransaction(tx, {
                    showInput: true,
                    showEffects: true,
                    showEvents: true,
                });
            } catch (error) {
                transaction.setTag('failure', true);
                throw error;
            } finally {
                transaction.finish();
            }
        },
        onSuccess: (response) => {
            queryClient.invalidateQueries(['get-coins']);
            queryClient.invalidateQueries(['coin-balance']);
            const receiptUrl = `/receipt?txdigest=${encodeURIComponent(
                getTransactionDigest(response)
            )}&from=transactions`;
            return navigate(receiptUrl);
        },
        onError: (error) => {
            toast.error(
                <div className="max-w-xs overflow-hidden flex flex-col">
                    <small className="text-ellipsis overflow-hidden">
                        {(error as Error).message || 'Something went wrong'}
                    </small>
                </div>
            );
        },
    });

    if (!coinType) {
        return <Navigate to="/" replace={true} />;
    }

    return (
        <Overlay
            showModal={true}
            title={showTransactionPreview ? 'Review & Send' : 'Send Coins'}
            closeOverlay={() => navigate('/')}
        >
            <div className="flex flex-col w-full mt-2.5">
                {showTransactionPreview && formData ? (
                    <BottomMenuLayout>
                        <Content>
                            <PreviewTransfer
                                coinType={coinType}
                                amount={formData.amount}
                                to={formData.to}
                                gasCostEstimation={formData.gasBudget}
                                approximation={formData.isPayAllSui}
                            />
                        </Content>
                        <Menu
                            stuckClass="sendCoin-cta"
                            className="w-full px-0 pb-0 mx-0 gap-2.5"
                        >
                            <Button
                                type="button"
                                variant="secondary"
                                onClick={() => setShowTransactionPreview(false)}
                                text="Back"
                                before={<ArrowLeft16 />}
                            />

                            <Button
                                type="button"
                                variant="primary"
                                onClick={() => executeTransfer.mutateAsync()}
                                size="tall"
                                text="Send Now"
                                disabled={coinType === null}
                                after={<ArrowRight16 />}
                                loading={executeTransfer.isLoading}
                            />
                        </Menu>
                    </BottomMenuLayout>
                ) : (
                    <>
                        <div className="mb-7">
                            <ActiveCoinsCard activeCoinType={coinType} />
                        </div>

                        <SendTokenForm
                            onSubmit={(formData) => {
                                setShowTransactionPreview(true);
                                setFormData(formData);
                            }}
                            coinType={coinType}
                            initialAmount={formData?.amount || ''}
                            initialTo={formData?.to || ''}
                            initialGasEstimation={formData?.gasBudget || 0}
                        />
                    </>
                )}
            </div>
        </Overlay>
    );
}

export default TransferCoinPage;
