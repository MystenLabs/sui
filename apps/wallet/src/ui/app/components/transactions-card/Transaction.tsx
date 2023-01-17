// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusType,
    getTransactionKindName,
    getMoveCallTransaction,
    getExecutionStatusError,
} from '@mysten/sui.js';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { TxnIcon } from './TxnIcon';
import { TxnImage } from './TxnImage';
import { CoinBalance } from '_app/shared/coin-balance';
import { Text } from '_app/shared/text';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { getEventsSummary, getAmount, formatDate } from '_helpers';
import {
    useAppSelector,
    useMiddleEllipsis,
    useGetTransactionById,
} from '_hooks';
import { getTxnEffectsEventID } from '_redux/slices/txresults';
import Alert from '_src/ui/app/components/alert';

import type {
    TransactionKindName,
    SuiTransactionResponse,
    ObjectId,
} from '@mysten/sui.js';

const TRUNCATE_MAX_LENGTH = 8;
const TRUNCATE_PREFIX_LENGTH = 4;

type TxnTypeProps = {
    value?: string;
    variant: TransactionKindName;
    isSender: boolean;
};

function TxnType({ value, variant, isSender }: TxnTypeProps) {
    const address = useMiddleEllipsis(
        value || '',
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );

    const label = useMemo(() => {
        let name;
        switch (variant) {
            case 'Call':
                name = 'Action';
                break;
            case 'ChangeEpoch':
                name = 'Action';
                break;
            case 'Publish':
                name = 'Action';
                break;
            case 'TransferObject':
                name = 'Action';
                break;
            default:
                name = isSender ? 'From' : 'To';
        }
        return name;
    }, [variant, isSender]);

    return (
        <div className="flex gap-1 break-all capitalize">
            <Text color="steel-darker" weight="semibold" variant="subtitle">
                {label}
            </Text>
            <Text
                color="steel-darker"
                weight="normal"
                variant="subtitle"
                mono={label !== 'Action'}
            >
                {label !== 'Action' ? address : value}
            </Text>
        </div>
    );
}

function TxnItem({ txn }: { txn: SuiTransactionResponse }) {
    const address = useAppSelector(({ account: { address } }) => address);
    const { certificate } = txn;
    const executionStatus = getExecutionStatusType(txn) as 'Success' | 'Failed';
    const txnKind = getTransactionKindName(certificate.data.transactions[0]);
    const { coins: eventsSummary } = getEventsSummary(
        txn.effects,
        address || ''
    );

    const objectIds = useMemo(() => {
        return getTxnEffectsEventID(txn.effects, address || '');
    }, [address, txn.effects]);

    const amount = useMemo(() => {
        const amountByRecipient = getAmount(
            certificate.data.transactions[0],
            txn.effects
        );
        const amount = amountByRecipient && amountByRecipient[0]?.amount;
        const amountTransfers = eventsSummary.reduce(
            (acc, { amount }) => acc + amount,
            0
        );

        return Math.abs(amount || amountTransfers);
    }, [certificate.data.transactions, eventsSummary, txn.effects]);

    const recipientAddress = useMemo(() => {
        const receiverAddr =
            eventsSummary &&
            eventsSummary.find(
                ({ receiverAddress }) => receiverAddress !== address
            )?.receiverAddress;

        return receiverAddr;
    }, [address, eventsSummary]);

    const moveCallTxn = getMoveCallTransaction(
        certificate.data.transactions[0]
    );

    const txnLabel = useMemo(() => {
        const moveCallTxn = getMoveCallTransaction(
            certificate.data.transactions[0]
        );
        if (txnKind === 'Call') return moveCallTxn?.function.replace(/_/g, ' ');
        if (txnKind === 'ChangeEpoch') return txnKind;
        return recipientAddress;
    }, [certificate.data.transactions, recipientAddress, txnKind]);

    const isSender = certificate.data.sender === address;
    const isMint = txnKind === 'Call' && moveCallTxn?.function === 'mint';
    const txnIconName = isMint ? 'Minted' : txnKind;

    const txnDate = useMemo(() => {
        return txn?.timestamp_ms
            ? formatDate(txn.timestamp_ms, ['month', 'day', 'hour', 'minute'])
            : false;
    }, [txn]);

    const error = useMemo(() => getExecutionStatusError(txn), [txn]);

    return (
        <div className="flex items-start w-full justify-between gap-3">
            <div className="w-7.5">
                <TxnIcon
                    txnKindName={txnIconName}
                    txnFailed={executionStatus === 'Failed'}
                    isSender={isSender}
                />
            </div>
            <div className="flex flex-col w-full gap-1.5">
                {error ? (
                    <div className="flex flex-col w-full gap-1.5">
                        <Text color="gray-90" weight="semibold">
                            Transaction failed
                        </Text>
                        <div className="flex break-all text-issue-dark text-subtitle">
                            {error}
                        </div>
                    </div>
                ) : (
                    <>
                        <div className="flex w-full justify-between">
                            <Text color="gray-90" weight="semibold">
                                {isMint
                                    ? 'Minted'
                                    : isSender
                                    ? 'Sent'
                                    : 'Received'}
                            </Text>
                            <CoinBalance amount={amount} />
                        </div>

                        {!isMint && (
                            <TxnType
                                variant={txnKind}
                                value={txnLabel}
                                isSender={isSender}
                            />
                        )}
                        {objectIds[0] && <TxnImage id={objectIds[0]} />}
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
    );
}

export function TxnListItem({ txnId }: { txnId: ObjectId }) {
    const { data: txn, isError, isLoading } = useGetTransactionById(txnId);
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

    return isLoading ? (
        <div className="flex w-full justify-start items-center flex-col gap-2 py-4 no-underline">
            <LoadingIndicator />
        </div>
    ) : (
        <Link
            to={`/receipt?${new URLSearchParams({
                txdigest: txnId,
            }).toString()}`}
            className="flex items-center w-full flex-col gap-2 py-4 no-underline"
        >
            <TxnItem txn={txn} />
        </Link>
    );
}
