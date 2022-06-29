// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getTransactionDigest,
    getTransactions,
    getTransactionKindName,
    getTransferObjectTransaction,
    getExecutionStatusType,
    getTotalGasUsed,
} from '@mysten/sui.js';
import { createSlice, createAsyncThunk } from '@reduxjs/toolkit';

import type {
    TransactionEffectsResponse,
    GetTxnDigestsResponse,
    CertifiedTransaction,
    TransactionKindName,
    ExecutionStatusType,
} from '@mysten/sui.js';
import type { AppThunkConfig } from '_store/thunk-extras';

export type TxResultState = {
    To?: string;
    seq: number;
    txId: string;
    status: ExecutionStatusType;
    txGas: number;
    kind: TransactionKindName | undefined;
    From: string;
};

interface TransactionManualState {
    loading: boolean;
    error: false | { code?: string; message?: string; name?: string };
    latestTx: TxResultState[];
}

const initialState: TransactionManualState = {
    loading: true,
    latestTx: [],
    error: false,
};
type TxResultByAddress = TxResultState[];

// Remove duplicate transactionsId, reduces the number of RPC calls
const deduplicate = (results: [number, string][] | undefined) =>
    results
        ? results
              .map((result) => result[1])
              .filter((value, index, self) => self.indexOf(value) === index)
        : [];

export const getTransactionsByAddress = createAsyncThunk<
    TxResultByAddress,
    void,
    AppThunkConfig
>(
    'sui-transactions/get-transactions-by-address',
    async (_, { getState, extra: { api } }): Promise<TxResultByAddress> => {
        const address = getState().account.address;

        if (!address) {
            return [];
        }
        // Get all transactions txId for address
        const transactions: GetTxnDigestsResponse = (
            await api.instance.fullNode.getTransactionsForAddress(address)
        ).filter((tx) => tx);

        if (!transactions || !transactions.length) {
            return [];
        }
        //getTransactionWithEffectsBatch
        const resp = await api.instance.fullNode
            .getTransactionWithEffectsBatch(deduplicate(transactions))
            .then((txEffs: TransactionEffectsResponse[]) => {
                return (
                    txEffs
                        .map((txEff, i) => {
                            const [seq, digest] = transactions.filter(
                                (transactionId) =>
                                    transactionId[1] ===
                                    getTransactionDigest(txEff.certificate)
                            )[0];
                            const res: CertifiedTransaction = txEff.certificate;
                            // TODO: handle multiple transactions
                            const txns = getTransactions(res);
                            if (txns.length > 1) {
                                return null;
                            }
                            const txn = txns[0];
                            const txKind = getTransactionKindName(txn);
                            const recipient =
                                getTransferObjectTransaction(txn)?.recipient;

                            return {
                                seq,
                                txId: digest,
                                status: getExecutionStatusType(txEff),
                                txGas: getTotalGasUsed(txEff),
                                kind: txKind,
                                From: res.data.sender,
                                ...(recipient
                                    ? {
                                          To: recipient,
                                      }
                                    : {}),
                            };
                        })
                        // Remove failed transactions and sort by sequence number
                        .filter((itm) => itm)
                        // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
                        .sort((a, b) => b!.seq - a!.seq)
                );
            });
        return resp as TxResultByAddress;
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
            })
            .addCase(getTransactionsByAddress.pending, (state, action) => {
                state.loading = true;
                state.latestTx = [];
            })
            .addCase(
                getTransactionsByAddress.rejected,
                (state, { error: { code, name, message } }) => {
                    state.loading = false;
                    state.error = { code, message, name };
                    state.latestTx = [];
                }
            );
    },
});

export default txSlice.reducer;
