// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export const IS_STATIC_ENV = import.meta.env.VITE_NETWORK === 'static';
export const IS_LOCAL_ENV = import.meta.env.VITE_NETWORK === 'local';
export const IS_STAGING_ENV = import.meta.env.VITE_NETWORK === 'staging';
export const IS_PROD_ENV = import.meta.env.VITE_NETWORK === 'prod';

export const CURRENT_ENV = import.meta.env.VITE_NETWORK;
