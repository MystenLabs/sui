// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createSlice, createAsyncThunk } from '@reduxjs/toolkit';
import Browser from 'webextension-polyfill';

import { AppType } from './AppType';
import { DEFAULT_API_ENV } from '_app/ApiProvider';
import { fetchAllOwnedObjects } from '_redux/slices/sui-objects';
import { getTransactionsByAddress } from '_redux/slices/txresults';

import type { PayloadAction } from '@reduxjs/toolkit';
import type { API_ENV } from '_app/ApiProvider';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

type AppState = {
    appType: AppType;
    apiEnv: API_ENV;
    showHideNetwork: boolean;
    navVisible: boolean;
};

const initialState: AppState = {
    appType: AppType.unknown,
    apiEnv: DEFAULT_API_ENV,
    showHideNetwork: false,
    navVisible: true,
};

// On network change, set setNewJsonRpcProvider, fetch all owned objects, and fetch all transactions
// TODO: add clear Object state because edge cases where use state stays in cache
export const changeRPCNetwork = createAsyncThunk<void, API_ENV, AppThunkConfig>(
    'changeRPCNetwork',
    (networkName, { extra: { api }, dispatch }) => {
        dispatch(setApiEnv(networkName));
        api.setNewJsonRpcProvider(networkName);
        dispatch(setNetworkSelector(true));
        dispatch(getTransactionsByAddress());
        dispatch(fetchAllOwnedObjects());
        // Set persistent network state
        Browser.storage.local.set({ sui_Env: networkName });
    }
);

export const loadNetworkFromStorage = createAsyncThunk<
    void,
    void,
    AppThunkConfig
>('loadNetworkFromStorage', async (_, { dispatch }) => {
    const result = await Browser.storage.local.get(['sui_Env']);
    if (result.sui_Env) {
        await dispatch(changeRPCNetwork(result.sui_Env));
    }
});

const slice = createSlice({
    name: 'app',
    reducers: {
        initAppType: (state, { payload }: PayloadAction<AppType>) => {
            state.appType = payload;
        },
        setApiEnv: (state, { payload }: PayloadAction<API_ENV>) => {
            state.apiEnv = payload;
        },
        // TODO: move to a separate slice
        setNetworkSelector: (state, { payload }: PayloadAction<boolean>) => {
            state.showHideNetwork = !payload;
        },
        setNavVisibility: (
            state,
            { payload: isVisible }: PayloadAction<boolean>
        ) => {
            state.navVisible = isVisible;
        },
    },

    initialState,
});

export const { initAppType, setApiEnv, setNetworkSelector, setNavVisibility } =
    slice.actions;
export const getNavIsVisible = ({ app }: RootState) => app.navVisible;

export default slice.reducer;
