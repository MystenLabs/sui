// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getTransactionDigest,
    Coin as CoinAPI,
} from '@mysten/sui.js';
import {
    createAsyncThunk,
    createEntityAdapter,
    createSlice,
} from '@reduxjs/toolkit';
import * as Sentry from '@sentry/react';

import {
    accountCoinsSelector,
    activeAccountSelector,
} from '_redux/slices/account';
import { fetchAllOwnedAndRequiredObjects } from '_redux/slices/sui-objects';

import type {
    SuiAddress,
    SuiExecuteTransactionResponse,
    SuiMoveObject,
} from '@mysten/sui.js';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

type SendTokensTXArgs = {
    tokenTypeArg: string;
    amount: bigint;
    recipientAddress: SuiAddress;
    gasBudget: number;
    sendMax: boolean;
};
type TransactionResult = SuiExecuteTransactionResponse;

export const sendTokens = createAsyncThunk<
    TransactionResult,
    SendTokensTXArgs,
    AppThunkConfig
>(
    'sui-objects/send-tokens',
    async (
        { tokenTypeArg, amount, recipientAddress, gasBudget, sendMax },
        { getState, extra: { api, background }, dispatch }
    ) => {
        const transaction = Sentry.startTransaction({ name: 'send-tokens' });
        const state = getState();
        const activeAddress = activeAccountSelector(state);
        if (!activeAddress) {
            throw new Error('Error, active address is not defined');
        }
        const coins: SuiMoveObject[] = accountCoinsSelector(state);
        const signer = api.getSignerInstance(activeAddress, background);
        try {
            transaction.setTag('coin', tokenTypeArg);
            const response = await signer.signAndExecuteTransaction(
                await CoinAPI.newPayTransaction(
                    coins,
                    tokenTypeArg,
                    amount,
                    recipientAddress,
                    gasBudget
                )
            );
            // TODO: better way to sync latest objects
            dispatch(fetchAllOwnedAndRequiredObjects());
            return response;
        } catch (e) {
            transaction.setTag('failure', true);
            throw e;
        } finally {
            transaction.finish();
        }
    }
);

const txAdapter = createEntityAdapter<TransactionResult>({
    selectId: (tx) => getTransactionDigest(tx),
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
            // eslint-disable-next-line @typescript-eslint/ban-ts-comment
            // @ts-ignore: This causes a compiler error, but it will be removed when we migrate off of Redux.
            return txAdapter.setOne(state, payload);
        });
    },
});

export default slice.reducer;
