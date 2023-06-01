// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectOwner } from '@mysten/sui.js';

/** The Transfer Policy module. */
export const TRANSFER_POLICY_MODULE = '0x2::transfer_policy';

/** Name of the event emitted when a TransferPolicy for T is created. */
export const TRANSFER_POLICY_CREATED_EVENT = `${TRANSFER_POLICY_MODULE}::TransferPolicyCreated`;

/** The Transfer Policy Type */
export const TRANSFER_POLICY_TYPE = `${TRANSFER_POLICY_MODULE}::TransferPolicy`;

/** The Transer Policy Rules package address */
export const TESTNET_TRANSFER_POLICY_RULES_PACKAGE_ADDRESS =
  'bd8fc1947cf119350184107a3087e2dc27efefa0dd82e25a1f699069fe81a585';

/** The `TransferPolicy` object */
export type TransferPolicy = {
  id: string;
  type: string;
  balance: string;
  rules: string[];
  owner: ObjectOwner;
};

/** Event emitted when a TransferPolicy is created. */
export type TransferPolicyCreated = {
  id: string;
};
