// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getObjectExistsResponse } from '@mysten/sui.js';
import {
    createAsyncThunk,
    createEntityAdapter,
    createSlice,
} from '@reduxjs/toolkit';

import { ExampleNFT } from './NFT';

import type { SuiObject, SuiAddress, ObjectId } from '@mysten/sui.js';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

const objectsAdapter = createEntityAdapter<SuiObject>({
    selectId: ({ reference }) => reference.objectId,
    sortComparer: (a, b) =>
        a.reference.objectId.localeCompare(b.reference.objectId),
});

export const fetchAllOwnedObjects = createAsyncThunk<
    SuiObject[],
    void,
    AppThunkConfig
>('sui-objects/fetch-all', async (_, { getState, extra: { api } }) => {
    const address = getState().account.address;
    const allSuiObjects: SuiObject[] = [];
    if (address) {
        const allObjectRefs =
            await api.instance.fullNode.getObjectsOwnedByAddress(`${address}`);
        const objectIDs = allObjectRefs.map((anObj) => anObj.objectId);
        const allObjRes = await api.instance.fullNode.getObjectBatch(objectIDs);
        for (const objRes of allObjRes) {
            const suiObj = getObjectExistsResponse(objRes);
            if (suiObj) {
                allSuiObjects.push(suiObj);
            }
        }
    }
    return allSuiObjects;
});

export const mintDemoNFT = createAsyncThunk<void, void, AppThunkConfig>(
    'mintDemoNFT',
    async (_, { extra: { api, keypairVault }, dispatch }) => {
        await ExampleNFT.mintExampleNFT(
            api.getSignerInstance(keypairVault.getKeyPair())
        );
        await dispatch(fetchAllOwnedObjects());
    }
);

export const transferSuiNFT = createAsyncThunk<
    void,
    { nftId: ObjectId; recipientAddress: SuiAddress },
    AppThunkConfig
>(
    'transferSuiNFT',
    async (data, { extra: { api, keypairVault }, dispatch }) => {
        await ExampleNFT.TransferNFT(
            api.getSignerInstance(keypairVault.getKeyPair()),
            data.nftId,
            data.recipientAddress
        );
        await dispatch(fetchAllOwnedObjects());
    }
);
interface SuiObjectsManualState {
    loading: boolean;
    error: false | { code?: string; message?: string; name?: string };
    lastSync: number | null;
}
const initialState = objectsAdapter.getInitialState<SuiObjectsManualState>({
    loading: true,
    error: false,
    lastSync: null,
});

const slice = createSlice({
    name: 'sui-objects',
    initialState: initialState,
    reducers: {},
    extraReducers: (builder) => {
        builder
            .addCase(fetchAllOwnedObjects.fulfilled, (state, action) => {
                objectsAdapter.setAll(state, action.payload);
                state.loading = false;
                state.error = false;
                state.lastSync = Date.now();
            })
            .addCase(fetchAllOwnedObjects.pending, (state, action) => {
                state.loading = true;
            })
            .addCase(
                fetchAllOwnedObjects.rejected,
                (state, { error: { code, name, message } }) => {
                    state.loading = false;
                    state.error = { code, message, name };
                }
            );
    },
});

export default slice.reducer;

export const suiObjectsAdapterSelectors = objectsAdapter.getSelectors(
    (state: RootState) => state.suiObjects
);
