// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
export function debounce(func: Function, wait: number = 250) {
    let timeout: ReturnType<typeof setTimeout>;
    return function (this: any, ...args: any[]) {
        clearTimeout(timeout);
        timeout = setTimeout(() => func.apply(this, args), wait);
    };
}
