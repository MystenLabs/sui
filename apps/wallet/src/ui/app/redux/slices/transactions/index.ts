// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject } from '@mysten/sui.js';
import {
    createAsyncThunk,
    createEntityAdapter,
    createSlice,
} from '@reduxjs/toolkit';

import {
    fetchAllOwnedAndRequiredObjects,
    suiObjectsAdapterSelectors,
} from '_redux/slices/sui-objects';
import { Coin } from '_redux/slices/sui-objects/Coin';

import type {
    SuiAddress,
    SuiMoveObject,
    SuiTransactionResponse,
} from '@mysten/sui.js';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

type SendTokensTXArgs = {
    tokenTypeArg: string;
    amount: bigint;
    recipientAddress: SuiAddress;
};
type TransactionResult = SuiTransactionResponse;

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
        const response =
            Coin.getCoinSymbol(tokenTypeArg) === 'SUI'
                ? await Coin.transferSui(
                      api.getSignerInstance(keypairVault.getKeyPair()),
                      coins,
                      amount,
                      recipientAddress
                  )
                : await Coin.transferCoin(
                      api.getSignerInstance(keypairVault.getKeyPair()),
                      coins,
                      amount,
                      recipientAddress
                  );

        // TODO: better way to sync latest objects
        dispatch(fetchAllOwnedAndRequiredObjects());
        // TODO: is this correct? Find a better way to do it
        return response as TransactionResult;
    }
);

type StakeTokensTXArgs = {
    tokenTypeArg: string;
    amount: bigint;
};

export const StakeTokens = createAsyncThunk<
    TransactionResult,
    StakeTokensTXArgs,
    AppThunkConfig
>(
    'sui-objects/stake',
    async (
        { tokenTypeArg, amount },
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

        // TODO: fetch the first active validator for now,
        // repalce it with the user picked one
        const activeValidators = await Coin.getActiveValidators(
            api.instance.fullNode
        );
        const first_validator = activeValidators[0];
        const metadata = (first_validator as SuiMoveObject).fields.metadata;
        const validatorAddress = (metadata as SuiMoveObject).fields.sui_address;
        const response = await Coin.stakeCoin(
            api.getSignerInstance(keypairVault.getKeyPair()),
            coins,
            amount,
            validatorAddress
        );
        dispatch(fetchAllOwnedAndRequiredObjects());
        return response as TransactionResult;
    }
);

const txAdapter = createEntityAdapter<TransactionResult>({
    selectId: (tx) => tx.certificate.transactionDigest,
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
        builder.addCase(StakeTokens.fulfilled, (state, { payload }) => {
            return txAdapter.setOne(state, payload);
        });
    },
});

export default slice.reducer;
