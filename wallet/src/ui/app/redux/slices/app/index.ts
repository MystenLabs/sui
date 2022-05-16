// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createSlice } from '@reduxjs/toolkit';

import { AppType } from './AppType';

import type { PayloadAction } from '@reduxjs/toolkit';

type AppState = {
    appType: AppType;
};

const initialState: AppState = {
    appType: AppType.unknown,
};

const slice = createSlice({
    name: 'app',
    reducers: {
        initAppType: (state, { payload }: PayloadAction<AppType>) => {
            state.appType = payload;
        },
    },
    initialState,
});

export const { initAppType } = slice.actions;

export default slice.reducer;
