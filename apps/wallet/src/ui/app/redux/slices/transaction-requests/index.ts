// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    fromB64,
    Transaction,
    type SignedMessage,
    type SignedTransaction,
    type SuiAddress,
} from '@mysten/sui.js';
import {
    createAsyncThunk,
    createEntityAdapter,
    createSlice,
} from '@reduxjs/toolkit';

import type { SuiTransactionResponse } from '@mysten/sui.js';
import type { PayloadAction } from '@reduxjs/toolkit';
import type { ApprovalRequest } from '_payloads/transactions/ApprovalRequest';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

const txRequestsAdapter = createEntityAdapter<ApprovalRequest>({
    sortComparer: (a, b) => {
        const aDate = new Date(a.createdDate);
        const bDate = new Date(b.createdDate);
        return aDate.getTime() - bDate.getTime();
    },
});

export const respondToTransactionRequest = createAsyncThunk<
    {
        txRequestID: string;
        approved: boolean;
        txResponse: SuiTransactionResponse | null;
    },
    {
        txRequestID: string;
        approved: boolean;
        addressForTransaction: SuiAddress;
    },
    AppThunkConfig
>(
    'respond-to-transaction-request',
    async (
        { txRequestID, approved, addressForTransaction },
        { extra: { background, api }, getState }
    ) => {
        const state = getState();
        const txRequest = txRequestsSelectors.selectById(state, txRequestID);
        if (!txRequest) {
            throw new Error(`TransactionRequest ${txRequestID} not found`);
        }
        let txSigned: SignedTransaction | undefined = undefined;
        let txResult: SuiTransactionResponse | SignedMessage | undefined =
            undefined;
        let txResultError: string | undefined;
        if (approved) {
            const signer = api.getSignerInstance(
                addressForTransaction,
                background
            );
            try {
                if (txRequest.tx.type === 'sign-message') {
                    txResult = await signer.signMessage({
                        message: fromB64(txRequest.tx.message),
                    });
                } else if (txRequest.tx.type === 'transaction') {
                    const tx = Transaction.from(txRequest.tx.data);
                    if (txRequest.tx.justSign) {
                        // Just a signing request, do not submit
                        txSigned = await signer.signTransaction({
                            transaction: tx,
                        });
                    } else {
                        txResult = await signer.signAndExecuteTransaction({
                            transaction: tx,
                            options: txRequest.tx.options?.contentOptions,
                            requestType: txRequest.tx.options?.requestType,
                        });
                    }
                } else {
                    throw new Error(
                        // eslint-disable-next-line @typescript-eslint/no-explicit-any
                        `Unexpected type: ${(txRequest.tx as any).type}`
                    );
                }
            } catch (e) {
                txResultError = (e as Error).message;
            }
        }
        background.sendTransactionRequestResponse(
            txRequestID,
            approved,
            txResult,
            txResultError,
            txSigned
        );
        return { txRequestID, approved: approved, txResponse: null };
    }
);

const slice = createSlice({
    name: 'transaction-requests',
    initialState: txRequestsAdapter.getInitialState({
        initialized: false,
    }),
    reducers: {
        setTransactionRequests: (
            state,
            { payload }: PayloadAction<ApprovalRequest[]>
        ) => {
            // eslint-disable-next-line @typescript-eslint/ban-ts-comment
            // @ts-ignore
            txRequestsAdapter.setAll(state, payload);
            state.initialized = true;
        },
    },
    extraReducers: (build) => {
        build.addCase(
            respondToTransactionRequest.fulfilled,
            (state, { payload }) => {
                const { txRequestID, approved: allowed, txResponse } = payload;
                txRequestsAdapter.updateOne(state, {
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

export const { setTransactionRequests } = slice.actions;

export const txRequestsSelectors = txRequestsAdapter.getSelectors(
    (state: RootState) => state.transactionRequests
);
