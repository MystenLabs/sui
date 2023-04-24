// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';
import {
    createAsyncThunk,
    createEntityAdapter,
    createSlice,
} from '@reduxjs/toolkit';
import Browser from 'webextension-polyfill';

import {
    AccountType,
    type SerializedAccount,
} from '_src/background/keyring/Account';
import { persister, queryClient } from '_src/ui/app/helpers/queryClient';

import type { PayloadAction, Reducer } from '@reduxjs/toolkit';
import type { KeyringPayload } from '_payloads/keyring';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

export const createVault = createAsyncThunk<
    void,
    {
        importedEntropy?: string;
        password: string;
    },
    AppThunkConfig
>(
    'account/createVault',
    async ({ importedEntropy, password }, { extra: { background } }) => {
        await background.createVault(password, importedEntropy);
        await background.unlockWallet(password);
    }
);

export const loadEntropyFromKeyring = createAsyncThunk<
    string,
    { password?: string }, // can be undefined when we know Keyring is unlocked
    AppThunkConfig
>(
    'account/loadEntropyFromKeyring',
    async ({ password }, { extra: { background } }) =>
        await background.getEntropy(password)
);

export const logout = createAsyncThunk<void, void, AppThunkConfig>(
    'account/logout',
    async (_, { extra: { background } }): Promise<void> => {
        await Browser.storage.local.clear();
        await Browser.storage.local.set({
            v: -1,
        });
        await background.clearWallet();

        queryClient.resetQueries();
        queryClient.clear();
        queryClient.unmount();
        await persister.removeClient();
    }
);

const sortOrderByAccountType = [
    AccountType.DERIVED,
    AccountType.IMPORTED,
    AccountType.LEDGER,
];

const accountsAdapter = createEntityAdapter<SerializedAccount>({
    selectId: ({ address }) => address,
    sortComparer: (a, b) => {
        if (a.type !== b.type) {
            const sortRankForA = sortOrderByAccountType.indexOf(a.type);
            const sortRankForB = sortOrderByAccountType.indexOf(b.type);
            return sortRankForA - sortRankForB;
        } else if (a.derivationPath) {
            // Sort accounts by their derivation path if one exists
            return (a.derivationPath || '').localeCompare(
                b.derivationPath || '',
                undefined,
                { numeric: true }
            );
        } else {
            // Otherwise, let's sort accounts by their address
            return a.address.localeCompare(b.address, undefined, {
                numeric: true,
            });
        }
    },
});

type AccountState = {
    creating: boolean;
    address: SuiAddress | null;
    isLocked: boolean | null;
    isInitialized: boolean | null;
};

const initialState = accountsAdapter.getInitialState<AccountState>({
    creating: false,
    address: null,
    isLocked: null,
    isInitialized: null,
});

const accountSlice = createSlice({
    name: 'account',
    initialState,
    reducers: {
        setKeyringStatus: (
            state,
            {
                payload,
            }: PayloadAction<
                Required<KeyringPayload<'walletStatusUpdate'>>['return']
            >
        ) => {
            state.isLocked = payload.isLocked;
            state.isInitialized = payload.isInitialized;
            state.address = payload.activeAddress; // is already normalized
            accountsAdapter.setAll(state, payload.accounts);
        },
    },
    extraReducers: (builder) =>
        builder
            .addCase(createVault.pending, (state) => {
                state.creating = true;
            })
            .addCase(createVault.fulfilled, (state) => {
                state.creating = false;
                state.isInitialized = true;
            })
            .addCase(createVault.rejected, (state) => {
                state.creating = false;
            }),
});

export const { setKeyringStatus } = accountSlice.actions;

export const accountsAdapterSelectors = accountsAdapter.getSelectors(
    (state: RootState) => state.account
);

const reducer: Reducer<typeof initialState> = accountSlice.reducer;
export default reducer;

export const activeAccountSelector = (state: RootState) => {
    const {
        account: { address },
    } = state;

    if (address) {
        return accountsAdapterSelectors.selectById(state, address);
    }
    return null;
};

export const activeAddressSelector = ({ account }: RootState) =>
    account.address;
