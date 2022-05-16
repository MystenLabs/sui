// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createAsyncThunk, createSlice } from '@reduxjs/toolkit';
import Browser from 'webextension-polyfill';

import { generateMnemonic } from '_shared/cryptography/mnemonics';

import type { PayloadAction } from '@reduxjs/toolkit';

export const loadAccountFromStorage = createAsyncThunk(
    'account/loadAccount',
    async (): Promise<string> => {
        const { mnemonic } = await Browser.storage.local.get('mnemonic');
        return mnemonic;
    }
);

export const createMnemonic = createAsyncThunk(
    'account/createMnemonic',
    async (existingMnemonic?: string): Promise<string> => {
        const mnemonic = existingMnemonic || generateMnemonic();
        await Browser.storage.local.set({ mnemonic });
        return mnemonic;
    }
);

type AccountState = {
    loading: boolean;
    mnemonic: string | null;
    creating: boolean;
    createdMnemonic: string | null;
};

const initialState: AccountState = {
    loading: true,
    mnemonic: null,
    creating: false,
    createdMnemonic: null,
};

const accountSlice = createSlice({
    name: 'account',
    initialState,
    reducers: {
        setMnemonic: (state, action: PayloadAction<string>) => {
            state.mnemonic = action.payload;
        },
    },
    extraReducers: (builder) =>
        builder
            .addCase(loadAccountFromStorage.fulfilled, (state, action) => {
                state.loading = false;
                state.mnemonic = action.payload;
            })
            .addCase(createMnemonic.pending, (state) => {
                state.creating = true;
            })
            .addCase(createMnemonic.fulfilled, (state, action) => {
                state.creating = false;
                state.createdMnemonic = action.payload;
            })
            .addCase(createMnemonic.rejected, (state) => {
                state.creating = false;
                state.createdMnemonic = null;
            }),
});

export const { setMnemonic } = accountSlice.actions;

export default accountSlice.reducer;
