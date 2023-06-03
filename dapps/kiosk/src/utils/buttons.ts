// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { OwnedObjectType } from '../components/Inventory/OwnedObjects';

export const actionWithLoader = async (
  fn: (item: OwnedObjectType, price?: string) => void,
  item: OwnedObjectType,
  setLoading: (state: boolean) => void,
) => {
  setLoading(true);
  await fn(item);
  setLoading(false);
};
