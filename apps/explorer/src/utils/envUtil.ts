// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Network } from './api/rpcSetting';

export const DEFAULT_NETWORK =
    import.meta.env.VITE_NETWORK ||
    (import.meta.env.DEV ? Network.LOCAL : Network.DEVNET);

export const IS_STATIC_ENV = DEFAULT_NETWORK === Network.STATIC;
export const IS_STAGING_ENV = DEFAULT_NETWORK === Network.LOCAL;
