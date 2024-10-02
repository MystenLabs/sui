// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Permission } from '_messages/payloads/permissions';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';
import {
	createAsyncThunk,
	createEntityAdapter,
	createSelector,
	createSlice,
} from '@reduxjs/toolkit';
import type { PayloadAction } from '@reduxjs/toolkit';

const permissionsAdapter = createEntityAdapter<Permission>({
	sortComparer: (a, b) => {
		const aDate = new Date(a.createdDate);
		const bDate = new Date(b.createdDate);
		return aDate.getTime() - bDate.getTime();
	},
});

export const respondToPermissionRequest = createAsyncThunk<
	{
		id: string;
		accounts: string[];
		allowed: boolean;
		responseDate: string;
	},
	{ id: string; accounts: string[]; allowed: boolean },
	AppThunkConfig
>('respond-to-permission-request', ({ id, accounts, allowed }, { extra: { background } }) => {
	const responseDate = new Date().toISOString();
	background.sendPermissionResponse(id, accounts, allowed, responseDate);
	return { id, accounts, allowed, responseDate };
});

const slice = createSlice({
	name: 'permissions',
	initialState: permissionsAdapter.getInitialState({ initialized: false }),
	reducers: {
		setPermissions: (state, { payload }: PayloadAction<Permission[]>) => {
			permissionsAdapter.setAll(state, payload);
			state.initialized = true;
		},
	},
	extraReducers: (build) => {
		build.addCase(respondToPermissionRequest.fulfilled, (state, { payload }) => {
			const { id, accounts, allowed, responseDate } = payload;
			permissionsAdapter.updateOne(state, {
				id,
				changes: {
					accounts,
					allowed,
					responseDate,
				},
			});
		});
	},
});

export default slice.reducer;

export const { setPermissions } = slice.actions;

export const permissionsSelectors = permissionsAdapter.getSelectors(
	(state: RootState) => state.permissions,
);

export function createDappStatusSelector(origin: string | null, activeAddress: string | null) {
	if (!origin || !activeAddress) {
		return () => false;
	}
	return createSelector(permissionsSelectors.selectAll, (permissions) => {
		const originPermission = permissions.find((aPermission) => aPermission.origin === origin);
		if (!originPermission) {
			return false;
		}
		return (
			originPermission.allowed && activeAddress && originPermission.accounts.includes(activeAddress)
		);
	});
}
