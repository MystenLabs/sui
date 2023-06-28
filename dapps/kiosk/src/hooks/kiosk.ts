// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
/* eslint-disable @tanstack/query/exhaustive-deps */

import { useQuery } from '@tanstack/react-query';
import {
	TANSTACK_KIOSK_DATA_KEY,
	TANSTACK_KIOSK_KEY,
	TANSTACK_OWNED_KIOSK_KEY,
} from '../utils/constants';
import { useRpc } from '../context/RpcClientContext';
import { ObjectId, SuiAddress, SuiObjectResponse } from '@mysten/sui.js';
import {
	Kiosk,
	KioskData,
	KioskItem,
	KioskListing,
	KioskOwnerCap,
	fetchKiosk,
	getKioskObject,
	getOwnedKiosks,
} from '@mysten/kiosk';
import { parseObjectDisplays, processKioskListings } from '../utils/utils';
import { OwnedObjectType } from '../components/Inventory/OwnedObjects';

export type KioskFnType = (item: OwnedObjectType, price?: string) => Promise<void> | void;

/**
 * A helper to get user's kiosks.
 * If the user doesn't have a kiosk, the return is an object with null values.
 */
export function useOwnedKiosk(address: SuiAddress | undefined) {
	const provider = useRpc();

	return useQuery({
		queryKey: [TANSTACK_OWNED_KIOSK_KEY, address],
		refetchOnMount: false,
		retry: false,
		queryFn: async (): Promise<{
			caps: KioskOwnerCap[];
			kioskId: SuiAddress | undefined;
			kioskCap: SuiAddress | undefined;
		} | null> => {
			if (!address) return null;

			const { kioskOwnerCaps, kioskIds } = await getOwnedKiosks(provider, address);

			return {
				caps: kioskOwnerCaps,
				kioskId: kioskIds[0],
				kioskCap: kioskOwnerCaps[0]?.objectId,
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
		retry: false,
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
				listings: processKioskListings(kioskData.items.map((x) => x.listing) as KioskListing[]),
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
