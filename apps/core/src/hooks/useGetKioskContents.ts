// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiObjectResponse } from '@mysten/sui.js';
import { fetchKiosk, getOwnedKiosks } from '@mysten/kiosk';
import { useQuery } from '@tanstack/react-query';
import { useRpcClient } from '../api/RpcClientContext';
import { SuiClient } from '@mysten/sui.js/client';

const getKioskId = (obj: SuiObjectResponse) =>
	obj.data?.content &&
	'fields' in obj.data.content &&
	(obj.data.content.fields.for ?? obj.data.content.fields.kiosk);

// OriginByte module for mainnet (we only support mainnet)
export const ORIGINBYTE_KIOSK_MODULE =
	'0x95a441d389b07437d00dd07e0b6f05f513d7659b13fd7c5d3923c7d9d847199b::ob_kiosk' as const;
export const ORIGINBYTE_KIOSK_OWNER_TOKEN = `${ORIGINBYTE_KIOSK_MODULE}::OwnerToken`;

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
	const ids = data.data.map((object) => getKioskId(object) ?? []);

	// fetch the user's kiosks
	const ownedKiosks = await client.multiGetObjects({
		ids: ids.flat(),
		options: {
			showContent: true,
		},
	});

	// find object IDs within a kiosk
	const kioskObjectIds = await Promise.all(
		ownedKiosks.map(async (kiosk) => {
			if (!kiosk.data?.objectId) return [];
			const objects = await client.getDynamicFields({
				parentId: kiosk.data.objectId,
			});
			return objects.data.map((obj) => obj.objectId);
		}),
	);

	// fetch the contents of the objects within a kiosk
	const kioskContent = await client.multiGetObjects({
		ids: kioskObjectIds.flat(),
		options: {
			showDisplay: true,
			showType: true,
		},
	});

	return kioskContent;
}

async function getSuiKioskContents(address: string, client: SuiClient) {
	const ownedKiosks = await getOwnedKiosks(client, address!);
	const kioskContents = await Promise.all(
		ownedKiosks.kioskIds.map(async (id) => {
			return fetchKiosk(client, id, { limit: 1000 }, {});
		}),
	);
	const items = kioskContents.flatMap((k) => k.data.items);
	const ids = items.map((item) => item.objectId);

	// fetch the contents of the objects within a kiosk
	const kioskContent = await client.multiGetObjects({
		ids,
		options: {
			showContent: true,
			showDisplay: true,
			showType: true,
		},
	});

	return kioskContent;
}

export function useGetKioskContents(address?: string | null, disableOriginByteKiosk?: boolean) {
	const rpc = useRpcClient();
	return useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: ['get-kiosk-contents', address, disableOriginByteKiosk],
		queryFn: async () => {
			const obKioskContents = await getOriginByteKioskContents(address!, rpc);
			const suiKioskContents = await getSuiKioskContents(address!, rpc);

			return {
				list: [...suiKioskContents, ...obKioskContents],
				kiosks: {
					sui: suiKioskContents ?? [],
					originByte: obKioskContents ?? [],
				},
			};
		},
	});
}
