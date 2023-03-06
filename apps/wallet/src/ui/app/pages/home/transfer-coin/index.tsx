// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCoinDecimals } from '@mysten/core';
import { ArrowRight16, ArrowLeft16 } from '@mysten/icons';
import { getTransactionDigest, SUI_TYPE_ARG, Coin } from '@mysten/sui.js';
import * as Sentry from '@sentry/react';
import { useMutation } from '@tanstack/react-query';
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

                // Use payAllSui if sendMax is true and the token type is SUI
                if (formData.isPayAllSui && coinType === SUI_TYPE_ARG) {
                    return signer.signAndExecuteTransaction({
                        kind: 'payAllSui',
                        data: {
                            recipient: formData.to,
                            gasBudget: formData.gasBudget,
                            inputCoins: formData.coinIds,
                        },
                    });
                }

                const bigIntAmount = parseAmount(formData.amount, coinDecimals);

                return signer.signAndExecuteTransaction(
                    await Coin.newPayTransaction(
                        formData.coins,
                        coinType,
                        bigIntAmount,
                        formData.to,
                        formData.gasBudget
                    )
                );
            } catch (error) {
                transaction.setTag('failure', true);
                throw error;
            } finally {
                transaction.finish();
            }
        },
        onSuccess: (response) => {
            const txDigest = getTransactionDigest(response);
            const receiptUrl = `/receipt?txdigest=${encodeURIComponent(
                txDigest
            )}&from=transactions`;
            return navigate(receiptUrl);
        },
        onError: (error) => {
            toast.error(
                <div className="max-w-xs overflow-hidden flex flex-col">
                    {error instanceof Error ? (
                        <small className="text-ellipsis overflow-hidden">
                            {error.message}
                        </small>
                    ) : null}
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
