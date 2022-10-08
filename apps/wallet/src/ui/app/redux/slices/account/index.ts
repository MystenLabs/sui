// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    createAsyncThunk,
    createSelector,
    createSlice,
} from '@reduxjs/toolkit';
import Browser from 'webextension-polyfill';

import { isErrorPayload } from '_payloads';
import { isKeyringPayload } from '_payloads/keyring';
import { suiObjectsAdapterSelectors } from '_redux/slices/sui-objects';
import { Coin } from '_redux/slices/sui-objects/Coin';

import type { SuiAddress, SuiMoveObject } from '@mysten/sui.js';
import type { PayloadAction, Reducer } from '@reduxjs/toolkit';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

export const loadAccountFromStorage = createAsyncThunk(
    'account/loadAccount',
    async (): Promise<string | null> => {
        const { mnemonic } = await Browser.storage.local.get('mnemonic');
        return mnemonic || null;
    }
);

export const createMnemonic = createAsyncThunk<
    string,
    {
        existingMnemonic?: string;
        password?: string;
    },
    AppThunkConfig
>(
    'account/createMnemonic',
    async ({ existingMnemonic, password }, { extra: { background } }) => {
        let mnemonic = existingMnemonic;
        if (!mnemonic) {
            const { payload } = await background.createMnemonic(password || '');
            if (isKeyringPayload<'createMnemonic'>(payload, 'createMnemonic')) {
                if (!payload.return) {
                    throw new Error('Empty mnemonic in payload');
                }
                mnemonic = payload.return;
            } else if (isErrorPayload(payload)) {
                throw new Error(payload.message);
            } else {
                throw new Error('Unknown payload');
            }
        }
        // TODO: store it unencrypted until everything switches to using the encrypted one (#encrypt-wallet)
        await Browser.storage.local.set({ mnemonic });
        return mnemonic;
    }
);

export const loadMnemonicFromKeyring = createAsyncThunk<
    string,
    { password?: string }, // can be undefined when we know Keyring is unlocked
    AppThunkConfig
>(
    'account/loadMnemonicFromKeyring',
    async ({ password }, { extra: { background } }) =>
        await background.getMnemonic(password)
);

export const logout = createAsyncThunk(
    'account/logout',
    async (): Promise<void> => {
        await Browser.storage.local.clear();
        window.location.reload();
    }
);

type AccountState = {
    loading: boolean;
    mnemonic: string | null;
    creating: boolean;
    address: SuiAddress | null;
};

const initialState: AccountState = {
    loading: true,
    mnemonic: null,
    creating: false,
    address: null,
};

const accountSlice = createSlice({
    name: 'account',
    initialState,
    reducers: {
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
                state.mnemonic = action.payload;
            })
            .addCase(createMnemonic.rejected, (state) => {
                state.creating = false;
                state.mnemonic = null;
            }),
});

export const { setAddress } = accountSlice.actions;

const reducer: Reducer<typeof initialState> = accountSlice.reducer;
export default reducer;

export const activeAccountSelector = ({ account }: RootState) =>
    account.address;

export const ownedObjects = createSelector(
    suiObjectsAdapterSelectors.selectAll,
    activeAccountSelector,
    (objects, address) => {
        if (address) {
            return objects.filter(
                ({ owner }) =>
                    typeof owner === 'object' &&
                    'AddressOwner' in owner &&
                    owner.AddressOwner === address
            );
        }
        return [];
    }
);

export const accountCoinsSelector = createSelector(
    ownedObjects,
    (allSuiObjects) => {
        return allSuiObjects
            .filter(Coin.isCoin)
            .map((aCoin) => aCoin.data as SuiMoveObject);
    }
);

// return an aggregate balance for each coin type
export const accountAggregateBalancesSelector = createSelector(
    accountCoinsSelector,
    (coins) => {
        return coins.reduce((acc, aCoin) => {
            const coinType = Coin.getCoinTypeArg(aCoin);
            if (coinType) {
                if (typeof acc[coinType] === 'undefined') {
                    acc[coinType] = BigInt(0);
                }
                acc[coinType] += Coin.getBalance(aCoin);
            }
            return acc;
        }, {} as Record<string, bigint>);
    }
);

// return a list of balances for each coin object for each coin type
export const accountItemizedBalancesSelector = createSelector(
    accountCoinsSelector,
    (coins) => {
        return coins.reduce((acc, aCoin) => {
            const coinType = Coin.getCoinTypeArg(aCoin);
            if (coinType) {
                if (typeof acc[coinType] === 'undefined') {
                    acc[coinType] = [];
                }
                acc[coinType].push(Coin.getBalance(aCoin));
            }
            return acc;
        }, {} as Record<string, bigint[]>);
    }
);

export const accountNftsSelector = createSelector(
    ownedObjects,
    (allSuiObjects) => {
        return allSuiObjects.filter((anObj) => !Coin.isCoin(anObj));
    }
);
