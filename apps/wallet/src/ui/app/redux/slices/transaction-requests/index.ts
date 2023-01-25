// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    Base64DataBuffer,
    getCertifiedTransaction,
    getTransactionEffects,
    LocalTxnDataSerializer,
} from '@mysten/sui.js';
import {
    createAsyncThunk,
    createEntityAdapter,
    createSlice,
} from '@reduxjs/toolkit';

import type {
    SuiMoveNormalizedFunction,
    SuiTransactionResponse,
    SignableTransaction,
    SuiExecuteTransactionResponse,
    MoveCallTransaction,
    UnserializedSignableTransaction,
} from '@mysten/sui.js';
import type { PayloadAction } from '@reduxjs/toolkit';
import type { TransactionRequest } from '_payloads/transactions';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

const txRequestsAdapter = createEntityAdapter<TransactionRequest>({
    sortComparer: (a, b) => {
        const aDate = new Date(a.createdDate);
        const bDate = new Date(b.createdDate);
        return aDate.getTime() - bDate.getTime();
    },
});

export const loadTransactionResponseMetadata = createAsyncThunk<
    { txRequestID: string; metadata: SuiMoveNormalizedFunction },
    {
        txRequestID: string;
        objectId: string;
        moduleName: string;
        functionName: string;
    },
    AppThunkConfig
>(
    'load-transaction-response-metadata',
    async (
        { txRequestID, objectId, moduleName, functionName },
        { extra: { api }, getState }
    ) => {
        const state = getState();
        const txRequest = txRequestsSelectors.selectById(state, txRequestID);
        if (!txRequest) {
            throw new Error(`TransactionRequest ${txRequestID} not found`);
        }

        const metadata = await api.instance.fullNode.getNormalizedMoveFunction(
            objectId,
            moduleName,
            functionName
        );

        return { txRequestID, metadata };
    }
);

export const deserializeTxn = createAsyncThunk<
    {
        txRequestID: string;
        unSerializedTxn: UnserializedSignableTransaction | null;
    },
    { serializedTxn: string; id: string },
    AppThunkConfig
>(
    'deserialize-transaction',
    async (data, { dispatch, extra: { api, keypairVault, background } }) => {
        const { id, serializedTxn } = data;
        const signer = api.getSignerInstance(
            keypairVault.getKeypair().getPublicKey().toSuiAddress(),
            background
        );
        const localSerializer = new LocalTxnDataSerializer(signer.provider);
        const txnBytes = new Base64DataBuffer(serializedTxn);
        const version = await api.instance.fullNode.getRpcApiVersion();

        //TODO: Error handling - either show the error or use the serialized txn
        const useIntentSigning =
            version != null && version.major >= 0 && version.minor > 18;
        const deserializeTx =
            (await localSerializer.deserializeTransactionBytesToSignableTransaction(
                useIntentSigning,
                txnBytes
            )) as UnserializedSignableTransaction;

        const deserializeData = deserializeTx?.data as MoveCallTransaction;

        const normalized = {
            ...deserializeData,
            gasBudget: Number(deserializeData.gasBudget.toString(10)),
            gasPayment: '0x' + deserializeData.gasPayment,
            arguments: deserializeData.arguments.map((d) => '0x' + d),
        };

        if (deserializeTx && normalized) {
            dispatch(
                loadTransactionResponseMetadata({
                    txRequestID: id,
                    objectId: normalized.packageObjectId,
                    moduleName: normalized.module,
                    functionName: normalized.function,
                })
            );
        }

        return {
            txRequestID: id,
            unSerializedTxn:
                ({
                    ...deserializeTx,
                    data: normalized,
                } as UnserializedSignableTransaction) || null,
        };
    }
);

export const respondToTransactionRequest = createAsyncThunk<
    {
        txRequestID: string;
        approved: boolean;
        txResponse: SuiTransactionResponse | null;
    },
    { txRequestID: string; approved: boolean },
    AppThunkConfig
>(
    'respond-to-transaction-request',
    async (
        { txRequestID, approved },
        { extra: { background, api, keypairVault }, getState }
    ) => {
        const state = getState();
        const txRequest = txRequestsSelectors.selectById(state, txRequestID);
        if (!txRequest) {
            throw new Error(`TransactionRequest ${txRequestID} not found`);
        }
        let txResult: SuiTransactionResponse | undefined = undefined;
        let tsResultError: string | undefined;
        if (approved) {
            const signer = api.getSignerInstance(
                keypairVault.getKeypair().getPublicKey().toSuiAddress(),
                background
            );
            try {
                let response: SuiExecuteTransactionResponse;
                if (
                    txRequest.tx.type === 'v2' ||
                    txRequest.tx.type === 'move-call'
                ) {
                    const txn: SignableTransaction =
                        txRequest.tx.type === 'move-call'
                            ? {
                                  kind: 'moveCall',
                                  data: txRequest.tx.data,
                              }
                            : txRequest.tx.data;

                    response = await signer.signAndExecuteTransaction(txn);
                } else if (txRequest.tx.type === 'serialized-move-call') {
                    const txBytes = new Base64DataBuffer(txRequest.tx.data);
                    response = await signer.signAndExecuteTransaction(txBytes);
                } else {
                    throw new Error(
                        `Either tx or txBytes needs to be defined.`
                    );
                }

                txResult = {
                    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
                    certificate: getCertifiedTransaction(response)!,
                    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
                    effects: getTransactionEffects(response)!,
                    timestamp_ms: null,
                    parsed_data: null,
                };
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
    name: 'transaction-requests',
    initialState: txRequestsAdapter.getInitialState({
        initialized: false,
        // show serialized txn if deserialization fails
        deserializeTxnFailed: false,
    }),
    reducers: {
        setTransactionRequests: (
            state,
            { payload }: PayloadAction<TransactionRequest[]>
        ) => {
            // eslint-disable-next-line @typescript-eslint/ban-ts-comment
            // @ts-ignore
            txRequestsAdapter.setAll(state, payload);
            state.deserializeTxnFailed = false;
            state.initialized = true;
        },
    },
    extraReducers: (build) => {
        build.addCase(
            loadTransactionResponseMetadata.fulfilled,
            (state, { payload }) => {
                const { txRequestID, metadata } = payload;
                txRequestsAdapter.updateOne(state, {
                    id: txRequestID,
                    changes: {
                        metadata,
                    },
                });
            }
        );

        build.addCase(deserializeTxn.rejected, (state, { payload }) => {
            state.deserializeTxnFailed = true;
        });
        build.addCase(deserializeTxn.fulfilled, (state, { payload }) => {
            const { txRequestID, unSerializedTxn } = payload;
            if (unSerializedTxn) {
                txRequestsAdapter.updateOne(state, {
                    id: txRequestID,
                    changes: {
                        unSerializedTxn: unSerializedTxn || null,
                    },
                });
            }
        });

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
