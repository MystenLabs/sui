// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

import type { SuiAddress, SuiObjectResponseQuery } from '@mysten/sui.js';

const KIOSK_TYPE = '0x2::kiosk::Kiosk';
const KIOSK_OWNER_TOKEN_TYPE_SUFFIX = '::ob_kiosk::OwnerToken';

export function useObjectsOwnedByAddress(
    address?: SuiAddress | null,
    query?: SuiObjectResponseQuery
) {
    const rpc = useRpcClient();
    return useQuery(
        ['objects-owned', address, query],
        async () => {
            const options: SuiObjectResponseQuery['options'] = {
                ...(query?.options ?? {}),
                showType: true,
                showContent: true,
            };
            const ownObjects = await rpc.getOwnedObjects({
                owner: address!,
                filter: query?.filter,
                options,
            });

            const kiosks = ownObjects.data.filter(
                (obj) => obj.data?.type === KIOSK_TYPE
            );

            const kioskOwnerTokens = ownObjects.data.filter(
                (obj) =>
                    obj.data?.type?.endsWith(KIOSK_OWNER_TOKEN_TYPE_SUFFIX) &&
                    obj.data.objectId !==
                        '0xf2ec762d1616d606af560286c158ee2bd0dd5cc00e5fa0f7bedcdc74c9689155'
            );

            if (kioskOwnerTokens.length) {
                const kioskIds = kioskOwnerTokens
                    .map((kioskOwnerToken) => {
                        if (
                            kioskOwnerToken.data?.content &&
                            'fields' in kioskOwnerToken.data.content
                        ) {
                            return kioskOwnerToken.data?.content.fields.kiosk;
                        }
                        return undefined;
                    })
                    .filter((id): id is string => !!id);

                const ownedKiosks = await rpc.multiGetObjects({
                    ids: kioskIds,
                    options: {
                        showContent: true,
                    },
                });

                kiosks.push(...ownedKiosks);
            }

            const kioskItemIds = await Promise.all(
                kiosks.map(async (kiosk) => {
                    if (!kiosk.data?.objectId) return [];
                    const objects = await rpc.getDynamicFields({
                        parentId: kiosk.data.objectId,
                    });

                    return objects.data.map((obj) => obj.objectId);
                })
            );

            const kioskContent = await rpc.multiGetObjects({
                ids: kioskItemIds.flat(),
                options,
            });

            return [...ownObjects.data, ...kioskContent];
        },
        {
            enabled: !!address,
        }
    );
}
