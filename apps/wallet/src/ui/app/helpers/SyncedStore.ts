// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export class SyncedStore<T> {
    #store: T;
    #listeners: (() => void)[] = [];

    constructor(value: T) {
        this.#store = value;
    }

    setValue = (value: T) => {
        this.#store = value;
        this.#notify();
    };

    subscribe = (callback: () => void) => {
        this.#listeners.push(callback);
        return () => {
            this.#listeners = this.#listeners.filter(
                (aListener) => aListener !== callback
            );
        };
    };

    getSnapshot = () => {
        return this.#store;
    };

    #notify() {
        for (const aListener of this.#listeners) {
            aListener();
        }
    }
}
