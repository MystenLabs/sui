// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject } from '@mysten/sui.js';
import {
    createAsyncThunk,
    createEntityAdapter,
    createSlice,
} from '@reduxjs/toolkit';

import {
    fetchAllOwnedObjects,
    suiObjectsAdapterSelectors,
} from '_redux/slices/sui-objects';
import { Coin } from '_redux/slices/sui-objects/Coin';

import type {
    SuiAddress,
    SuiMoveObject,
    TransactionEffectsResponse,
} from '@mysten/sui.js';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

type SendTokensTXArgs = {
    tokenTypeArg: string;
    amount: bigint;
    recipientAddress: SuiAddress;
};
type TransactionResult = { EffectResponse: TransactionEffectsResponse };

export const sendTokens = createAsyncThunk<
    TransactionResult,
    SendTokensTXArgs,
    AppThunkConfig
>(
    'sui-objects/send-tokens',
    async (
        { tokenTypeArg, amount, recipientAddress },
        { getState, extra: { api, keypairVault }, dispatch }
    ) => {
        const state = getState();
        const coinType = Coin.getCoinTypeFromArg(tokenTypeArg);
        const coins: SuiMoveObject[] = suiObjectsAdapterSelectors
            .selectAll(state)
            .filter(
                (anObj) =>
                    isSuiMoveObject(anObj.data) && anObj.data.type === coinType
            )
            .map(({ data }) => data as SuiMoveObject);
        const response = await Coin.publicTransferObject(
            api.getSignerInstance(keypairVault.getKeyPair()),
            coins,
            amount,
            recipientAddress
        );

        // TODO: better way to sync latest objects
        dispatch(fetchAllOwnedObjects());
        // TODO: is this correct? Find a better way to do it
        return response as TransactionResult;
    }
);

const txAdapter = createEntityAdapter<TransactionResult>({
    selectId: (tx) => tx.EffectResponse.certificate.transactionDigest,
});

export const txSelectors = txAdapter.getSelectors(
    (state: RootState) => state.transactions
);

const slice = createSlice({
    name: 'transactions',
    initialState: txAdapter.getInitialState(),
    reducers: {},
    extraReducers: (builder) => {
        builder.addCase(sendTokens.fulfilled, (state, { payload }) => {
            return txAdapter.setOne(state, payload);
        });
    },
});

export default slice.reducer;
