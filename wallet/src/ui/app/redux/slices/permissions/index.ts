// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    createAsyncThunk,
    createEntityAdapter,
    createSelector,
    createSlice,
} from '@reduxjs/toolkit';
import Browser from 'webextension-polyfill';

import { activeAccountSelector } from '../account';

import type { SuiAddress } from '@mysten/sui.js';
import type { PayloadAction } from '@reduxjs/toolkit';
import type { Permission } from '_messages/payloads/permissions';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

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
        accounts: SuiAddress[];
        allowed: boolean;
        responseDate: string;
    },
    { id: string; accounts: SuiAddress[]; allowed: boolean },
    AppThunkConfig
>(
    'respond-to-permission-request',
    ({ id, accounts, allowed }, { extra: { background } }) => {
        const responseDate = new Date().toISOString();
        background.sendPermissionResponse(id, accounts, allowed, responseDate);
        return { id, accounts, allowed, responseDate };
    }
);

// Todo: move this to the wallet adapter
// Get all permissions from from storage
// remove the permission for a given origin
// set the new permissions in storage
export const revokeAppPermissionByOrigin = createAsyncThunk<
    void,
    { origin: string },
    AppThunkConfig
>('revoke-app-permission', async ({ origin }, { dispatch }) => {
    const connectedApps = await Browser.storage.local.get('permissions');
    if (connectedApps.permissions[origin]) {
        const appId = connectedApps.permissions[origin].id;
        delete connectedApps.permissions[origin];
        // remove app from state store
        dispatch(revokeAppPermission(appId));
        await Browser.storage.local.set({
            permissions: connectedApps.permissions,
        });
        return;
    }
    return;
});

const slice = createSlice({
    name: 'permissions',
    initialState: permissionsAdapter.getInitialState({ initialized: false }),
    reducers: {
        setPermissions: (state, { payload }: PayloadAction<Permission[]>) => {
            permissionsAdapter.setAll(state, payload);
            state.initialized = true;
        },
        revokeAppPermission: (state, { payload }: PayloadAction<string>) => {
            permissionsAdapter.removeOne(state, payload);
        },
    },
    extraReducers: (build) => {
        build.addCase(
            respondToPermissionRequest.fulfilled,
            (state, { payload }) => {
                const { id, accounts, allowed, responseDate } = payload;
                permissionsAdapter.updateOne(state, {
                    id,
                    changes: {
                        accounts,
                        allowed,
                        responseDate,
                    },
                });
            }
        );
    },
});

export default slice.reducer;

export const { setPermissions, revokeAppPermission } = slice.actions;

export const permissionsSelectors = permissionsAdapter.getSelectors(
    (state: RootState) => state.permissions
);

export function createDappStatusSelector(origin: string | null) {
    if (!origin) {
        return () => false;
    }
    return createSelector(
        permissionsSelectors.selectAll,
        activeAccountSelector,
        (permissions, activeAccount) => {
            const originPermission = permissions.find(
                (aPermission) => aPermission.origin === origin
            );
            if (!originPermission) {
                return false;
            }
            return (
                originPermission.allowed &&
                activeAccount &&
                originPermission.accounts.includes(activeAccount)
            );
        }
    );
}
