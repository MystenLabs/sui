// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';
import { createAsyncThunk } from '@reduxjs/toolkit';

import type { AppThunkConfig } from '_redux/store/thunk-extras';

export const appDisconnect = createAsyncThunk<
	void,
	{ origin: string; accounts: SuiAddress[] },
	AppThunkConfig
>('dapp-status-app-disconnect', async ({ origin, accounts }, { extra: { background } }) => {
	await background.disconnectApp(origin, accounts);
	await background.sendGetPermissionRequests();
});
