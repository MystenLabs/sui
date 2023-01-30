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
import { getEventsSummary, getAmount, notEmpty } from '_helpers';
import { useGetTxnRecipientAddress } from '_hooks';

import type {
    SuiTransactionResponse,
    SuiAddress,
    TransactionEffects,
    SuiEvent,
} from '@mysten/sui.js';

const getTxnEffectsEventID = (
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

export function Transaction({
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

    const amountByRecipient = getAmount(
        certificate.data.transactions[0],
        txn.effects
    );

    const transferAmount = useMemo(() => {
        const amount = amountByRecipient && amountByRecipient?.[0];

        const amountTransfers = eventsSummary.find(
            ({ receiverAddress }) => receiverAddress === address
        );

        return {
            amount: Math.abs(amount?.amount || amountTransfers?.amount || 0),
            coinType:
                amount?.coinType || amountTransfers?.coinType || SUI_TYPE_ARG,
        };
    }, [address, amountByRecipient, eventsSummary]);

    const recipientAddress = useGetTxnRecipientAddress({ txn, address });

    const isSender = address === certificate.data.sender;

    const moveCallTxn = getMoveCallTransaction(
        certificate.data.transactions[0]
    );

    const error = useMemo(() => getExecutionStatusError(txn), [txn]);

    const isSuiTransfer =
        txnKind === 'PaySui' ||
        txnKind === 'TransferSui' ||
        txnKind === 'PayAllSui' ||
        txnKind === 'Pay';

    const isTransfer = isSuiTransfer || txnKind === 'TransferObject';

    const moveCallLabel = useMemo(() => {
        if (txnKind !== 'Call') return null;
        if (
            moveCallTxn?.module === 'sui_system' &&
            moveCallTxn?.function === 'request_add_delegation_mul_coin'
        )
            return 'Staked';
        if (
            moveCallTxn?.module === 'sui_system' &&
            moveCallTxn?.function === 'request_withdraw_delegation'
        )
            return 'Unstaked';
        return 'Call';
    }, [moveCallTxn, txnKind]);

    const txnIcon = useMemo(() => {
        if (txnKind === 'ChangeEpoch') return 'Rewards';
        if (moveCallLabel && moveCallLabel !== 'Call') return moveCallLabel;
        return isSender ? 'Send' : 'Received';
    }, [isSender, moveCallLabel, txnKind]);

    const txnLabel = useMemo(() => {
        if (txnKind === 'ChangeEpoch') return 'Received Staking Rewards';
        if (moveCallLabel) return moveCallLabel;
        return isSender ? 'Sent' : 'Received';
    }, [txnKind, moveCallLabel, isSender]);

    // Show sui symbol only if it is a sui transfer, staking or unstaking
    const showSuiSymbol =
        isSuiTransfer || (moveCallLabel && moveCallLabel !== 'Call');

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
                                <CoinBalance amount={transferAmount.amount} />
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
