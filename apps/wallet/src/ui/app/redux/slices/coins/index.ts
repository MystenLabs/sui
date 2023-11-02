// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createSlice, type PayloadAction } from '@reduxjs/toolkit';

type CoinsState = {
	pinnedCoinTypes: string[];
};

const initialState: CoinsState = {
	pinnedCoinTypes: [],
};

const slice = createSlice({
	name: 'coins',
	reducers: {
		setPinnedCoinTypes: (state, { payload }: PayloadAction<string[]>) => {
			state.pinnedCoinTypes = payload;
		},
	},
	initialState,
});

export const { setPinnedCoinTypes } = slice.actions;
export default slice.reducer;
