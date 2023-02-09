// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import ApiProvider from '_app/ApiProvider';
import { BackgroundClient } from '_app/background-client';
import { growthbook } from '_app/experimentation/feature-gating';

import type { RootState } from '_redux/RootReducer';
import type { AppDispatch } from '_store';

import WebHIDTransport from '@ledgerhq/hw-transport-webhid';
import WebUSBTransport from '@ledgerhq/hw-transport-webusb';
import type Transport from '@ledgerhq/hw-transport';
import AppSui from 'hw-app-sui';

export const api = new ApiProvider();

let appSui: AppSui | null = null;

export const thunkExtras = {
    api,
    growthbook,
    background: new BackgroundClient(),
    initAppSui,
};

type ThunkExtras = typeof thunkExtras;

export interface AppThunkConfig {
    extra: ThunkExtras;
    state: RootState;
    dispatch: AppDispatch;
}

const getTransport = async () => {
    let transport = null;
    let error;
    //try {
    //    return await WebHIDTransport.request();
    //} catch (e) {
    //    console.error(`HID Transport is not supported: ${e}`);
    //    error = e;
    //}

    if ((window as any).USB) {
        try {
            return await WebUSBTransport.request();
        } catch (e) {
            console.error(`WebUSB Transport is not supported: ${e}`);
            error = e;
        }
    }

    throw error;
};

async function initAppSui(): Promise<AppSui> {
    if (appSui === null) {
        appSui = new AppSui(await getTransport());
    }
    return appSui;
}
