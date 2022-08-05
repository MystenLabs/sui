// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    createSlice,
    // createSelector,
    createEntityAdapter,
} from '@reduxjs/toolkit';

import type { SuiObject } from '@mysten/sui.js';
import type { PayloadAction } from '@reduxjs/toolkit';

export type ActiveNFT = {
    data?: SuiObject;
    loaded: boolean;
};
const initialState: ActiveNFT = {
    loaded: false,
};
const selectedNFTAdapter = createEntityAdapter<ActiveNFT>({});

const selectedNft = createSlice({
    name: 'selected-nft',
    initialState: selectedNFTAdapter.getInitialState(initialState),
    reducers: {
        setSelectedNFT: (state, { payload }: PayloadAction<ActiveNFT>) => {
            state.data = payload.data;
            state.loaded = payload.loaded;
        },
        clearActiveNFT: (state) => {
            state.data = undefined;
            state.loaded = false;
        },
    },
});
export const { setSelectedNFT, clearActiveNFT } = selectedNft.actions;
export default selectedNft.reducer;
