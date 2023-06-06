// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from '@mysten/wallet-kit';
import { useQuery } from '@tanstack/react-query';
import {
  TANSTACK_KIOSK_DATA_KEY,
  TANSTACK_KIOSK_KEY,
  TANSTACK_OWNED_KIOSK_KEY,
} from '../utils/constants';
import { useRpc } from '../context/RpcClientContext';
import {
  ObjectId,
  SuiAddress,
  SuiObjectResponse,
  getObjectFields,
  getObjectId,
} from '@mysten/sui.js';
import {
  KIOSK_OWNER_CAP,
  Kiosk,
  KioskData,
  KioskItem,
  KioskListing,
  fetchKiosk,
  getKioskObject,
} from '@mysten/kiosk';
import { parseObjectDisplays, processKioskListings } from '../utils/utils';
import { OwnedObjectType } from '../components/Inventory/OwnedObjects';

export type KioskFnType = (
  item: OwnedObjectType,
  price?: string,
) => Promise<void> | void;

/**
 * A helper to get user's kiosks.
 * If the user doesn't have a kiosk, the return is an object with null values.
 */
export function useOwnedKiosk() {
  const { currentAccount } = useWalletKit();
  const provider = useRpc();

  return useQuery({
    queryKey: [TANSTACK_OWNED_KIOSK_KEY, currentAccount?.address],
    refetchOnMount: false,
    queryFn: async (): Promise<{
      kioskId: SuiAddress | null;
      kioskCap: SuiAddress | null;
    } | null> => {
      if (!currentAccount?.address) return null;
      const ownedKiosks = await provider.getOwnedObjects({
        owner: currentAccount.address,
        filter: { StructType: `${KIOSK_OWNER_CAP}` },
        options: {
          showContent: true,
        },
      });
      // gather a list of owned kiosk Ids, and kioskCaps.
      // we will only use the first one.
      const kioskIdList = ownedKiosks?.data?.map(
        (x) => getObjectFields(x)?.for,
      );
      const kioskOwnerCaps = ownedKiosks?.data.map((x) => getObjectId(x));

      return {
        kioskId: kioskIdList[0],
        kioskCap: kioskOwnerCaps[0],
      };
    },
  });
}

/**
 * A hook to fetch a kiosk (items, listings, etc) by its id.
 */
export function useKiosk(kioskId: string | undefined | null) {
  const provider = useRpc();

  return useQuery({
    queryKey: [TANSTACK_KIOSK_KEY, kioskId],
    queryFn: async (): Promise<{
      kioskData: KioskData | null;
      items: SuiObjectResponse[];
    }> => {
      if (!kioskId) return { kioskData: null, items: [] };
      const { data: res } = await fetchKiosk(
        provider,
        kioskId,
        { limit: 1000 },
        {
          withKioskFields: true,
          withListingPrices: true,
        },
      );

      // get the items from rpc.
      const items = await provider.multiGetObjects({
        ids: res.itemIds,
        options: { showDisplay: true, showType: true },
      });

      return {
        kioskData: res,
        items,
      };
    },
    select: ({
      items,
      kioskData,
    }): {
      items: OwnedObjectType[];
      listings: Record<ObjectId, KioskListing>;
    } => {
      if (!kioskData) return { items: [], listings: {} };
      // parse the displays for FE.
      const displays = parseObjectDisplays(items) || {};

      // attach the displays to the objects.
      const ownedItems = kioskData.items.map((item: KioskItem) => {
        return {
          ...item,
          display: displays[item.objectId] || {},
        };
      });

      // return the items & listings.
      return {
        items: ownedItems,
        listings: processKioskListings(
          kioskData.items.map((x) => x.listing) as KioskListing[],
        ),
      };
    },
  });
}

/**
 * A hook to fetch a kiosk's details.
 */
export function useKioskDetails(kioskId: string | undefined | null) {
  const provider = useRpc();

  return useQuery({
    queryKey: [TANSTACK_KIOSK_DATA_KEY, kioskId],
    queryFn: async (): Promise<Kiosk | null> => {
      if (!kioskId) return null;
      return await getKioskObject(provider, kioskId);
    },
  });
}
