// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createAsyncThunk } from '@reduxjs/toolkit';

import { isErrorPayload } from '_payloads';

import type { AppThunkConfig } from '_redux/store/thunk-extras';

export const unlockWallet = createAsyncThunk<
    void,
    { password: string },
    AppThunkConfig
>('wallet-unlock-wallet', async ({ password }, { extra: { background } }) => {
    const { payload } = await background.unlockWallet(password);
    if (isErrorPayload(payload)) {
        throw new Error(payload.message);
    }
});
