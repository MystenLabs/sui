// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type SuiMoveObject,
    type SuiObject as SuiObjectType,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useRpc } from './useRpc';
import { useAppSelector } from '_hooks';
import { accountNftsSelector } from '_redux/slices/account';

type SafeNft = {
    type: string;
    fields: {
        key: string;
        value: {
            fields: {
                is_exclusively_listed: boolean;
                is_generic: boolean;
                transfer_cap_counter: string;
                version: string;
            };
        };
    };
};

export function useOriginByteSafes() {
    const nfts = useAppSelector(accountNftsSelector);
    const rpc = useRpc();
    const safeOwnerCaps = nfts.filter((object) =>
        (object.data as SuiMoveObject).type.endsWith('::safe::OwnerCap')
    );

    const ownerCapIds = safeOwnerCaps.map((safe) => safe.reference.objectId);

    return useQuery(
        ['originbyte-safes', ownerCapIds],
        async () => {
            const safeIds: string[] = safeOwnerCaps.map(
                (orwnerCap) => (orwnerCap.data as SuiMoveObject).fields.safe
            );
            if (!safeIds.length) return [];

            const safes = await rpc.getObjectBatch(safeIds);
            const nftIds = safes
                .map((safe) => safe.details as SuiObjectType)
                .map((safeDetails) => safeDetails.data as SuiMoveObject)
                .map(
                    (safeData) =>
                        safeData.fields.inner.fields.refs.fields
                            .contents as SafeNft[]
                )
                .flatMap((safeNfts) =>
                    safeNfts.map((safeNft) => safeNft.fields.key)
                )
                .filter((nftId) => !!nftId);
            const objects = await rpc.getObjectBatch(nftIds);
            const result = objects.map(
                (object) => object.details as SuiObjectType
            );

            result.forEach(
                (obj) =>
                    ((obj.data as SuiMoveObject).has_public_transfer = false)
            );

            return result;
        },
        {
            enabled: !!ownerCapIds.length,
        }
    );
}

export const useSafeNft = (nftId: string | null) => {
    const { data: safeObjects } = useOriginByteSafes();

    const data =
        safeObjects?.find(
            (safeObject) => safeObject.reference.objectId === nftId
        ) ?? null;
    return { data, loading: !safeObjects };
};
