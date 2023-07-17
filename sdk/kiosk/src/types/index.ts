// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SharedObjectRef, SuiObjectRef, TransactionArgument } from '@mysten/sui.js';

export * from './kiosk';
export * from './transfer-policy';
export * from './env';

/**
 * A valid argument for any of the Kiosk functions.
 */
export type ObjectArgument = string | TransactionArgument | SharedObjectRef | SuiObjectRef;
