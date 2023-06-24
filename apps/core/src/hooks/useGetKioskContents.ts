// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider, SuiAddress, getObjectFields } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { useRpcClient } from '../api/RpcClientContext';

// OriginByte module for mainnet (we only support mainnet)
export const ORIGINBYTE_KIOSK_MODULE =
	'0x95a441d389b07437d00dd07e0b6f05f513d7659b13fd7c5d3923c7d9d847199b::ob_kiosk' as const;
export const ORIGINBYTE_KIOSK_OWNER_TOKEN = `${ORIGINBYTE_KIOSK_MODULE}::OwnerToken`;

const KIOSK_MODULE = '0x2::kiosk';
const KIOSK_OWNER_CAP = `${KIOSK_MODULE}::KioskOwnerCap`;

async function getKioskContents(address: SuiAddress, type: string, rpc: JsonRpcProvider) {
	// fetch owner cap
	const data = await rpc.getOwnedObjects({
		owner: address,
		filter: {
			StructType: type,
		},
		options: {
			showContent: true,
		},
	});

	// find kiosk ids
	const ids = data.data.map(
		(object) => getObjectFields(object)?.kiosk ?? getObjectFields(object)?.for,
	);

	// fetch the user's kiosks
	const ownedKiosks = await rpc.multiGetObjects({
		ids: ids.flat(),
		options: {
			showContent: true,
		},
	});

	// find object IDs within a kiosk
	const kioskObjectIds = await Promise.all(
		ownedKiosks.map(async (kiosk) => {
			if (!kiosk.data?.objectId) return [];
			const objects = await rpc.getDynamicFields({
				parentId: kiosk.data.objectId,
			});
			return objects.data.map((obj) => obj.objectId);
		}),
	);

	// fetch the contents of the objects within a kiosk
	const kioskContent = await rpc.multiGetObjects({
		ids: kioskObjectIds.flat(),
		options: {
			showDisplay: true,
			showType: true,
		},
	});
	return kioskContent;
}

export function useGetKioskContents(address?: SuiAddress | null, disableOriginByteKiosk?: boolean) {
	const rpc = useRpcClient();
	return useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: ['get-kiosk-contents', address, disableOriginByteKiosk],
		queryFn: async () => {
			const obKioskContents = await getKioskContents(address!, ORIGINBYTE_KIOSK_OWNER_TOKEN, rpc);
			const suiKioskContents = await getKioskContents(address!, KIOSK_OWNER_CAP, rpc);

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
