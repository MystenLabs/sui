// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createSlice, createAsyncThunk } from '@reduxjs/toolkit';

import { AppType } from './AppType';
import { DEFAULT_API_ENV } from '_app/ApiProvider';

import type { PayloadAction } from '@reduxjs/toolkit';
import type { RootState } from '_redux/RootReducer';
import type { NetworkEnvType } from '_src/background/NetworkEnv';
import type { API_ENV } from '_src/shared/api-env';
import type { AppThunkConfig } from '_store/thunk-extras';

type AppState = {
	appType: AppType;
	apiEnv: API_ENV;
	navVisible: boolean;
	customRPC?: string | null;
	activeOrigin: string | null;
	activeOriginFavIcon: string | null;
};

const initialState: AppState = {
	appType: AppType.unknown,
	apiEnv: DEFAULT_API_ENV,
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
	async ({ network, store = false }, { extra: { background, api }, dispatch }) => {
		if (store) {
			await background.setActiveNetworkEnv(network);
		}
		api.setNewJsonRpcProvider(network.env, network.customRpcUrl);
		await dispatch(slice.actions.setActiveNetwork(network));
	},
);

const slice = createSlice({
	name: 'app',
	reducers: {
		initAppType: (state, { payload }: PayloadAction<AppType>) => {
			state.appType = payload;
		},
		setActiveNetwork: (
			state,
			{ payload: { env, customRpcUrl } }: PayloadAction<NetworkEnvType>,
		) => {
			state.apiEnv = env;
			state.customRPC = customRpcUrl;
		},
		setNavVisibility: (state, { payload: isVisible }: PayloadAction<boolean>) => {
			state.navVisible = isVisible;
		},
		setActiveOrigin: (
			state,
			{ payload }: PayloadAction<{ origin: string | null; favIcon: string | null }>,
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
