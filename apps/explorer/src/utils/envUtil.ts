// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Network } from './api/rpcSetting';

export const DEFAULT_NETWORK =
    import.meta.env.VITE_NETWORK ||
    (import.meta.env.DEV ? Network.LOCAL : Network.DEVNET);
