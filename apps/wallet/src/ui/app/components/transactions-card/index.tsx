// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusType,
    getTransactionKindName,
    getMoveCallTransaction,
    getExecutionStatusError,
    getTransferObjectTransaction,
    SUI_TYPE_ARG,
    getTransactions,
    getTransactionSender,
    getTransactionDigest,
} from '@mysten/sui.js';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { TxnTypeLabel } from './TxnActionLabel';
import { TxnIcon } from './TxnIcon';
import { TxnImage } from './TxnImage';
import { CoinBalance } from '_app/shared/coin-balance';
import { DateCard } from '_app/shared/date-card';
import { Text } from '_app/shared/text';
import { notEmpty, checkStakingTxn } from '_helpers';
import { useGetTxnRecipientAddress, useGetTransferAmount } from '_hooks';

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
    const [transaction] = getTransactions(txn);
    const executionStatus = getExecutionStatusType(txn);
    const txnKind = getTransactionKindName(transaction);

    const objectId = useMemo(() => {
        const transferId =
            getTransferObjectTransaction(transaction)?.objectRef?.objectId;
        return transferId
            ? transferId
            : getTxnEffectsEventID(txn.effects, address)[0];
    }, [address, transaction, txn.effects]);

    const transfer = useGetTransferAmount({
        txn,
        activeAddress: address,
    });

    // we only show Sui Transfer amount or the first non-Sui transfer amount
    const transferAmount = useMemo(() => {
        // Find SUI transfer amount
        const amountTransfersSui = transfer.find(
            ({ receiverAddress, coinType }) =>
                receiverAddress === address && coinType === SUI_TYPE_ARG
        );

        // Find non-SUI transfer amount
        const amountTransfersNonSui = transfer.find(
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
    }, [address, transfer]);

    const recipientAddress = useGetTxnRecipientAddress({ txn, address });

    const isSender = address === getTransactionSender(txn);

    const moveCallTxn = getMoveCallTransaction(transaction);

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

    // display the transaction icon - depending on the transaction type and amount and label
    const txnIcon = useMemo(() => {
        if (txnKind === 'ChangeEpoch') return 'Rewards';
        if (moveCallLabel && moveCallLabel !== 'Call') return moveCallLabel;
        return isSender ? 'Send' : 'Received';
    }, [isSender, moveCallLabel, txnKind]);

    // Transition label - depending on the transaction type and amount
    // Epoch change without amount is delegation object
    // Special case for staking and unstaking move call transaction,
    // For other transaction show Sent or Received
    const txnLabel = useMemo(() => {
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

    const timestamp = txn.timestamp_ms || txn.timestampMs;

    return (
        <Link
            to={`/receipt?${new URLSearchParams({
                txdigest: getTransactionDigest(txn),
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
                            <Text color="gray-90" weight="medium">
                                Transaction Failed
                            </Text>

                            <div className="flex break-all">
                                <Text
                                    variant="subtitle"
                                    weight="medium"
                                    color="issue-dark"
                                >
                                    {error}
                                </Text>
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

                    {timestamp && <DateCard timestamp={timestamp} size="sm" />}
                </div>
            </div>
        </Link>
    );
}
