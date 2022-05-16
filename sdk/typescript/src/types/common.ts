// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/** Base64 string representing the object digest */
export type TransactionDigest = string;
export type SuiAddress = string;
export type ObjectOwner =
  | { AddressOwner: SuiAddress }
  | { ObjectOwner: SuiAddress }
  | 'Shared'
  | 'Immutable';
