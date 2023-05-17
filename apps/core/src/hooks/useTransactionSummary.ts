// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    DryRunTransactionBlockResponse,
    getExecutionStatusType,
    getTransactionDigest,
    getTransactionSender,
    is,
    type SuiAddress,
    SuiObjectChangePublished,
    type SuiTransactionBlockResponse,
} from '@mysten/sui.js';
import { useMemo } from 'react';

import {
    getBalanceChangeSummary,
    getGasSummary,
    getGroupByOwner,
    getLabel,
    getObjectChangeSummary,
} from '../utils/transaction';
import { useNFTsMeta } from './useGetNFTMeta';

const getSummary = (
    transaction: DryRunTransactionBlockResponse | SuiTransactionBlockResponse,
    currentAddress?: SuiAddress
) => {
    const objectSummary = getObjectChangeSummary(transaction, currentAddress);
    const balanceChangeSummary = getBalanceChangeSummary(transaction);

    const gas = getGasSummary(transaction);

    if (is(transaction, DryRunTransactionBlockResponse)) {
        return {
            gas,
            objectSummary,
            balanceChanges: balanceChangeSummary,
        };
    } else {
        return {
            gas,
            sender: getTransactionSender(transaction),
            balanceChanges: balanceChangeSummary,
            digest: getTransactionDigest(transaction),
            label: getLabel(transaction, currentAddress),
            objectSummary,
            status: getExecutionStatusType(transaction),
            timestamp: transaction.timestampMs,
        };
    }
};

const getObjectSummaryNFTData = <T extends { objectId: string }>({
    nftsMeta,
    originalData,
}: {
    nftsMeta: ReturnType<typeof useNFTsMeta>;
    originalData: T[];
}) => {
    const objectIdsFromNFTData = nftsMeta.data.objectIds;
    const objectIdsFromNFTDataSet = new Set(objectIdsFromNFTData);

    return originalData.reduce((acc, obj) => {
        if (objectIdsFromNFTDataSet.has(obj.objectId)) {
            const index = nftsMeta.data.objectIds.indexOf(obj.objectId);

            return [
                ...acc,
                {
                    ...obj,
                    nftMeta: nftsMeta.data.metaData[index],
                },
            ];
        }
        return acc;
    }, [] as (T & { nftMeta: Record<string, string | null> })[]);
};

export function useTransactionSummaryWithNFTs({
    transaction,
    currentAddress,
}: {
    transaction: SuiTransactionBlockResponse | DryRunTransactionBlockResponse;
    currentAddress?: SuiAddress;
}) {
    const summary = useMemo(() => {
        if (!transaction) {
            return null;
        }
        return getSummary(transaction, currentAddress);
    }, [transaction, currentAddress]);

    const { objectSummary, balanceChanges } = summary || {};

    const { created, mutated, transferred } = objectSummary || {};

    const createdObjectIds = created?.map((obj) => obj.objectId) || [];
    const mutatedObjectIds = mutated?.map((obj) => obj.objectId) || [];

    const createdNFTData = useNFTsMeta(createdObjectIds);
    const mutatedNFTData = useNFTsMeta(mutatedObjectIds);

    return useMemo(() => {
        const filteredObjectSummary = {
            ...(summary?.objectSummary || {}),
            published: [] as SuiObjectChangePublished[],
            mutated: getGroupByOwner(mutated || []),
            created: getGroupByOwner(created || []),
            transferred: getGroupByOwner(transferred || []),
        };

        const respObject = {
            ...summary,
            balanceChanges,
            objectSummary: filteredObjectSummary,
            objectSummaryNFTData: {},
            isLoading: createdNFTData.isLoading || mutatedNFTData.isLoading,
        };

        if (createdNFTData.isLoading || mutatedNFTData.isLoading) {
            return respObject;
        }

        if (
            !summary ||
            createdNFTData.status !== 'success' ||
            mutatedNFTData.status !== 'success'
        ) {
            return null;
        }

        if (createdNFTData.error || mutatedNFTData.error) {
            return respObject;
        }

        const objectSummaryNFTData = {
            created: {},
            mutated: {},
        };

        if (created) {
            const objectIdsFromNFTData = createdNFTData.data.objectIds;
            const excludedObjectIdsSet = new Set(objectIdsFromNFTData);
            filteredObjectSummary.created = getGroupByOwner(
                created.filter((obj) => !excludedObjectIdsSet.has(obj.objectId))
            );

            objectSummaryNFTData.created = getGroupByOwner(
                getObjectSummaryNFTData({
                    nftsMeta: createdNFTData,
                    originalData: created,
                })
            );
        }

        if (mutated) {
            const objectIdsFromNFTData = mutatedNFTData.data.objectIds;
            const excludedObjectIdsSet = new Set(objectIdsFromNFTData);
            filteredObjectSummary.mutated = getGroupByOwner(
                mutated.filter((obj) => !excludedObjectIdsSet.has(obj.objectId))
            );

            objectSummaryNFTData.mutated = getGroupByOwner(
                getObjectSummaryNFTData({
                    nftsMeta: mutatedNFTData,
                    originalData: mutated,
                })
            );
        }

        return {
            ...respObject,
            objectSummary: filteredObjectSummary,
            objectSummaryNFTData,
        };
    }, [
        balanceChanges,
        created,
        createdNFTData,
        mutated,
        mutatedNFTData,
        summary,
        transferred,
    ]);
}

export function useTransactionSummary({
    transaction,
    currentAddress,
}: {
    transaction?: SuiTransactionBlockResponse | DryRunTransactionBlockResponse;
    currentAddress?: SuiAddress;
}) {
    const summary = useMemo(() => {
        if (!transaction) return null;
        return getSummary(transaction, currentAddress);
    }, [transaction, currentAddress]);

    return summary;
}
