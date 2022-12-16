// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Storage } from 'webextension-polyfill';

export const SESSION_STORAGE: Storage.LocalStorageArea | null =
    // @ts-expect-error chrome
    global?.chrome?.storage?.session || null;
export const IS_SESSION_STORAGE_SUPPORTED = !!SESSION_STORAGE;
