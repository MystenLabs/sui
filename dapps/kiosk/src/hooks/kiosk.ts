// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
/* eslint-disable @tanstack/query/exhaustive-deps */

import { useSuiClient, useSuiClientContext } from '@mysten/dapp-kit';
import {
	getKioskObject,
	Kiosk,
	KioskData,
	KioskItem,
	KioskListing,
	KioskOwnerCap,
} from '@mysten/kiosk';
import { SuiObjectResponse } from '@mysten/sui/client';
import { useQuery } from '@tanstack/react-query';

import { OwnedObjectType } from '../components/Inventory/OwnedObjects';
import { useKioskClient } from '../context/KioskClientContext';
import {
	TANSTACK_KIOSK_DATA_KEY,
	TANSTACK_KIOSK_KEY,
	TANSTACK_OWNED_KIOSK_KEY,
} from '../utils/constants';
import { parseObjectDisplays, processKioskListings } from '../utils/utils';

export type KioskFnType = (item: OwnedObjectType, price?: string) => Promise<void> | void;

/**
 * A helper to get user's kiosks.
 * If the user doesn't have a kiosk, the return is an object with null values.
 */
export function useOwnedKiosk(address: string | undefined) {
	const kioskClient = useKioskClient();
	const { network } = useSuiClientContext();

	return useQuery({
		queryKey: [TANSTACK_OWNED_KIOSK_KEY, address, network],
		refetchOnMount: false,
		retry: false,
		queryFn: async (): Promise<{
			caps: KioskOwnerCap[];
			kioskId: string | undefined;
			kioskCap: KioskOwnerCap;
		} | null> => {
			if (!address) return null;

			const { kioskOwnerCaps, kioskIds } = await kioskClient.getOwnedKiosks({ address });

			return {
				caps: kioskOwnerCaps,
				kioskId: kioskIds[0],
				kioskCap: kioskOwnerCaps[0],
			};
		},
	});
}

/**
 * A hook to fetch a kiosk (items, listings, etc) by its id.
 */
export function useKiosk(kioskId: string | undefined | null) {
	const kioskClient = useKioskClient();
	const { network } = useSuiClientContext();

	return useQuery({
		queryKey: [TANSTACK_KIOSK_KEY, kioskId, network],
		queryFn: async (): Promise<{
			kioskData: KioskData | null;
			items: SuiObjectResponse[];
		}> => {
			if (!kioskId) return { kioskData: null, items: [] };
			const res = await kioskClient.getKiosk({
				id: kioskId,
				options: {
					withKioskFields: true,
					withListingPrices: true,
					withObjects: true,
				},
			});

			return {
				kioskData: res,
				items: res.items,
			};
		},
		retry: false,
		select: ({
			items,
			kioskData,
		}): {
			items: OwnedObjectType[];
			listings: Record<string, KioskListing>;
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
	const client = useSuiClient();
	const { network } = useSuiClientContext();

	return useQuery({
		queryKey: [TANSTACK_KIOSK_DATA_KEY, kioskId, network],
		queryFn: async (): Promise<Kiosk | null> => {
			if (!kioskId) return null;
			return await getKioskObject(client, kioskId);
		},
	});
}
