// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { string, Infer } from 'superstruct';

export type TransactionDigest = Infer<typeof TransactionDigestSchema>;
export type SuiAddress = Infer<typeof SuiAddressSchema>;

export const TransactionDigestSchema = string();
export const SuiAddressSchema = string();
