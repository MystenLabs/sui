// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createSlice } from '@reduxjs/toolkit';

import { AppType } from './AppType';
import { DEFAULT_API_ENV } from '_app/ApiProvider';

import type { PayloadAction } from '@reduxjs/toolkit';
import type { API_ENV } from '_app/ApiProvider';

type AppState = {
    appType: AppType;
    apiEnv: API_ENV | null;
};

const initialState: AppState = {
    appType: AppType.unknown,
    apiEnv: DEFAULT_API_ENV,
};

const slice = createSlice({
    name: 'app',
    reducers: {
        initAppType: (state, { payload }: PayloadAction<AppType>) => {
            state.appType = payload;
        },
        setApiEnv: (state, { payload }: PayloadAction<API_ENV>) => {
            state.apiEnv = payload;
        },
    },
    initialState,
});

export const { initAppType, setApiEnv } = slice.actions;

export default slice.reducer;
