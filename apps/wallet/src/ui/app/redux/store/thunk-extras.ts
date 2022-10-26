// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import ApiProvider from '_app/ApiProvider';
import KeypairVault from '_app/KeypairVault';
import { BackgroundClient } from '_app/background-client';
import { growthbook } from '_app/experimentation/feature-gating';

import type { RootState } from '_redux/RootReducer';
import type { AppDispatch } from '_store';

export const api = new ApiProvider();

export const thunkExtras = {
    api,
    growthbook,
    keypairVault: new KeypairVault(),
    background: new BackgroundClient(),
};

type ThunkExtras = typeof thunkExtras;

export interface AppThunkConfig {
    extra: ThunkExtras;
    state: RootState;
    dispatch: AppDispatch;
}
