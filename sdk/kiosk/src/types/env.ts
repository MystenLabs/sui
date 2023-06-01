// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiAddress } from '@mysten/sui.js';

/* A list of environments. */
export type Environment = 'mainnet' | 'testnet' | 'devnet' | 'custom';

/** A Parameter to support enivronments for rules.  */
export type RulesEnvironmentParam = { env: Environment; address?: SuiAddress };
