// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createSlice, createAsyncThunk } from '@reduxjs/toolkit';

import { AppType } from './AppType';
import { DEFAULT_API_ENV, type API_ENV } from '_app/ApiProvider';
import {
    clearForNetworkSwitch,
    fetchAllOwnedAndRequiredObjects,
} from '_redux/slices/sui-objects';

import type { PayloadAction } from '@reduxjs/toolkit';
import type { RootState } from '_redux/RootReducer';
import type { NetworkEnvType } from '_src/background/NetworkEnv';
import type { AppThunkConfig } from '_store/thunk-extras';

type AppState = {
    appType: AppType;
    apiEnv: API_ENV;
    apiEnvInitialized: boolean;
    navVisible: boolean;
    customRPC?: string | null;
    activeOrigin: string | null;
    activeOriginFavIcon: string | null;
};

const initialState: AppState = {
    appType: AppType.unknown,
    apiEnv: DEFAULT_API_ENV,
    apiEnvInitialized: false,
    customRPC: null,
    navVisible: true,
    activeOrigin: null,
    activeOriginFavIcon: null,
};

export const changeActiveNetwork = createAsyncThunk<
    void,
    { network: NetworkEnvType; store?: boolean },
    AppThunkConfig
>(
    'changeRPCNetwork',
    async (
        { network, store = false },
        { extra: { background, api }, dispatch, getState }
    ) => {
        if (store) {
            await background.setActiveNetworkEnv(network);
        }
        const { apiEnvInitialized } = getState().app;
        await dispatch(slice.actions.setActiveNetwork(network));
        api.setNewJsonRpcProvider(network.env, network.customRpcUrl);
        if (apiEnvInitialized) {
            await dispatch(clearForNetworkSwitch());
            dispatch(fetchAllOwnedAndRequiredObjects());
        }
    }
);

const slice = createSlice({
    name: 'app',
    reducers: {
        initAppType: (state, { payload }: PayloadAction<AppType>) => {
            state.appType = payload;
        },
        setActiveNetwork: (
            state,
            { payload: { env, customRpcUrl } }: PayloadAction<NetworkEnvType>
        ) => {
            state.apiEnv = env;
            state.customRPC = customRpcUrl;
            state.apiEnvInitialized = true;
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

export const { initAppType, setNavVisibility, setActiveOrigin } = slice.actions;
export const getNavIsVisible = ({ app }: RootState) => app.navVisible;

export default slice.reducer;
