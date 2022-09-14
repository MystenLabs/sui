// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export let CURRENT_ENV = import.meta.env.VITE_NETWORK || 'local';

const { hostname } = window.location;
if (hostname.includes('.sui.io')) {
    CURRENT_ENV = hostname.split('.').at(-3) || 'prod';
}

export const IS_STATIC_ENV = CURRENT_ENV === 'static';
export const IS_LOCAL_ENV = CURRENT_ENV === 'local';
export const IS_STAGING_ENV = CURRENT_ENV === 'staging';
export const IS_PROD_ENV = CURRENT_ENV === 'prod';
