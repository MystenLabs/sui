// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createSlice, createAsyncThunk } from '@reduxjs/toolkit';
import Browser from 'webextension-polyfill';

import { AppType } from './AppType';
import { DEFAULT_API_ENV } from '_app/ApiProvider';
import { fetchAllOwnedAndRequiredObjects } from '_redux/slices/sui-objects';
import { getTransactionsByAddress } from '_redux/slices/txresults';

import type { PayloadAction } from '@reduxjs/toolkit';
import type { API_ENV } from '_app/ApiProvider';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

type AppState = {
    appType: AppType;
    apiEnv: API_ENV;
    navVisible: boolean;
    activeOrigin: string | null;
    activeOriginFavIcon: string | null;
};

const initialState: AppState = {
    appType: AppType.unknown,
    apiEnv: DEFAULT_API_ENV,
    navVisible: true,
    activeOrigin: null,
    activeOriginFavIcon: null,
};

// On network change, set setNewJsonRpcProvider, fetch all owned objects, and fetch all transactions
// TODO: add clear Object state because edge cases where use state stays in cache
export const changeRPCNetwork = createAsyncThunk<void, API_ENV, AppThunkConfig>(
    'changeRPCNetwork',
    (networkName, { extra: { api }, dispatch }) => {
        dispatch(setApiEnv(networkName));
        api.setNewJsonRpcProvider(networkName);
        dispatch(getTransactionsByAddress());
        dispatch(fetchAllOwnedAndRequiredObjects());
        // Set persistent network state
        Browser.storage.local.set({ sui_Env: networkName });
    }
);

export const initNetworkFromStorage = createAsyncThunk<
    void,
    void,
    AppThunkConfig
>('initNetworkFromStorage', async (_, { dispatch, extra: { api } }) => {
    const result = await Browser.storage.local.get(['sui_Env']);
    const network = result.sui_Env;
    if (network) {
        api.setNewJsonRpcProvider(network);
        await dispatch(setApiEnv(network));
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
        setNavVisibility: (
            state,
            { payload: isVisible }: PayloadAction<boolean>
        ) => {
            state.navVisible = isVisible;
        },
        setActiveOrigin: (
            state,
            {
                payload,
            }: PayloadAction<{ origin: string | null; favIcon: string | null }>
        ) => {
            state.activeOrigin = payload.origin;
            state.activeOriginFavIcon = payload.favIcon;
        },
    },
    initialState,
});

export const { initAppType, setApiEnv, setNavVisibility, setActiveOrigin } =
    slice.actions;
export const getNavIsVisible = ({ app }: RootState) => app.navVisible;

export default slice.reducer;
