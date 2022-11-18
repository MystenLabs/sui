// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useSigner, useIndividualCoinMaxBalance } from '_hooks';

import type { ObjectId } from '@mysten/sui.js';

export function useGasEstimation(objectId: ObjectId | null) {
    const suiCoinMaxBalance = useIndividualCoinMaxBalance(SUI_TYPE_ARG);
    const signer = useSigner();
    const estimationResult = useQuery({
        queryKey: [
            'gas-estimation',
            'nft-transfer',
            objectId,
            suiCoinMaxBalance.toString(),
        ],
        queryFn: async () => {
            const address = await signer.getAddress();
            return await signer.getGasCostEstimationAndSuggestedBudget(
                'transferObject',
                async (gasBudget) => [
                    await signer.serializer.serializeToBytes(address, {
                        kind: 'transferObject',
                        data: {
                            // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
                            objectId: objectId!,
                            recipient: address, // gas cost is the same regardless the recipient
                            gasBudget,
                        },
                    }),
                ],
                suiCoinMaxBalance
            );
        },
        enabled: !!objectId,
    });
    return [
        estimationResult.data?.suggestedGasBudget ?? null,
        estimationResult.data?.gasCostEstimation ?? null,
        estimationResult.isLoading,
    ] as const;
}
