// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createSlice, createAsyncThunk } from '@reduxjs/toolkit';

import type { AppThunkConfig } from '_store/thunk-extras';

const SUI_APPS_API_ENDPOINT = process.env.SUI_APPS_API_ENDPOINT;

interface DappApiResponse {
    name: string;
    icon: string;
    link: string;
    tags: string[];
    description: string;
}

interface DappApiData {
    loading: boolean;
    curatedApps: DappApiResponse[];
    error: false | { code?: string; message?: string; name?: string };
}

const initialState: DappApiData = {
    loading: true,
    curatedApps: [],
    error: false,
};

// Get curated list of official apps from SUI Apps API
export const getCuratedApps = createAsyncThunk<
    DappApiResponse[],
    void,
    AppThunkConfig
>('sui-apps/get-curated-apps', async (_): Promise<DappApiResponse[]> => {
    if (!SUI_APPS_API_ENDPOINT) {
        throw new Error('SUI_APPS_API_ENDPOINT is not defined');
    }
    const response = await fetch(SUI_APPS_API_ENDPOINT);
    if (!response.ok) {
        throw new Error('No data returned from SUI Apps API');
    }

    const data = await response.json();
    if (!data || !data.length) {
        throw new Error('No data returned from SUI Apps API');
    }

    return data as DappApiResponse[];
});

const slice = createSlice({
    name: 'curated-apps',
    initialState,
    reducers: {},
    extraReducers: (builder) => {
        builder
            .addCase(getCuratedApps.fulfilled, (state, action) => {
                state.loading = false;
                state.curatedApps = action.payload;
                state.error = false;
            })
            .addCase(getCuratedApps.pending, (state, action) => {
                state.loading = true;
                state.curatedApps = [];
                state.error = false;
            })
            .addCase(
                getCuratedApps.rejected,
                (state, { error: { code, name, message } }) => {
                    state.loading = false;
                    state.error = { code, message, name };
                    state.curatedApps = [];
                }
            );
    },
});

export default slice.reducer;
