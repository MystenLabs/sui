// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiObjectResponse } from '@mysten/sui.js';
import { KIOSK_ITEM, KioskData, KioskItem, fetchKiosk, getOwnedKiosks } from '@mysten/kiosk';
import { useQuery } from '@tanstack/react-query';
import { useRpcClient } from '../api/RpcClientContext';
import { ORIGINBYTE_KIOSK_OWNER_TOKEN, getKioskIdFromOwnerCap } from '../utils/kiosk';
import { SuiClient } from '@mysten/sui.js/src/client';

export type KioskContents = Omit<KioskData, 'items'> & {
	items: Partial<KioskItem & SuiObjectResponse>[];
	ownerCap?: string;
};

export enum KioskTypes {
	SUI = 'sui',
	ORIGINBYTE = 'originByte',
}

export type Kiosk = {
	items: Partial<KioskItem & SuiObjectResponse>[];
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
	const kiosks = new Map<string, Kiosk>();

	// fetch the user's kiosks
	const ownedKiosks = await client.multiGetObjects({
		ids: ids.flat(),
		options: {
			showContent: true,
		},
	});

	// find object IDs within a kiosk
	await Promise.all(
		ownedKiosks.map(async (kiosk) => {
			if (!kiosk.data?.objectId) return [];
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

			kiosks.set(kiosk.data.objectId, {
				items: kioskContent.map((item) => ({ ...item, kioskId: kiosk.data?.objectId })),
				kioskId: kiosk.data.objectId,
				type: KioskTypes.ORIGINBYTE,
			});
		}),
	);

	return kiosks;
}

async function getSuiKioskContents(address: string, client: SuiClient) {
	const ownedKiosks = await getOwnedKiosks(client, address!);
	const kiosks = new Map<string, Kiosk>();

	await Promise.all(
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

			kiosks.set(id, {
				...kiosk.data,
				items,
				kioskId: id,
				type: KioskTypes.SUI,
				ownerCap: ownedKiosks.kioskOwnerCaps.find((k) => k.kioskId === id)?.objectId,
			});
		}, kiosks),
	);

	return kiosks;
}

export function useGetKioskContents(address?: string | null, disableOriginByteKiosk?: boolean) {
	const rpc = useRpcClient();
	return useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: ['get-kiosk-contents', address, disableOriginByteKiosk],
		queryFn: async () => {
			const suiKiosks = await getSuiKioskContents(address!, rpc);
			const obKiosks = !disableOriginByteKiosk
				? await getOriginByteKioskContents(address!, rpc)
				: new Map();

			const list = [...Array.from(suiKiosks.values()), ...Array.from(obKiosks.values())].flatMap(
				(d) => d.items,
			);
			const kiosks = new Map([...suiKiosks, ...obKiosks]) as Map<string, Kiosk>;
			// a map of object ID to Kiosk ID
			const lookup = list.reduce((acc, curr) => {
				acc.set(curr.data.objectId, curr.kioskId);
				return acc;
			}, new Map<string, string>());

			return {
				list,
				lookup,
				kiosks,
			};
		},
	});
}
