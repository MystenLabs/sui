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
} from '@mysten/sui.js';
import { createSlice, createAsyncThunk } from '@reduxjs/toolkit';

import { notEmpty } from '_helpers';
import { batchFetchObject } from '_redux/slices/sui-objects';
import { Coin } from '_redux/slices/sui-objects/Coin';
import { reportSentryError } from '_src/shared/sentry';

import type {
    GetTxnDigestsResponse,
    CertifiedTransaction,
    TransactionKindName,
    ExecutionStatusType,
    TransactionEffects,
} from '@mysten/sui.js';
import type { AppThunkConfig } from '_store/thunk-extras';

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
    objectId: string;
    description?: string;
    name?: string;
    isSender?: boolean;
    error?: string;
    balance?: number;
    callFunctionName?: string;
    coinSymbol?: string;
    coinType?: string;
};

interface TransactionManualState {
    loading: boolean;
    error: false | { code?: string; message?: string; name?: string };
    latestTx: TxResultState[];
    recentAddresses: string[];
}

const initialState: TransactionManualState = {
    loading: true,
    latestTx: [],
    recentAddresses: [],
    error: false,
};
type TxResultByAddress = TxResultState[];

// Remove duplicate transactionsId, reduces the number of RPC calls
const deduplicate = (results: string[] | undefined) =>
    results
        ? results.filter((value, index, self) => self.indexOf(value) === index)
        : [];

// TODO: This is a temporary solution to get the NFT data from Call txn
const getCreatedObjectID = (txEffects: TransactionEffects): string | null => {
    return txEffects?.created
        ? txEffects?.created.map((item) => item.reference)[0]?.objectId
        : null;
};

const moveCallTxnName = (moveCallFunctionName?: string): string | null =>
    moveCallFunctionName ? moveCallFunctionName.replace(/_/g, ' ') : null;

export const getTransactionsByAddress = createAsyncThunk<
    TxResultByAddress,
    void,
    AppThunkConfig
>(
    'sui-transactions/get-transactions-by-address',
    async (
        _,
        { getState, dispatch, extra: { api }, rejectWithValue }
    ): Promise<TxResultByAddress> => {
        try {
            const address = getState().account.address;

            if (!address) {
                return [];
            }
            // Get all transactions txId for address
            const transactions: GetTxnDigestsResponse =
                await api.instance.fullNode.getTransactionsForAddress(
                    address,
                    'Descending'
                );

            if (!transactions || !transactions.length) {
                return [];
            }

            const resp = await api.instance.fullNode
                .getTransactionWithEffectsBatch(deduplicate(transactions))
                .then(async (txEffs) => {
                    return txEffs
                        .map((txEff) => {
                            const digest = transactions.filter(
                                (transactionId) =>
                                    transactionId ===
                                    getTransactionDigest(txEff.certificate)
                            )[0];
                            const res: CertifiedTransaction = txEff.certificate;

                            const txns = getTransactions(res);
                            if (txns.length > 1) {
                                return null;
                            }
                            // TODO handle batch transactions
                            const txn = txns[0];
                            const txKind = getTransactionKindName(txn);

                            const transferSui = getTransferSuiTransaction(txn);
                            const txTransferObject =
                                getTransferObjectTransaction(txn);

                            const recipient =
                                transferSui?.recipient ??
                                txTransferObject?.recipient;

                            const moveCallTxn = getMoveCallTransaction(txn);

                            const callObjectId = getCreatedObjectID(
                                txEff.effects
                            );

                            return {
                                txId: digest,
                                status: getExecutionStatusType(txEff),
                                txGas: getTotalGasUsed(txEff),
                                kind: txKind,
                                callFunctionName: moveCallTxnName(
                                    moveCallTxn?.function
                                ),
                                from: res.data.sender,
                                ...(txTransferObject || callObjectId
                                    ? {
                                          objectId:
                                              txTransferObject?.objectRef
                                                  .objectId ?? callObjectId,
                                      }
                                    : {}),
                                error: getExecutionStatusError(txEff),
                                timestampMs: txEff.timestamp_ms,
                                isSender: res.data.sender === address,
                                ...(transferSui?.amount
                                    ? { amount: transferSui.amount }
                                    : {}),
                                ...(recipient
                                    ? {
                                          to: recipient,
                                      }
                                    : {}),
                            };
                        })
                        .filter(notEmpty);
                });

            // Get all objectId and batch fetch objects for transactions with objectIds
            // remove duplicates
            const objectIDs = [
                ...new Set(resp.map((itm) => itm.objectId).filter(notEmpty)),
            ];

            const getObjectBatch = await dispatch(batchFetchObject(objectIDs));
            const txObjects = getObjectBatch.payload;

            const txnResp = resp.map((itm) => {
                const objectTxObj =
                    txObjects && itm?.objectId && Array.isArray(txObjects)
                        ? txObjects.find(
                              (obj) => obj.reference.objectId === itm.objectId
                          )
                        : null;

                const coinType =
                    objectTxObj?.data?.type &&
                    Coin.getCoinTypeArg(objectTxObj.data);

                return {
                    ...itm,
                    ...(objectTxObj
                        ? {
                              description: objectTxObj.data.fields.description,
                              name: objectTxObj.data.fields.name,
                              url: objectTxObj.data.fields.url,
                              balance: objectTxObj.data.fields.balance,
                              coinType,
                              coinSymbol:
                                  coinType && Coin.getCoinSymbol(coinType),
                          }
                        : {}),
                };
            });

            return txnResp as TxResultByAddress;
        } catch (err) {
            const error = err as Error;
            reportSentryError(error);
            throw rejectWithValue(error.message);
        }
    }
);

const txSlice = createSlice({
    name: 'txresult',
    initialState,
    reducers: {},
    extraReducers: (builder) => {
        builder
            .addCase(getTransactionsByAddress.fulfilled, (state, action) => {
                state.loading = false;
                state.error = false;
                state.latestTx = action.payload;
                // Add recent addresses to the list
                const recentAddresses = action.payload.map((tx) => [
                    tx?.to as string,
                    tx.from as string,
                ]);
                // Remove duplicates
                state.recentAddresses = [
                    ...new Set(recentAddresses.flat().filter((itm) => itm)),
                ];
            })
            .addCase(getTransactionsByAddress.pending, (state, action) => {
                state.loading = true;
                state.latestTx = [];
                state.recentAddresses = [];
            })
            .addCase(
                getTransactionsByAddress.rejected,
                (state, { error: { code, name, message } }) => {
                    state.loading = false;
                    state.error = { code, message, name };
                    state.latestTx = [];
                    state.recentAddresses = [];
                }
            );
    },
});

export default txSlice.reducer;
