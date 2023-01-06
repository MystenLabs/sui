// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getTransactionDigest,
    getTransactions,
    getTransactionKindName,
    getTransferObjectTransaction,
    getExecutionStatusType,
    getTotalGasUsed,
    getTransferSuiTransaction,
    getExecutionStatusError,
    getMoveCallTransaction,
    getTransactionSender,
    getObjectId,
    getObjectFields,
    Coin,
    is,
    SuiObject,
    getPaySuiTransaction,
    getPayTransaction,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { useEffect, useMemo } from 'react';

import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import TransactionCard from '_components/transactions-card';
import { notEmpty, getEventsSummary } from '_helpers';
import { useAppSelector, useAppDispatch, useRpc } from '_hooks';
import { getTransactionsByAddress } from '_redux/slices/txresults';

import type {
    GetTxnDigestsResponse,
    TransactionKindName,
    ExecutionStatusType,
    TransactionEffects,
    SuiEvent,
    SuiTransactionKind,
} from '@mysten/sui.js';
import type { TxResultState } from '_redux/slices/txresults';

import st from './TransactionsCard.module.scss';
// stale after 2 seconds
const TRANSACTION_STALE_TIME = 2 * 1000;

// Remove duplicate transactionsId, reduces the number of RPC calls
const deduplicate = (results: string[] | undefined) =>
    results
        ? results.filter((value, index, self) => self.indexOf(value) === index)
        : [];

// Get objectId from a transaction effects -> events where recipient is the address
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

const moveCallTxnName = (moveCallFunctionName?: string): string | null =>
    moveCallFunctionName ? moveCallFunctionName.replace(/_/g, ' ') : null;

// if multiple recipients return list of recipients and amounts
function getAmount(
    txnData: SuiTransactionKind,
    address?: string
): { [key: string]: number } | number | null {
    //TODO: add PayAllSuiTransaction
    const transferSui = getTransferSuiTransaction(txnData);
    if (transferSui?.amount) {
        return transferSui.amount;
    }

    const paySuiData =
        getPaySuiTransaction(txnData) ?? getPayTransaction(txnData);

    const amountByRecipient =
        paySuiData?.recipients.reduce((acc, value, index) => {
            return {
                ...acc,
                [value]:
                    paySuiData.amounts[index] + (value in acc ? acc[value] : 0),
            };
        }, {} as { [key: string]: number }) ?? null;

    // return amount if only one recipient or if address is in recipient object
    const amountByRecipientList = Object.values(amountByRecipient || {});

    const amount =
        amountByRecipientList.length === 1
            ? amountByRecipientList[0]
            : amountByRecipient;

    return address && amountByRecipient ? amountByRecipient[address] : amount;
}

type Props = {
    address: string;
};

