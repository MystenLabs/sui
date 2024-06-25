// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClientContext } from '@mysten/dapp-kit';
import { KIOSK_ITEM, KioskClient, KioskItem, KioskOwnerCap } from '@mysten/kiosk';
import { SuiClient } from '@mysten/sui/client';
import { useQuery } from '@tanstack/react-query';

import { getKioskIdFromOwnerCap, ORIGINBYTE_KIOSK_OWNER_TOKEN } from '../utils/kiosk';
import { useKioskClient } from './useKioskClient';

export enum KioskTypes {
	SUI = 'sui',
	ORIGINBYTE = 'originByte',
}

export type Kiosk = {
	items: KioskItem[];
	itemIds: string[];
	kioskId: string;
	type: KioskTypes;
	ownerCap?: KioskOwnerCap;
};

async function getOriginByteKioskContents(address: string, client: SuiClient) {
	const data = await client.getOwnedObjects({
		owner: address,
		filter: {
			StructType: ORIGINBYTE_KIOSK_OWNER_TOKEN,
		},
		options: {
			showContent: true,
		},
	});
	const ids = data.data.map((object) => getKioskIdFromOwnerCap(object));

	// fetch the user's kiosks
	const ownedKiosks = await client.multiGetObjects({
		ids: ids.flat(),
		options: {
			showContent: true,
		},
	});

	const contents = await Promise.all(
		ownedKiosks
			.map(async (kiosk) => {
				if (!kiosk.data) return;
				const objects = await client.getDynamicFields({
					parentId: kiosk.data.objectId,
				});

				const objectIds = objects.data
					.filter((obj) => obj.name.type === KIOSK_ITEM)
					.map((obj) => obj.objectId);

				// fetch the contents of the objects within a kiosk
				const kioskContent = await client.multiGetObjects({
					ids: objectIds,
					options: {
						showDisplay: true,
						showType: true,
					},
				});

				return {
					itemIds: objectIds,
					items: kioskContent.map((item) => ({ ...item, kioskId: kiosk.data?.objectId })),
					kioskId: kiosk.data.objectId,
					type: KioskTypes.ORIGINBYTE,
				};
			})
			.filter(Boolean) as Promise<Kiosk>[],
	);
	return contents;
}

async function getSuiKioskContents(address: string, kioskClient: KioskClient) {
	const ownedKiosks = await kioskClient.getOwnedKiosks({ address });
	const contents = await Promise.all(
		ownedKiosks.kioskIds.map(async (id: string) => {
			const kiosk = await kioskClient.getKiosk({
				id,
				options: {
					withObjects: true,
					objectOptions: { showDisplay: true, showContent: true },
				},
			});
			return {
				itemIds: kiosk.itemIds,
				items: kiosk.items,
				kioskId: id,
				type: KioskTypes.SUI,
				ownerCap: ownedKiosks.kioskOwnerCaps.find((k) => k.kioskId === id),
			};
		}),
	);
	return contents;
}

export function useGetKioskContents(address?: string | null, disableOriginByteKiosk?: boolean) {
	const { client: suiClient, network } = useSuiClientContext();
	const kioskClient = useKioskClient();
	return useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: ['get-kiosk-contents', address, disableOriginByteKiosk, network, kioskClient.network],
		queryFn: async () => {
			const suiKiosks = await getSuiKioskContents(address!, kioskClient);
			const obKiosks = await getOriginByteKioskContents(address!, suiClient);
			return [...suiKiosks, ...obKiosks];
		},
		select(data) {
			const kiosks = new Map<string, Kiosk>();
			const lookup = new Map<string, string>();

			data.forEach((kiosk) => {
				kiosks.set(kiosk.kioskId, kiosk);
				kiosk.itemIds.forEach((id) => {
					lookup.set(id, kiosk.kioskId);
				});
			});

			return {
				kiosks,
				list: data.flatMap((kiosk) => kiosk.items),
				lookup,
			};
		},
		enabled: !!address,
	});
}
