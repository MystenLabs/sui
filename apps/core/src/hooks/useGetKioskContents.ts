// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider, SuiAddress, SuiObjectResponse } from '@mysten/sui.js';
import { KioskData, KioskItem, fetchKiosk, getOwnedKiosks } from '@mysten/kiosk';
import { useQuery } from '@tanstack/react-query';
import { useRpcClient } from '../api/RpcClientContext';
import { ORIGINBYTE_KIOSK_OWNER_TOKEN, getKioskIdFromDynamicFields } from '../utils/kiosk';

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
	kioskId: SuiAddress;
	type: KioskTypes;
	ownerCap?: string;
};

async function getOriginByteKioskContents(address: SuiAddress, rpc: JsonRpcProvider) {
	const data = await rpc.getOwnedObjects({
		owner: address,
		filter: {
			StructType: ORIGINBYTE_KIOSK_OWNER_TOKEN,
		},
		options: {
			showContent: true,
		},
	});

	const ids = data.data.map((object) => getKioskIdFromDynamicFields(object));
	const kiosks = new Map<string, Kiosk>();

	// fetch the user's kiosks
	const ownedKiosks = await rpc.multiGetObjects({
		ids: ids.flat(),
		options: {
			showContent: true,
		},
	});

	// find object IDs within a kiosk
	await Promise.all(
		ownedKiosks.map(async (kiosk) => {
			if (!kiosk.data?.objectId) return [];
			const objects = await rpc.getDynamicFields({
				parentId: kiosk.data.objectId,
			});

			const objectIds = objects.data.map((obj) => obj.objectId);

			// fetch the contents of the objects within a kiosk
			const kioskContent = await rpc.multiGetObjects({
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

async function getSuiKioskContents(address: SuiAddress, rpc: JsonRpcProvider) {
	const ownedKiosks = await getOwnedKiosks(rpc, address!);
	const kiosks = new Map<string, Kiosk>();

	await Promise.all(
		ownedKiosks.kioskIds.map(async (id) => {
			const kiosk = await fetchKiosk(rpc, id, { limit: 1000 }, {});
			const contents = await rpc.multiGetObjects({
				ids: kiosk.data.itemIds,
				options: { showDisplay: true, showContent: true, showOwner: true },
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

export function useGetKioskContents(address?: SuiAddress | null, disableOriginByteKiosk?: boolean) {
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
