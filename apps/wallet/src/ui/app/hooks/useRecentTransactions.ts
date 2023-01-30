// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getTransactionDigest,
    getTransactions,
    getTransactionKindName,
    getTransferObjectTransaction,
    getExecutionStatusType,
    getTotalGasUsed,
    getExecutionStatusError,
    getMoveCallTransaction,
    getTransactionSender,
    getObjectId,
    getObjectFields,
    Coin,
    is,
    SuiObject,
    type TransactionEffects,
    type SuiEvent,
    type ExecutionStatusType,
    type TransactionKindName,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { getAmount, getEventsSummary, notEmpty } from '../helpers';
import useAppSelector from './useAppSelector';
import { useRpc } from './useRpc';

export type TxResultState = {
    to?: string;
    txId: string;
    status: ExecutionStatusType;
    txGas: number;
    kind: TransactionKindName | undefined;
    from: string;
    amount?: number;
    timestampMs?: number;
    url?: string;
    balance?: number;
    objectId?: string;
    description?: string;
    name?: string;
    isSender?: boolean;
    error?: string;
    callFunctionName?: string;
    coinSymbol?: string;
    coinType?: string;
};

type TxResultByAddress = TxResultState[];

const deduplicate = (arr: string[]) => [...new Set(arr)];

const moveCallTxnName = (moveCallFunctionName?: string): string | null =>
    moveCallFunctionName ? moveCallFunctionName.replace(/_/g, ' ') : null;

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

// TODO: This is not an ideal hook, and was ported from redux quickly in order to fix
// performance issues in the wallet.
export function useRecentTransactions() {
    const rpc = useRpc();
    const address = useAppSelector((state) => state.account.address);

    return useQuery(
        ['transactions', 'recent'],
        async () => {
            if (!address) return [];

            // Get all transactions txId for address
            const transactions = await rpc.getTransactionsForAddress(
                address,
                true
            );

            if (!transactions || !transactions.length) {
                return [];
            }

            const txEffs = await rpc.getTransactionWithEffectsBatch(
                deduplicate(transactions)
            );

            const txResults = txEffs.map((txEff) => {
                const digest = transactions.filter(
                    (transactionId) =>
                        transactionId ===
                        getTransactionDigest(txEff.certificate)
                )[0];

                const txns = getTransactions(txEff.certificate);

                // TODO handle batch transactions
                if (txns.length > 1) {
                    return null;
                }

                const txn = txns[0];
                const txKind = getTransactionKindName(txn);
                const txTransferObject = getTransferObjectTransaction(txn);
                const amountByRecipient = getAmount(txn, txEff.effects);
                const sender = getTransactionSender(txEff.certificate);
                const amount =
                    amountByRecipient && amountByRecipient[0]?.amount;
                //TODO: Support multiple recipients
                const recipientObj =
                    amountByRecipient &&
                    amountByRecipient?.filter(
                        ({ recipientAddress }) => recipientAddress !== sender
                    );

                const recipient =
                    recipientObj && recipientObj[0]?.recipientAddress;

                const moveCallTxn = getMoveCallTransaction(txn);
                const metaDataObjectId = getTxnEffectsEventID(
                    txEff.effects,
                    address
                );

                const { coins: eventsSummary } = getEventsSummary(
                    txEff.effects,
                    address
                );
                const amountTransfers = eventsSummary.reduce(
                    (acc, { amount }) => acc + amount,
                    0
                );

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
                            name:
                                typeof fields.name === 'string' && fields.name,
                            url: fields.url,
                        }),
                    ...(fields && {
                        balance: fields.balance,
                    }),
                };
            });

            return txnResp as TxResultByAddress;
        },
        {
            enabled: !!address,
            // Consider data stale after 10 seconds to keep activty view fresher:
            staleTime: 10 * 1000,
        }
    );
}
