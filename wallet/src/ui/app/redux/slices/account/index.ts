// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject } from '@mysten/sui.js';
import {
    createAsyncThunk,
    createSelector,
    createSlice,
} from '@reduxjs/toolkit';
import Browser from 'webextension-polyfill';

import { suiObjectsAdapterSelectors } from '_redux/slices/sui-objects';
import { generateMnemonic } from '_shared/cryptography/mnemonics';

import type { SuiAddress, SuiMoveObject } from '@mysten/sui.js';
import type { PayloadAction } from '@reduxjs/toolkit';
import type { RootState } from '_redux/RootReducer';

export const loadAccountFromStorage = createAsyncThunk(
    'account/loadAccount',
    async (): Promise<string | null> => {
        const { mnemonic } = await Browser.storage.local.get('mnemonic');
        return mnemonic || null;
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
    address: SuiAddress | null;
};

const initialState: AccountState = {
    loading: true,
    mnemonic: null,
    creating: false,
    createdMnemonic: null,
    address: null,
};

const accountSlice = createSlice({
    name: 'account',
    initialState,
    reducers: {
        setMnemonic: (state, action: PayloadAction<string>) => {
            state.mnemonic = action.payload;
        },
        setAddress: (state, action: PayloadAction<string | null>) => {
            state.address = action.payload;
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

export const { setMnemonic, setAddress } = accountSlice.actions;

export default accountSlice.reducer;

export const accountCoinsSelector = createSelector(
    (state: RootState) =>
        suiObjectsAdapterSelectors.selectAll(state.suiObjects),
    (allSuiObjects) => {
        return allSuiObjects
            .filter(
                (anObj) =>
                    isSuiMoveObject(anObj.data) &&
                    anObj.data.type.startsWith('0x2::Coin::Coin')
            )
            .map((aCoin) => aCoin.data as SuiMoveObject);
    }
);

const coinRegex = /^0x2::Coin::Coin<(.+)>$/;
export const accountBalancesSelector = createSelector(
    accountCoinsSelector,
    (coins) => {
        return coins.reduce((acc, aCoin) => {
            const res = aCoin.type.match(coinRegex);
            if (res) {
                const coinType = res[1];
                if (typeof acc[coinType] === 'undefined') {
                    acc[coinType] = 0;
                }
                acc[coinType] += aCoin.fields.balance;
            }
            return acc;
        }, {} as Record<string, number>);
    }
);
