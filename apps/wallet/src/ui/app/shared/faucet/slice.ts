// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createSlice } from '@reduxjs/toolkit';

import { requestGas } from './actions';

type SliceType = {
    loading: boolean;
    lastRequest: {
        status: number;
        statusTxt: string;
        error: boolean;
        totalGasReceived?: number;
        retryAfter?: number;
    } | null;
};

const initialState: SliceType = {
    loading: false,
    lastRequest: null,
};

const slice = createSlice({
    name: 'faucet',
    reducers: {
        clearLastRequest: (state) => {
            state.lastRequest = null;
        },
    },
    extraReducers(builder) {
        builder
            .addCase(requestGas.pending, (state) => {
                state.loading = true;
                state.lastRequest = null;
            })
            .addCase(
                requestGas.fulfilled,
                (state, { payload: { status, statusTxt, total } }) => {
                    state.loading = false;
                    state.lastRequest = {
                        status,
                        statusTxt,
                        totalGasReceived: total,
                        error: false,
                    };
                }
            )
            .addCase(requestGas.rejected, (state, { payload }) => {
                state.loading = false;
                state.lastRequest = {
                    status: payload?.status ?? -1,
                    statusTxt: payload?.statusTxt || '',
                    retryAfter: payload?.retryAfter,
                    error: true,
                };
            });
    },
    initialState,
});

export const { clearLastRequest } = slice.actions;
export const reducer = slice.reducer;
