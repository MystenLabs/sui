// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  MIST_PER_SUI,
  ObjectId,
  SuiObjectResponse,
  getObjectDisplay,
  getObjectId,
} from '@mysten/sui.js';
// Parse the display of a list of objects into a simple {object_id: display} map
// to use throughout the app.
export const parseObjectDisplays = (
  data: SuiObjectResponse[],
): Record<ObjectId, Record<string, string> | undefined> => {
  return data.reduce<Record<ObjectId, Record<string, string> | undefined>>(
    (acc, item: SuiObjectResponse) => {
      const display = getObjectDisplay(item)?.data;
      const id = getObjectId(item);
      acc[id] = display || undefined;
      return acc;
    },
    {},
  );
};

export const localStorageKeys = {
  LAST_VISITED_KIOSK_ID: 'LAST_VISITED_KIOSK_ID',
  USER_KIOSK_OWNER_CAP: 'USER_KIOSK_OWNER_CAP',
  USER_KIOSK_ID: 'USER_KIOSK',
};

export const getOwnedKiosk = (): string | null => {
  return localStorage.getItem(localStorageKeys.USER_KIOSK_ID) || null;
};

export const getOwnedKioskCap = (): string | null => {
  return localStorage.getItem(localStorageKeys.USER_KIOSK_OWNER_CAP) || null;
};

export const mistToSui = (mist: bigint | string | undefined) => {
  if (!mist) return 0;
  return Number(mist || 0) / Number(MIST_PER_SUI);
};

export const formatSui = (amount: number) => {
  return new Intl.NumberFormat('en-US', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 5,
  }).format(amount);
};
