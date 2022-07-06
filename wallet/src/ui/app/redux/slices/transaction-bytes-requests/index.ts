// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    createAsyncThunk,
    createEntityAdapter,
    createSlice,
} from '@reduxjs/toolkit';

import type { TransactionResponse } from '@mysten/sui.js';
import type { PayloadAction } from '@reduxjs/toolkit';
import type { TransactionBytesRequest } from '_payloads/transactions';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

const txBytesRequestsAdapter = createEntityAdapter<TransactionBytesRequest>({
    sortComparer: (a, b) => {
        const aDate = new Date(a.createdDate);
        const bDate = new Date(b.createdDate);
        return aDate.getTime() - bDate.getTime();
    },
});

export const respondToTransactionBytesRequest = createAsyncThunk<
    {
        txRequestID: string;
        approved: boolean;
        txResponse: TransactionResponse | null;
    },
    { txRequestID: string; approved: boolean },
    AppThunkConfig
>(
    'respond-to-transaction-bytes-request',
    async (
        { txRequestID, approved },
        { extra: { background, api, keypairVault }, getState }
    ) => {
        const state = getState();
        const txBytesRequest = txBytesRequestsSelectors.selectById(state, txRequestID);
        if (!txBytesRequest) {
            throw new Error(`TransactionBytesRequest ${txRequestID} not found`);
        }
        let txResult: TransactionResponse | undefined = undefined;
        let tsResultError: string | undefined;
        if (approved) {
            const signer = api.getSignerInstance(keypairVault.getKeyPair());
            try {
                txResult = await signer.signAndExecuteTransaction(txBytesRequest.txBytes);
            } catch (e) {
                tsResultError = (e as Error).message;
            }
        }
        background.sendTransactionRequestResponse(
            txRequestID,
            approved,
            txResult,
            tsResultError
        );
        return { txRequestID, approved: approved, txResponse: null };
    }
);

const slice = createSlice({
    name: 'transaction-bytes-requests',
    initialState: txBytesRequestsAdapter.getInitialState({ initialized: false }),
    reducers: {
        setTransactionBytesRequests: (
            state,
            { payload }: PayloadAction<TransactionBytesRequest[]>
        ) => {
            txBytesRequestsAdapter.setAll(state, payload);
            state.initialized = true;
        },
    },
    extraReducers: (build) => {
        build.addCase(
            respondToTransactionBytesRequest.fulfilled,
            (state, { payload }) => {
                const { txRequestID, approved: allowed, txResponse } = payload;
                txBytesRequestsAdapter.updateOne(state, {
                    id: txRequestID,
                    changes: {
                        approved: allowed,
                        txResult: txResponse || undefined,
                    },
                });
            }
        );
    },
});

export default slice.reducer;

export const { setTransactionBytesRequests } = slice.actions;

export const txBytesRequestsSelectors = txBytesRequestsAdapter.getSelectors(
    (state: RootState) => state.transactionBytesRequests
);
