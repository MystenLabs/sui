// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { KIOSK_ITEM, KioskClient, KioskItem, KioskOwnerCap, Network } from '@mysten/kiosk';
import { useQuery } from '@tanstack/react-query';
import { ORIGINBYTE_KIOSK_OWNER_TOKEN, getKioskIdFromOwnerCap } from '../utils/kiosk';
import { SuiClient } from '@mysten/sui.js/src/client';
import { useSuiClient } from '@mysten/dapp-kit';

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

// TODO: Replace `getSuiKioskContent` references to also pass in the network.
async function getSuiKioskContents(
	address: string,
	client: SuiClient,
	network: Network = Network.MAINNET,
) {
	const kioskClient = new KioskClient({ client, network });

	const ownedKiosks = await kioskClient.getOwnedKiosks(address);

	const contents = await Promise.all(
		ownedKiosks.kioskIds.map(async (id: string) => {
			const kiosk = await kioskClient.getKiosk(id, {
				withObjects: true,
				objectOptions: { showDisplay: true, showContent: true },
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

export function useGetKioskContents(
	address?: string | null,
	network?: Network,
	disableOriginByteKiosk?: boolean,
) {
	const client = useSuiClient();
	return useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: ['get-kiosk-contents', address, network, disableOriginByteKiosk],
		queryFn: async () => {
			const suiKiosks = await getSuiKioskContents(address!, client, network);
			const obKiosks = await getOriginByteKioskContents(address!, client);
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
