// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getObjectId } from '@mysten/sui.js';
import {
    createAsyncThunk,
    createEntityAdapter,
    createSelector,
    createSlice,
} from '@reduxjs/toolkit';
import Browser from 'webextension-polyfill';

import { suiObjectsAdapterSelectors } from '_redux/slices/sui-objects';
import { Coin } from '_redux/slices/sui-objects/Coin';

import type { ObjectId, SuiAddress, SuiMoveObject } from '@mysten/sui.js';
import type { PayloadAction, Reducer } from '@reduxjs/toolkit';
import type { KeyringPayload } from '_payloads/keyring';
import type { RootState } from '_redux/RootReducer';
import type { AccountSerialized } from '_src/background/keyring/Account';
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
        await background.clearWallet();
    }
);

const accountsAdapter = createEntityAdapter<AccountSerialized>({
    selectId: ({ address }) => address,
    sortComparer: (a, b) => {
        if (a.type !== b.type) {
            // first derived accounts
            return a.type === 'derived' ? -1 : 1;
        } else if (a.type === 'derived') {
            // sort derived accounts by derivation path
            return (a.derivationPath || '').localeCompare(
                b.derivationPath || ''
            );
        } else {
            // sort imported account by address
            return a.address.localeCompare(b.address);
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
        setAddress: (state, action: PayloadAction<string | null>) => {
            state.address = '0x84b8f36b2d6bd941af2f7cc5af8861fa00a508da';
            // state.address = action.payload;
        },
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
            // state.address = payload.activeAddress || null; // is already normalized
            state.address = '0x84b8f36b2d6bd941af2f7cc5af8861fa00a508da';
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

export const { setAddress, setKeyringStatus } = accountSlice.actions;

export const accountsAdapterSelectors = accountsAdapter.getSelectors(
    (state: RootState) => state.account
);

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

export function createAccountNftByIdSelector(nftId: ObjectId) {
    return createSelector(
        accountNftsSelector,
        (allNfts) =>
            allNfts.find((nft) => getObjectId(nft.reference) === nftId) || null
    );
}

export function createCoinsForTypeSelector(coinTypeArg: string) {
    return createSelector(accountCoinsSelector, (allCoins) =>
        allCoins.filter((aCoin) => Coin.getCoinTypeArg(aCoin) === coinTypeArg)
    );
}
