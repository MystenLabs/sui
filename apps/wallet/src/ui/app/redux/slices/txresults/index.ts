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

import type {
    GetTxnDigestsResponse,
    CertifiedTransaction,
    TransactionKindName,
    ExecutionStatusType,
    TransactionEffects,
    SuiEvent,
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

const moveCallTxnName = (moveCallFunctionName?: string): string | null =>
    moveCallFunctionName ? moveCallFunctionName.replace(/_/g, ' ') : null;

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
    //
    return objectIDs;
};

export const getTransactionsByAddress = createAsyncThunk<
    TxResultByAddress,
    void,
    AppThunkConfig
>(
    'sui-transactions/get-transactions-by-address',
    async (
        _,
        { getState, dispatch, extra: { api } }
    ): Promise<TxResultByAddress> => {
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
                return txEffs.map((txEff) => {
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
                    const txTransferObject = getTransferObjectTransaction(txn);

                    const recipient =
                        transferSui?.recipient ?? txTransferObject?.recipient;

                    const moveCallTxn = getMoveCallTransaction(txn);

                    const callObjectId = getTxnEffectsEventID(
                        txEff.effects,
                        address
                    )[0];

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
                                      txTransferObject?.objectRef.objectId ??
                                      callObjectId,
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
                });
            });

        // Get all objectId and batch fetch objects for transactions with objectIds
        // remove duplicates

        const objectIDs = [
            ...new Set(
                resp
                    .filter(notEmpty)
                    .map((itm) => itm.objectId)
                    .filter(notEmpty)
            ),
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

            const fields = objectTxObj?.data?.fields;

            return {
                ...itm,
                coinType,
                coinSymbol: coinType && Coin.getCoinSymbol(coinType),
                ...(objectTxObj
                    ? {
                          //Temporary solution to deal unknown object type
                          description:
                              typeof fields.description === 'string' &&
                              fields.description,
                          name: typeof fields.name === 'string' && fields.name,
                          url: objectTxObj.data.fields.url,
                          balance: objectTxObj.data.fields.balance,
                      }
                    : {}),
            };
        });

        return txnResp as TxResultByAddress;
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
