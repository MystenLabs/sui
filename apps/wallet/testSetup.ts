// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { webcrypto } from 'crypto';
import { vi } from 'vitest';

import type { Storage } from 'webextension-polyfill';

if (!globalThis.crypto) {
    globalThis.crypto = webcrypto as Crypto;
}

function mockStorage(): Storage.LocalStorageArea {
    return {
        clear: vi.fn(),
        get: vi.fn(),
        remove: vi.fn(),
        set: vi.fn(),
        onChanged: {
            addListener: vi.fn(),
            hasListener: vi.fn(),
            hasListeners: vi.fn(),
            removeListener: vi.fn(),
        },
        QUOTA_BYTES: 5242880,
    };
}

// Create a fake chrome object so that the webextension polyfill can load:
globalThis.chrome = {
    runtime: {
        id: 'some-test-id-from-test-setup',
    },
    storage: {
        local: mockStorage(),
        session: mockStorage(),
    },
};
