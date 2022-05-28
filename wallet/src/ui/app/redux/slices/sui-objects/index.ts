// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getObjectExistsResponse } from '@mysten/sui.js';
import {
    createAsyncThunk,
    createEntityAdapter,
    createSlice,
} from '@reduxjs/toolkit';

import type { SuiObject } from '@mysten/sui.js';
import type { AppThunkConfig } from '_store/thunk-extras';

const objectsAdapter = createEntityAdapter<SuiObject>({
    selectId: ({ reference }) => reference.objectId,
});

export const fetchAllOwnedObjects = createAsyncThunk<
    SuiObject[],
    void,
    AppThunkConfig
>('sui-objects/fetch-all', async (_, { getState, extra: { api } }) => {
    const address = getState().account.address;
    const allSuiObjects: SuiObject[] = [];
    if (address) {
        const allObjectRefs = await api.instance.getObjectsOwnedByAddress(
            `${address}`
        );
        const objectIDs = allObjectRefs.map((anObj) => anObj.objectId);
        const allObjRes = await api.instance.getObjectBatch(objectIDs);
        for (const objRes of allObjRes) {
            const suiObj = getObjectExistsResponse(objRes);
            if (suiObj) {
                allSuiObjects.push(suiObj);
            }
        }
    }
    return allSuiObjects;
});

const objectsAdapterInitialState = objectsAdapter.getInitialState();
type SuiObjectsAdapterType = typeof objectsAdapterInitialState;
interface SuiObjectsState extends SuiObjectsAdapterType {
    loading: boolean;
    error: false | { code?: string; message?: string; name?: string };
    lastSync: number | null;
}
const initialState: SuiObjectsState = {
    ...objectsAdapterInitialState,
    loading: true,
    error: false,
    lastSync: null,
};

const slice = createSlice({
    name: 'sui-objects',
    initialState: initialState,
    reducers: {
        setOwnedObjects: objectsAdapter.setAll,
    },
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

export const suiObjectsAdapterSelectors = objectsAdapter.getSelectors();
