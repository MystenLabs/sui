// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { useQuery } from '@tanstack/react-query';

import { roundFloat } from '../utils/roundFloat';

// recentEpochRewards is list of the last 30 epoch rewards for a specific validator
// APY_e = (1 + epoch_rewards / stake)^365-1
// APY_e_30rollingaverage = average(APY_e,APY_e-1,…,APY_e-29);

const DEFAULT_APY_DECIMALS = 2;

export interface ApyByValidator {
    [validatorAddress: string]: number;
}

export function useGetValidatorsApy() {
    const rpc = useRpcClient();
    return useQuery(
        ['get-rolling-average-apys'],
        () => rpc.getValidatorsApy(),
        {
            select: (data) => {
                return data.apys.reduce((acc, { apy, address }) => {
                    acc[address] = roundFloat(apy * 100, DEFAULT_APY_DECIMALS);
                    return acc;
                }, {} as ApyByValidator);
            },
        }
    );
}
