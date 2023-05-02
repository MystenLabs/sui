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
    [validatorAddress: string]: {
        apy: number;
        isApyApproxZero: boolean;
    };
}
// For small APYs or AYPs before stakeSubsidyStartEpoch, show ~0% instead of 0%
// If APY falls below 0.001, show ~0% instead of 0% since we round to 2 decimal places
const MINIMUM_THRESHOLD = 0.001;

export function useGetValidatorsApy() {
    const rpc = useRpcClient();
    return useQuery(
        ['get-rolling-average-apys'],
        async () => {
            const [apy, systemStateResponse] = await Promise.all([
                rpc.getValidatorsApy(),
                //TODO: remove the stakeSubsidyStartEpoch check once its past that epoch
                rpc.getLatestSuiSystemState(),
            ]);

            // check if stakeSubsidyStartEpoch is greater than current epoch, flag for UI to show ~0% instead of 0%
            const currentEpoch = Number(systemStateResponse?.epoch);
            const stakeSubsidyStartEpoch = Number(
                systemStateResponse?.stakeSubsidyStartEpoch
            );
            return {
                validatorApys: apy,
                isStakeSubsidyStarted: currentEpoch > stakeSubsidyStartEpoch,
            };
        },
        {
            select: ({ validatorApys, isStakeSubsidyStarted }) => {
                return validatorApys?.apys.reduce((acc, { apy, address }) => {
                    acc[address] = {
                        apy: roundFloat(apy * 100, DEFAULT_APY_DECIMALS),
                        isApyApproxZero:
                            !isStakeSubsidyStarted || apy < MINIMUM_THRESHOLD,
                    };
                    return acc;
                }, {} as ApyByValidator);
            },
        }
    );
}
