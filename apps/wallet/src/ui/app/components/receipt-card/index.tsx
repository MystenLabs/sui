// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getTransactionKindName,
    getTransactions,
    getTransactionSender,
    getExecutionStatusType,
    getExecutionStatusGasSummary,
    getTotalGasUsed,
} from '@mysten/sui.js';
import cl from 'classnames';
import { useCallback, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { GasCard } from './GasCard';
import { NftMiniCard } from './NftCard';
import { Transfer } from './Transfer';
import { ValidatorCard } from './ValidatorCard';
import { DateCard } from '_app/shared/date-card';
import { Text } from '_app/shared/text';
import Alert from '_components/alert';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import Loading from '_components/loading';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import Overlay from '_components/overlay';
import {
    getEventsPayReceiveSummary,
    getMoveCallMeta,
    getRelatedObjectIds,
    getTxnAmount,
} from '_helpers';
import { useGetTransaction, useAppSelector } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

import st from './ReceiptCard.module.scss';

function ReceiptCard({ txId }: { txId: string }) {
    const [showModal, setShowModal] = useState(true);
    const { data, isLoading, isError } = useGetTransaction(txId);
    const navigate = useNavigate();
    const accountAddress = useAppSelector(({ account }) => account.address);

    const closeReceipt = useCallback(() => {
        navigate('/transactions');
    }, [navigate]);

    if (isError) {
        return (
            <div className="p-2">
                <Alert mode="warning">
                    <div className="mb-1 font-semibold">
                        Something went wrong
                    </div>
                </Alert>
            </div>
        );
    }

    if (isLoading) {
        return (
            <div className="p-2 w-full flex justify-center item-center h-full">
                <LoadingIndicator />
            </div>
        );
    }

    const gasBreakdown = getExecutionStatusGasSummary(data);

    const totalGasUsed = getTotalGasUsed(data);

    const txDetails = getTransactions(data.certificate)[0];
    const txKindName = getTransactionKindName(txDetails);
    const sender = getTransactionSender(data.certificate);
    const txMoveSummery = getEventsPayReceiveSummary(data.effects.events);
    const txCoinSummery = getTxnAmount(txDetails, data.effects);

    const relatedObjectIds = getRelatedObjectIds(
        data.effects.events,
        accountAddress || ''
    );

    const txStatus = getExecutionStatusType(data);
    const walletAddressTxnMeta = txMoveSummery.find(
        ({ receiverAddress }) => receiverAddress === accountAddress
    );

    const transferLabel = accountAddress === sender ? 'Sent' : 'Received';
    const moveMetaInfo = getMoveCallMeta(txDetails);

    const transfersTxt =
        txKindName === 'Call' ? moveMetaInfo?.label : transferLabel;

    const statusClassName = txStatus === 'success' ? st.success : st.failed;

    return (
        <Loading loading={isLoading} className={st.centerLoading}>
            <Overlay
                showModal={showModal}
                setShowModal={setShowModal}
                title={
                    txStatus === 'success'
                        ? `${transfersTxt} ${
                              transfersTxt === 'Move Call'
                                  ? ''
                                  : 'Successfully!'
                          }`
                        : 'Transaction Failed'
                }
                closeOverlay={closeReceipt}
            >
                <div className={cl(st.txnResponse, statusClassName)}>
                    <div className={cl(st.txnResponseStatus, 'gap-3.5')}>
                        <div className={st.statusIcon}></div>
                        {data.timestamp_ms && (
                            <DateCard date={data.timestamp_ms} />
                        )}
                    </div>
                    <div
                        className={cl(
                            st.responseCard,
                            'border border-solid border-gray-45 box-border'
                        )}
                    >
                        {moveMetaInfo?.validatorAddress && accountAddress && (
                            <div className="flex flex-col gap-3.5 !pb-0">
                                <ValidatorCard
                                    validatorAddress={
                                        moveMetaInfo.validatorAddress
                                    }
                                    accountAddress={accountAddress}
                                    amount={walletAddressTxnMeta?.amount || 0}
                                    stakeType={moveMetaInfo.label}
                                    coinType={
                                        walletAddressTxnMeta?.coinType ||
                                        GAS_TYPE_ARG
                                    }
                                />
                            </div>
                        )}
                        {relatedObjectIds &&
                            relatedObjectIds.map((id) => (
                                <div className="flex flex-col gap-2" key={id}>
                                    <Text
                                        variant="body"
                                        weight="medium"
                                        color="steel-darker"
                                    >
                                        {transfersTxt}
                                    </Text>
                                    <NftMiniCard
                                        objectId={id}
                                        fnCallName={
                                            moveMetaInfo?.fnName || null
                                        }
                                        variant="square"
                                    />
                                </div>
                            ))}

                        {txCoinSummery &&
                            accountAddress &&
                            !moveMetaInfo?.validatorAddress &&
                            txCoinSummery.map(
                                ({ recipientAddress, amount, coinType }) => (
                                    <Transfer
                                        key={recipientAddress}
                                        address={recipientAddress}
                                        amount={amount || null}
                                        coinType={coinType || null}
                                        accountAddress={accountAddress}
                                        isSender={transferLabel === 'Sent'}
                                    />
                                )
                            )}

                        {totalGasUsed && gasBreakdown && (
                            <GasCard
                                totalGasUsed={totalGasUsed}
                                computationCost={gasBreakdown.computationCost}
                                storageRebate={gasBreakdown.storageRebate}
                                storageCost={gasBreakdown.storageCost}
                                totalAmount={
                                    txKindName !== 'Call'
                                        ? walletAddressTxnMeta?.amount
                                        : null
                                }
                            />
                        )}
                        <ExplorerLink
                            type={ExplorerLinkType.transaction}
                            transactionID={txId}
                            className="text-sui-dark no-underline font-semibold uppercase text-caption pt-3.5"
                        >
                            View on explorer
                        </ExplorerLink>
                    </div>
                </div>
            </Overlay>
        </Loading>
    );
}
export default ReceiptCard;
