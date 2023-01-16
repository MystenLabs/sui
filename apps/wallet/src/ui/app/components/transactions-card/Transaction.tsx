// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusType,
    getTransactionKindName,
    getMoveCallTransaction,
    getExecutionStatusError,
} from '@mysten/sui.js';
import cl from 'classnames';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { TxnImage } from './TxnImage';
import { CoinBalance } from '_app/shared/coin-balance';
import { Text } from '_app/shared/text';
import Icon, { SuiIcons } from '_components/icon';
import { getEventsSummary, getAmount, formatDate } from '_helpers';
import { useAppSelector, useMiddleEllipsis } from '_hooks';
import { getTxnEffectsEventID } from '_redux/slices/txresults';

import type {
    TransactionKindName,
    SuiTransactionResponse,
} from '@mysten/sui.js';

const TRUNCATE_MAX_LENGTH = 8;
const TRUNCATE_PREFIX_LENGTH = 4;

interface TxnItemIconProps {
    txnKindName: TransactionKindName | 'Minted';
    txnFailed?: boolean;
    isSender: boolean;
}

function TxnItemIcon({ txnKindName, txnFailed, isSender }: TxnItemIconProps) {
    const variant = useMemo(() => {
        if (txnKindName === 'Minted') return 'Minted';
        return isSender ? 'Send' : 'Receive';
    }, [isSender, txnKindName]);

    const icons = {
        Minted: (
            <Icon icon={SuiIcons.Buy} className="text-gradient-blue-start" />
        ),
        Send: (
            <Icon
                icon={SuiIcons.ArrowLeft}
                className="text-gradient-blue-start rotate-135"
            />
        ),
        Receive: (
            <Icon
                icon={SuiIcons.ArrowLeft}
                className="text-gradient-blue-start -rotate-45"
            />
        ),

        Swapped: (
            <Icon icon={SuiIcons.Swap} className="text-gradient-blue-start" />
        ),
    };

    return (
        <div
            className={cl([
                txnFailed ? 'bg-issue-light' : 'bg-gray-45',
                'w-7.5 h-7.5 flex justify-center items-center rounded-2lg',
            ])}
        >
            {txnFailed ? (
                <Icon
                    icon={SuiIcons.Info}
                    className="text-issue-dark text-body"
                />
            ) : (
                icons[variant]
            )}
        </div>
    );
}

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

export function TxnItem({ txn }: { txn: SuiTransactionResponse }) {
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

    const amountByRecipient = useMemo(
        () => getAmount(certificate.data.transactions[0], txn.effects),
        [certificate.data.transactions, txn.effects]
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
        <Link
            to={`/receipt?${new URLSearchParams({
                txdigest: certificate.transactionDigest,
            }).toString()}`}
            className="flex items-center w-full flex-col gap-2 py-4 no-underline"
        >
            <div className="flex items-start w-full justify-between gap-3">
                <div className="w-7.5">
                    <TxnItemIcon
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
        </Link>
    );
}