function RecentTransactions({ address }: Props) {
    const rpc = useRpc();

    // Get recent transaction IDs for the address
    const {
        isLoading: loadingTxIds,
        error: errorTxIds,
        data: txnIds,
    } = useQuery(
        ['txnActivities', address],
        async () => {
            return rpc.getTransactionsForAddress(address, true);
        },
        { staleTime: TRANSACTION_STALE_TIME }
    );

    // Get recent transaction IDs for the address
    const {
        isLoading: loadingTxnWithEffect,
        error: errorTxnWithEffect,
        data: txnWithEffect,
    } = useQuery(
        ['txnActivities', address],
        async () => {
            return rpc.getTransactionWithEffectsBatch(deduplicate(txnIds));
        },
        {
            enabled: txnIds && txnIds.length > 0,
            staleTime: TRANSACTION_STALE_TIME,
        }
    );

    const txByAddress = useMemo(() => {
        if (!txnWithEffect || !txnWithEffect.length || !txnIds) return [];

        const txResults = txnWithEffect.map((txEff) => {
            const txns = getTransactions(txEff.certificate);

            if (txns.length > 1) {
                return null;
            }
            const digest = txnIds.filter(
                (transactionId) =>
                    transactionId === getTransactionDigest(txEff.certificate)
            )[0];

            const txn = txns[0];
            const txKind = getTransactionKindName(txn);
            const transferSui = getTransferSuiTransaction(txn);
            const txTransferObject = getTransferObjectTransaction(txn);

            // revisit this
            const recipient =
                transferSui?.recipient ?? txTransferObject?.recipient;

            const moveCallTxn = getMoveCallTransaction(txn);
            const metaDataObjectId = getTxnEffectsEventID(
                txEff.effects,
                address
            );

            const sender = getTransactionSender(txEff.certificate);
            const amountByRecipient = getAmount(txn);

            const { coins: eventsSummary } = getEventsSummary(
                txEff.effects,
                address
            );

            const amountTransfers = eventsSummary.reduce(
                (acc, { amount }) => acc + amount,
                0
            );

            const amount =
                typeof amountByRecipient === 'number'
                    ? amountByRecipient
                    : Object.values(amountByRecipient || {})[0];

            return {
                txId: digest,
                status: getExecutionStatusType(txEff),
                txGas: getTotalGasUsed(txEff),
                kind: txKind,
                callFunctionName: moveCallTxnName(moveCallTxn?.function),
                from: sender,
                isSender: sender === address,
                error: getExecutionStatusError(txEff),
                timestampMs: txEff.timestamp_ms,
                ...(recipient && { to: recipient }),
                ...((amount || amountTransfers) && {
                    amount: Math.abs(amount || amountTransfers),
                }),
                ...((txTransferObject?.objectRef?.objectId ||
                    metaDataObjectId.length > 0) && {
                    objectId: txTransferObject?.objectRef?.objectId
                        ? [txTransferObject?.objectRef?.objectId]
                        : [...metaDataObjectId],
                }),
            };
        });

        const objectIds = txResults
            .map((itm) => itm?.objectId)
            .filter(notEmpty);
        const objectIDs = [...new Set(objectIds.flat())];
        const getObjectBatch = await rpc.getObjectBatch(objectIDs);
        const txObjects = getObjectBatch.filter(
            ({ status }) => status === 'Exists'
        );

        const txnResp = txResults.map((itm) => {
            const txnObjects =
                txObjects && itm?.objectId && Array.isArray(txObjects)
                    ? txObjects
                          .filter(({ status }) => status === 'Exists')
                          .find((obj) =>
                              itm.objectId?.includes(getObjectId(obj))
                          )
                    : null;

            const { details } = txnObjects || {};

            const coinType =
                txnObjects &&
                is(details, SuiObject) &&
                Coin.getCoinTypeArg(txnObjects);

            const fields =
                txnObjects && is(details, SuiObject)
                    ? getObjectFields(txnObjects)
                    : null;

            return {
                ...itm,
                coinType,
                coinSymbol: coinType && Coin.getCoinSymbol(coinType),
                ...(fields &&
                    fields.url && {
                        description:
                            typeof fields.description === 'string' &&
                            fields.description,
                        name: typeof fields.name === 'string' && fields.name,
                        url: fields.url,
                    }),
                ...(fields && {
                    balance: fields.balance,
                }),
            };
        });
        return txnResp;
    }, [address, rpc, txnIds, txnWithEffect]);

    if (loadingTxIds || loadingTxnWithEffect) {
        return (
            <LoadingIndicator className="w-full flex justify-center items-center" />
        );
    }

    return (
        <>
            <Loading
                loading={loadingTxIds || loadingTxnWithEffect}
                className="w-full flex justify-center items-center"
            >
                {txByAddress.map((txn) => (
                    <ErrorBoundary key={txn.txId}>
                        <TransactionCard txn={txn} />
                    </ErrorBoundary>
                ))}
            </Loading>
        </>
    );
}

export default RecentTransactions;
