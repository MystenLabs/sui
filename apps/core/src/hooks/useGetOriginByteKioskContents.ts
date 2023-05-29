// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { useRpcClient } from '../api/RpcClientContext';
import { useGetOwnedObjects } from './useGetOwnedObjects';

// OriginByte contract address for mainnet (we only support mainnet)
export const ORIGINBYTE_KIOSK_MODULE =
    '0x95a441d389b07437d00dd07e0b6f05f513d7659b13fd7c5d3923c7d9d847199b::ob_kiosk';
export const ORIGINBYTE_KIOSK_OWNER_TOKEN = `${ORIGINBYTE_KIOSK_MODULE}::OwnerToken`;

export function useGetOriginByteKioskContents(
    address?: SuiAddress | null,
    disable?: boolean
) {
    const rpc = useRpcClient();
    const { data } = useGetOwnedObjects(address, {
        StructType: ORIGINBYTE_KIOSK_OWNER_TOKEN,
    });

    // find list of kiosk IDs owned by address
    const obKioskIds =
        data?.pages
            .flatMap((page) => page.data)
            .map(
                (obj) =>
                    obj.data?.content &&
                    'fields' in obj.data.content &&
                    obj.data.content.fields.kiosk
            ) ?? [];

    return useQuery({
        queryKey: ['originbyte-kiosk-contents', address, obKioskIds],
        queryFn: async () => {
            if (!obKioskIds.length) return [];

            // fetch the user's kiosks
            const ownedKiosks = await rpc.multiGetObjects({
                ids: obKioskIds,
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
                })
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
        },
        enabled: !disable && !!address,
    });
}
