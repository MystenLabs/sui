// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusType,
    getTransactionKindName,
    getMoveCallTransaction,
    getExecutionStatusError,
    getTransferObjectTransaction,
} from '@mysten/sui.js';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { TxnTypeLabel } from './TxnActionLabel';
import { TxnIcon } from './TxnIcon';
import { TxnImage } from './TxnImage';
import { CoinBalance } from '_app/shared/coin-balance';
import { Text } from '_app/shared/text';
import { getEventsSummary, getAmount, formatDate } from '_helpers';
import { useMiddleEllipsis } from '_hooks';
import { getTxnEffectsEventID } from '_redux/slices/txresults';

import type { SuiTransactionResponse, SuiAddress } from '@mysten/sui.js';

const TRUNCATE_MAX_LENGTH = 8;
const TRUNCATE_PREFIX_LENGTH = 4;

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
    const { coins: eventsSummary } = getEventsSummary(
        txn.effects,
        address || ''
    );

    const objectId = useMemo(() => {
        const transferId = getTransferObjectTransaction(
            certificate.data.transactions[0]
        )?.objectRef?.objectId;
        return transferId
            ? transferId
            : getTxnEffectsEventID(txn.effects, address || '')[0];
    }, [address, certificate.data.transactions, txn.effects]);

    const amountByRecipient = getAmount(
        certificate.data.transactions[0],
        txn.effects
    );

    const amount = useMemo(() => {
        const amount = amountByRecipient && amountByRecipient[0]?.amount;
        const amountTransfers = eventsSummary.reduce(
            (acc, { amount }) => acc + amount,
            0
        );

        return Math.abs(amount || amountTransfers);
    }, [amountByRecipient, eventsSummary]);

    const recipientAddress = useMemo(() => {
        const tranferObjectRecipientAddress =
            amountByRecipient &&
            amountByRecipient?.find(
                ({ recipientAddress }) => recipientAddress !== address
            )?.recipientAddress;
        const receiverAddr =
            eventsSummary &&
            eventsSummary.find(
                ({ receiverAddress }) => receiverAddress !== address
            )?.receiverAddress;

        return (
            receiverAddr ??
            tranferObjectRecipientAddress ??
            certificate.data.sender
        );
    }, [address, amountByRecipient, certificate.data.sender, eventsSummary]);

    // receiving transaction from move contract sometimes has the sender as the receiver
    // so we need to check if the sender is the same as the address
    const isSender = address === certificate.data.sender;

    const receiverAddress = useMiddleEllipsis(
        recipientAddress || '',
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );

    const txnLabel = useMemo(() => {
        const moveCallTxn = getMoveCallTransaction(
            certificate.data.transactions[0]
        );
        if (txnKind === 'Call')
            return moveCallTxn?.function.replace(/_/g, ' ') || 'Call';
        if (txnKind === 'ChangeEpoch') return txnKind;
        return recipientAddress;
    }, [certificate.data.transactions, recipientAddress, txnKind]);

    const txnDate = useMemo(() => {
        return txn?.timestamp_ms
            ? formatDate(txn.timestamp_ms, ['month', 'day', 'hour', 'minute'])
            : false;
    }, [txn]);

    const error = useMemo(() => getExecutionStatusError(txn), [txn]);
    const isSuiTransfer =
        txnKind === 'PaySui' ||
        txnKind === 'TransferSui' ||
        txnKind === 'PayAllSui' ||
        txnKind === 'Pay';

    const label = useMemo(() => {
        return isSuiTransfer || txnKind === 'TransferObject'
            ? isSender
                ? 'To'
                : 'From'
            : 'Action';
    }, [isSender, isSuiTransfer, txnKind]);

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
                        isSender={isSender}
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
                        <>
                            <div className="flex w-full justify-between flex-col gap-1">
                                <div className="flex w-full justify-between ">
                                    <div className="flex gap-1 align-middle  items-baseline">
                                        <Text color="gray-90" weight="semibold">
                                            {isSender ? 'Sent' : 'Received'}
                                        </Text>
                                        {isSuiTransfer && (
                                            <Text
                                                color="gray-90"
                                                weight="normal"
                                                variant="subtitleSmall"
                                            >
                                                SUI
                                            </Text>
                                        )}
                                    </div>

                                    <CoinBalance amount={amount} />
                                </div>
                                <div className="flex flex-col w-full">
                                    <div className="flex flex-col w-full gap-1.5">
                                        <TxnTypeLabel
                                            label={label}
                                            address={receiverAddress}
                                            actionLabel={txnLabel}
                                        />
                                        {objectId && <TxnImage id={objectId} />}
                                    </div>
                                </div>
                            </div>
                        </>
                    )}

                    {txnDate && (
                        <Text
                            color="steel-dark"
                            weight="medium"
                            variant="subtitleSmallExtra"
                        >
                            {txnDate}
                        </Text>
                    )}
                </div>
            </div>
        </Link>
    );
}
