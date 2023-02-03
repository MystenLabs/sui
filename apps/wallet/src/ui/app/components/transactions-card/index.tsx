// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusType,
    getTransactionKindName,
    getMoveCallTransaction,
    getExecutionStatusError,
    getTransferObjectTransaction,
    SUI_TYPE_ARG,
} from '@mysten/sui.js';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { TxnTypeLabel } from './TxnActionLabel';
import { TxnIcon } from './TxnIcon';
import { TxnImage } from './TxnImage';
import { CoinBalance } from '_app/shared/coin-balance';
import { DateCard } from '_app/shared/date-card';
import { Text } from '_app/shared/text';
import { getEventsSummary, notEmpty, checkStakingTxn } from '_helpers';
import { useGetTxnRecipientAddress } from '_hooks';

import type {
    SuiTransactionResponse,
    SuiAddress,
    TransactionEffects,
    SuiEvent,
} from '@mysten/sui.js';

export const getTxnEffectsEventID = (
    txEffects: TransactionEffects,
    address: string
): string[] => {
    const events = txEffects?.events || [];
    const objectIDs = events
        ?.map((event: SuiEvent) => {
            const data = Object.values(event).find(
                (itm) => itm?.recipient?.AddressOwner === address
            );
            return data?.objectId;
        })
        .filter(notEmpty);
    return objectIDs;
};

export function TransactionCard({
    txn,
    address,
}: {
    txn: SuiTransactionResponse;
    address: SuiAddress;
}) {
    const { certificate } = txn;
    const executionStatus = getExecutionStatusType(txn);
    const txnKind = getTransactionKindName(certificate.data.transactions[0]);
    const { coins: eventsSummary } = getEventsSummary(txn.effects, address);

    const objectId = useMemo(() => {
        const transferId = getTransferObjectTransaction(
            certificate.data.transactions[0]
        )?.objectRef?.objectId;
        return transferId
            ? transferId
            : getTxnEffectsEventID(txn.effects, address)[0];
    }, [address, certificate.data.transactions, txn.effects]);

    // we only show Sui Transfer amount or the first non-Sui transfer amount
    // positive amount means received, negative amount means sent
    const transferAmount = useMemo(() => {
        // Find SUI transfer amount
        const amountTransfersSui = eventsSummary.find(
            ({ receiverAddress, coinType }) =>
                receiverAddress === address && coinType === SUI_TYPE_ARG
        );

        // Find non-SUI transfer amount
        const amountTransfersNonSui = eventsSummary.find(
            ({ receiverAddress, coinType }) =>
                receiverAddress === address && coinType !== SUI_TYPE_ARG
        );

        return {
            amount:
                amountTransfersSui?.amount ||
                amountTransfersNonSui?.amount ||
                null,
            coinType:
                amountTransfersSui?.coinType ||
                amountTransfersNonSui?.coinType ||
                null,
        };
    }, [address, eventsSummary]);

    const recipientAddress = useGetTxnRecipientAddress({ txn, address });

    // sometime sender and receiver are the same address
    // for txn with amount determine sender or receiver by amount. negative amount means sender and positive amount means receiver
    // fall back to address comparison if amount is not available
    const isSender = transferAmount.amount
        ? transferAmount.amount < 0
        : address === certificate.data.sender;

    const moveCallTxn = getMoveCallTransaction(
        certificate.data.transactions[0]
    );

    const error = useMemo(() => getExecutionStatusError(txn), [txn]);

    const isSuiTransfer =
        txnKind === 'PaySui' ||
        txnKind === 'TransferSui' ||
        txnKind === 'PayAllSui';

    const isTransfer =
        isSuiTransfer || txnKind === 'Pay' || txnKind === 'TransferObject';

    const moveCallLabel = useMemo(() => {
        if (txnKind !== 'Call') return null;
        return checkStakingTxn(txn) || txnKind;
    }, [txn, txnKind]);

    const txnIcon = useMemo(() => {
        if (txnKind === 'ChangeEpoch') return 'Rewards';
        if (moveCallLabel && moveCallLabel !== 'Call') return moveCallLabel;
        return isSender ? 'Send' : 'Received';
    }, [isSender, moveCallLabel, txnKind]);

    // Transition label
    const txnLabel = useMemo(() => {
        // Epoch change with amount is staking rewards and without amount is delegation object
        if (txnKind === 'ChangeEpoch')
            return transferAmount.amount
                ? 'Received Staking Rewards'
                : 'Received Delegation Object';
        if (moveCallLabel) return moveCallLabel;
        return isSender ? 'Sent' : 'Received';
    }, [txnKind, transferAmount.amount, moveCallLabel, isSender]);

    // Show sui symbol only if transfer transferAmount coinType is SUI_TYPE_ARG, staking or unstaking
    const showSuiSymbol =
        (transferAmount.coinType === SUI_TYPE_ARG && isSuiTransfer) ||
        moveCallLabel === 'Staked' ||
        moveCallLabel === 'Unstaked';

    return (
        <Link
            to={`/receipt?${new URLSearchParams({
                txdigest: txn.certificate.transactionDigest,
            }).toString()}`}
            className="flex items-center w-full flex-col gap-2 py-4 no-underline"
        >
            <div className="flex items-start w-full justify-between gap-3">
                <div className="w-7.5">
                    <TxnIcon
                        txnFailed={executionStatus !== 'success' || !!error}
                        variant={txnIcon}
                    />
                </div>
                <div className="flex flex-col w-full gap-1.5">
                    {error ? (
                        <div className="flex flex-col w-full gap-1.5">
                            <Text color="gray-90" weight="semibold">
                                Transaction Failed
                            </Text>
                            <div className="flex break-all text-issue-dark text-subtitle">
                                {error}
                            </div>
                        </div>
                    ) : (
                        <div className="flex w-full justify-between flex-col ">
                            <div className="flex w-full justify-between">
                                <div className="flex gap-1 align-middle items-baseline">
                                    <Text color="gray-90" weight="semibold">
                                        {txnLabel}
                                    </Text>
                                    {showSuiSymbol && (
                                        <Text
                                            color="gray-90"
                                            weight="normal"
                                            variant="subtitleSmall"
                                        >
                                            SUI
                                        </Text>
                                    )}
                                </div>
                                {transferAmount.coinType &&
                                    transferAmount.amount && (
                                        <CoinBalance
                                            amount={Math.abs(
                                                transferAmount.amount
                                            )}
                                            coinType={transferAmount.coinType}
                                        />
                                    )}
                            </div>
                            <div className="flex flex-col w-full gap-1.5">
                                <TxnTypeLabel
                                    address={recipientAddress}
                                    moveCallFnName={moveCallTxn?.function}
                                    isSender={isSender}
                                    isTransfer={isTransfer}
                                />
                                {objectId && <TxnImage id={objectId} />}
                            </div>
                        </div>
                    )}

                    {txn.timestamp_ms && (
                        <DateCard timestamp={txn.timestamp_ms} size="sm" />
                    )}
                </div>
            </div>
        </Link>
    );
}
