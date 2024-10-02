// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { AppThunkConfig } from '_redux/store/thunk-extras';
import { createAsyncThunk } from '@reduxjs/toolkit';

export const appDisconnect = createAsyncThunk<
	void,
	{ origin: string; accounts: string[] },
	AppThunkConfig
>('dapp-status-app-disconnect', async ({ origin, accounts }, { extra: { background } }) => {
	await background.disconnectApp(origin, accounts);
	await background.sendGetPermissionRequests();
});
