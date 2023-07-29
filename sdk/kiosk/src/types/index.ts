// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SharedObjectRef } from '@mysten/sui.js/bcs';
import { SuiObjectRef } from '@mysten/sui.js/client';
import { TransactionArgument } from '@mysten/sui.js/transactions';

export * from './kiosk';
export * from './transfer-policy';
export * from './env';

/**
 * A valid argument for any of the Kiosk functions.
 */
export type ObjectArgument = string | TransactionArgument | SharedObjectRef | SuiObjectRef;
