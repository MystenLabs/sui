// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createSlice, createAsyncThunk } from '@reduxjs/toolkit';

import { AppType } from './AppType';
import { DEFAULT_API_ENV } from '_app/ApiProvider';
// import { getTransactionsByAddress } from '_redux/slices/txresults';

import type { PayloadAction } from '@reduxjs/toolkit';
import type { API_ENV } from '_app/ApiProvider';
import type { AppThunkConfig } from '_store/thunk-extras';

type AppState = {
    appType: AppType;
    apiEnv: API_ENV;
    showHideNetwork: boolean;
};

const initialState: AppState = {
    appType: AppType.unknown,
    apiEnv: DEFAULT_API_ENV,
    showHideNetwork: false,
};

export const changeRPCNetwork = createAsyncThunk<void, void, AppThunkConfig>(
    'changeRPCNetwork',
    async (_, { extra: { api }, getState }) => {
        const newApiEnv = getState().app.apiEnv;
        api.setNewJsonRpcProvider(newApiEnv || 'devNet');
    }
);

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
    },

    initialState,
});

export const { initAppType, setApiEnv, setNetworkSelector } = slice.actions;

export default slice.reducer;
