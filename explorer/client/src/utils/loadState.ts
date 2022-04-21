// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type LoadState = 'pending' | 'loaded' | 'fail';

export interface ILoadState {
    loadState: LoadState;
}

// abstraction to help transition explorer from the prototype data model
export type Loadable<T> = T & ILoadState



