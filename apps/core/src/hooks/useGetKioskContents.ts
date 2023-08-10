// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { KIOSK_ITEM, KioskItem, fetchKiosk, getOwnedKiosks } from '@mysten/kiosk';
import { useQuery } from '@tanstack/react-query';
import { useRpcClient } from '../api/RpcClientContext';
import { ORIGINBYTE_KIOSK_OWNER_TOKEN, getKioskIdFromOwnerCap } from '../utils/kiosk';
import { SuiClient, SuiObjectResponse } from '@mysten/sui.js/src/client';

export enum KioskTypes {
	SUI = 'sui',
	ORIGINBYTE = 'originByte',
}

export type Kiosk = {
	items: Partial<SuiObjectResponse & KioskItem>[];
	itemIds: string[];
	kioskId: string;
	type: KioskTypes;
	ownerCap?: string;
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

async function getSuiKioskContents(address: string, client: SuiClient) {
	const ownedKiosks = await getOwnedKiosks(client, address!);

	const contents = await Promise.all(
		ownedKiosks.kioskIds.map(async (id) => {
			const kiosk = await fetchKiosk(client, id, { limit: 1000 }, {});
			const contents = await client.multiGetObjects({
				ids: kiosk.data.itemIds,
				options: { showDisplay: true, showContent: true },
			});
			const items = contents.map((object) => {
				const kioskData = kiosk.data.items.find((item) => item.objectId === object.data?.objectId);
				return { ...object, ...kioskData, kioskId: id };
			});
			return {
				itemIds: kiosk.data.itemIds,
				items,
				kioskId: id,
				type: KioskTypes.SUI,
				ownerCap: ownedKiosks.kioskOwnerCaps.find((k) => k.kioskId === id)?.objectId,
			};
		}),
	);
	return contents;
}

export function useGetKioskContents(address?: string | null, disableOriginByteKiosk?: boolean) {
	const rpc = useRpcClient();
	return useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: ['get-kiosk-contents', address, disableOriginByteKiosk],
		queryFn: async () => {
			const suiKiosks = await getSuiKioskContents(address!, rpc);
			const obKiosks = await getOriginByteKioskContents(address!, rpc);
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
	});
}
