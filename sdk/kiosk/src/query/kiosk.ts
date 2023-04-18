// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  JsonRpcProvider,
  PaginationArguments,
  SuiAddress,
} from '@mysten/sui.js';

import { DynamicFieldPage } from '@mysten/sui.js/src/types/dynamic_fields';

export type KioskListing = {
  itemId: string;
  isExclusive: boolean;
};

export type KioskData = {
  listings: KioskListing[];
  items: string[];
};

export async function fetchKiosk(
  provider: JsonRpcProvider,
  kioskId: SuiAddress,
  pagination: PaginationArguments<DynamicFieldPage['nextCursor']> = {},
) {
  const fields = await provider.getDynamicFields({ parentId: kioskId, ...pagination });
  console.log(fields);
}
