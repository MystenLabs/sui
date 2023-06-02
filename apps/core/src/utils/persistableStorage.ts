// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CookieStorage } from '@amplitude/analytics-client-common';
import { MemoryStorage } from '@amplitude/analytics-core';
import { CookieStorageOptions, type Storage } from '@amplitude/analytics-types';

/**
 * A custom storage mechanism for Amplitude that stores device
 * data in memory until we persist the storage to cookies. This
 * allows us to collect analytics data in a GDPR-compliant way
 * before the user has formally provided consent for us to use
 * tracking cookies :)
 */
export class PersistableStorage<T> implements Storage<T> {
    #cookieStorage: CookieStorage<T>;
    #memoryStorage: MemoryStorage<T>;
    #isPersisted: boolean;

    constructor(options?: CookieStorageOptions) {
        this.#cookieStorage = new CookieStorage<T>({
            // These are the default options that the Amplitude SDK uses under the hood
            expirationDays: 365,
            sameSite: 'Lax',
            ...options,
        });
        this.#memoryStorage = new MemoryStorage<T>();
        this.#isPersisted = false;
    }

    async isEnabled(): Promise<boolean> {
        return this.#getActiveStorage().isEnabled();
    }

    async get(key: string): Promise<T | undefined> {
        return this.#getActiveStorage().get(key);
    }

    async getRaw(key: string): Promise<string | undefined> {
        return this.#getActiveStorage().getRaw(key);
    }

    async set(key: string, value: T): Promise<void> {
        this.#getActiveStorage().set(key, value);
    }

    async remove(key: string): Promise<void> {
        this.#getActiveStorage().remove(key);
    }

    async reset(): Promise<void> {
        this.#getActiveStorage().reset();
    }

    persist() {
        this.#isPersisted = true;
        for (const [key, value] of this.#memoryStorage.memoryStorage) {
            this.#cookieStorage.set(key, value);
        }
        this.#memoryStorage.reset();
    }

    #getActiveStorage() {
        return this.#isPersisted ? this.#cookieStorage : this.#memoryStorage;
    }
}
