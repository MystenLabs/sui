// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getObjectId,
    getObjectVersion,
    getSuiObjectData,
    SUI_SYSTEM_STATE_OBJECT_ID,
} from '@mysten/sui.js';
import {
    createAsyncThunk,
    createEntityAdapter,
    createSlice,
} from '@reduxjs/toolkit';

import { activeAccountSelector } from '../account';
import { ExampleNFT } from './NFT';

import type { SuiObjectData, ObjectId } from '@mysten/sui.js';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

const objectsAdapter = createEntityAdapter<SuiObjectData>({
    selectId: ({ objectId }) => objectId,
    sortComparer: (a, b) => a.objectId.localeCompare(b.objectId),
});

export const fetchAllOwnedAndRequiredObjects = createAsyncThunk<
    SuiObjectData[],
    void,
    AppThunkConfig
>('sui-objects/fetch-all', async (_, { getState, extra: { api } }) => {
    const state = getState();
    const {
        account: { address },
    } = state;
    const allSuiObjects: SuiObjectData[] = [];
    if (address) {
        const allObjectRefs =
            await api.instance.fullNode.getObjectsOwnedByAddress(`${address}`);
        const objectIDs = allObjectRefs
            .filter((anObj) => {
                const fetchedVersion = getObjectVersion(anObj);
                const storedObj = suiObjectsAdapterSelectors.selectById(
                    state,
                    getObjectId(anObj)
                );
                const storedVersion = storedObj
                    ? getObjectVersion(storedObj)
                    : null;
                const objOutdated = fetchedVersion !== storedVersion;
                if (!objOutdated && storedObj) {
                    allSuiObjects.push(storedObj);
                }
                return objOutdated;
            })
            .map((anObj) => anObj.objectId);
        objectIDs.push(SUI_SYSTEM_STATE_OBJECT_ID);
        const allObjRes = await api.instance.fullNode.getObjectBatch(
            objectIDs,
            {
                showType: true,
                showContent: true,
                showOwner: true,
                showPreviousTransaction: true,
                showStorageRebate: true,
            }
        );
        for (const objRes of allObjRes) {
            const suiObj = getSuiObjectData(objRes);
            if (suiObj) {
                allSuiObjects.push(suiObj);
            }
        }
    }
    return allSuiObjects;
});

export const batchFetchObject = createAsyncThunk<
    SuiObjectData[],
    ObjectId[],
    AppThunkConfig
>('sui-objects/batch', async (objectIDs, { extra: { api } }) => {
    const allSuiObjects: SuiObjectData[] = [];
    const allObjRes = await api.instance.fullNode.getObjectBatch(objectIDs, {
        showType: true,
        showContent: true,
        showOwner: true,
    });
    for (const objRes of allObjRes) {
        const suiObj = getSuiObjectData(objRes);
        if (suiObj) {
            allSuiObjects.push(suiObj);
        }
    }
    return allSuiObjects;
});

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
    reducers: {
        clearSuiObjects: (state) => {
            state.error = false;
            state.lastSync = null;
            objectsAdapter.removeAll(state);
        },
    },
    extraReducers: (builder) => {
        builder
            .addCase(
                fetchAllOwnedAndRequiredObjects.fulfilled,
                (state, action) => {
                    objectsAdapter.setAll(state, action.payload);
                    state.loading = false;
                    state.error = false;
                    state.lastSync = Date.now();
                }
            )
            .addCase(
                fetchAllOwnedAndRequiredObjects.pending,
                (state, action) => {
                    state.loading = true;
                }
            )
            .addCase(
                fetchAllOwnedAndRequiredObjects.rejected,
                (state, { error: { code, name, message } }) => {
                    state.loading = false;
                    state.error = { code, message, name };
                }
            );
    },
});

export default slice.reducer;

export const { clearSuiObjects } = slice.actions;

export const suiObjectsAdapterSelectors = objectsAdapter.getSelectors(
    (state: RootState) => state.suiObjects
);

export const suiSystemObjectSelector = (state: RootState) =>
    suiObjectsAdapterSelectors.selectById(state, SUI_SYSTEM_STATE_OBJECT_ID);
