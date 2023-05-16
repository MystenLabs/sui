// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

import type { Storage } from 'webextension-polyfill';

const SESSION_STORAGE: Storage.LocalStorageArea | null =
    // @ts-expect-error chrome
    global?.chrome?.storage?.session || null;

async function getFromStorage<T>(
    storage: Storage.LocalStorageArea,
    key: string,
    defaultValue: T | null = null
): Promise<T | null> {
    return (await storage.get({ [key]: defaultValue }))[key];
}

async function setToStorage<T>(
    storage: Storage.LocalStorageArea,
    key: string,
    value: T
): Promise<void> {
    return await storage.set({ [key]: value });
}

export function isSessionStorageSupported() {
    return !!SESSION_STORAGE;
}

//eslint-disable-next-line @typescript-eslint/no-explicit-any
type OmitFirst<T extends any[]> = T extends [any, ...infer R] ? R : never;
type GetParams<T> = OmitFirst<Parameters<typeof getFromStorage<T>>>;
type SetParams<T> = OmitFirst<Parameters<typeof setToStorage<T>>>;

export function getFromLocalStorage<T>(...params: GetParams<T>) {
    return getFromStorage<T>(Browser.storage.local, ...params);
}
export function setToLocalStorage<T>(...params: SetParams<T>) {
    return setToStorage<T>(Browser.storage.local, ...params);
}
export async function getFromSessionStorage<T>(...params: GetParams<T>) {
    if (!SESSION_STORAGE) {
        return null;
    }
    return getFromStorage<T>(SESSION_STORAGE, ...params);
}
export async function setToSessionStorage<T>(...params: SetParams<T>) {
    if (!SESSION_STORAGE) {
        return;
    }
    return setToStorage<T>(SESSION_STORAGE, ...params);
}

export const addSessionStorageEventListener: Browser.Storage.LocalStorageArea['onChanged']['addListener'] =
    (...params) => {
        if (!SESSION_STORAGE) {
            return;
        }
        SESSION_STORAGE.onChanged.addListener(...params);
    };
