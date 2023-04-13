// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';
// NOTE: Bignumber's .pow() method is very slow, so we use decimal.js for this use-case.
// this inflates bundle size quite a bit, so we should move this calculation to the API in the future.
import Decimal from 'decimal.js';

import { useGetValidatorsEvents } from './useGetValidatorsEvents';
import { useGetSystemState } from './useGetSystemState';
import { roundFloat } from '../utils/roundFloat';

// recentEpochRewards is list of the last 30 epoch rewards for a specific validator
// APY_e = (1 + epoch_rewards / stake)^365-1
// APY_e_30rollingaverage = average(APY_e,APY_e-1,…,APY_e-29);

const ROLLING_AVERAGE = 30;
const DEFAULT_APY_DECIMALS = 2;

// define the type parsedJson response
type ParsedJson = {
    commission_rate: string;
    epoch: string;
    pool_staking_reward: string;
    pool_token_exchange_rate: {
        pool_token_amount: string;
        sui_amount: string;
    };
    reference_gas_survey_quote: string;
    stake: string;
    storage_fund_staking_reward: string;
    tallying_rule_global_score: string;
    tallying_rule_reporters: string[];
    validator_address: string;
};

interface ApyGroups {
    [validatorAddress: string]: number[];
}

export interface ApyByValidator {
    [validatorAddress: string]: number;
}

const calculateApy = (stake: string, poolStakingReward: string) => {
    const poolStakingRewardBigNumber = new Decimal(poolStakingReward);
    const stakeBigNumber = new Decimal(stake);
    // Calculate the ratio of pool_staking_reward / stake
    const ratio = poolStakingRewardBigNumber.div(stakeBigNumber);

    // Perform the exponentiation and subtraction using BigNumber
    const apy = ratio.plus(1).pow(365).minus(1);
    return apy.toNumber();
};

export function useGetRollingAverageApys(numberOfValidators: number | null) {
    // Set the limit to the number of validators  * the rolling average
    // Order the response in descending order so that the most recent epoch are at the top
    const validatorEpochEvents = useGetValidatorsEvents({
        limit: numberOfValidators ? numberOfValidators * ROLLING_AVERAGE : null,
        order: 'descending',
    });

    const { data, isLoading } = useGetSystemState();

    const apyByValidator =
        useMemo<ApyByValidator | null>(() => {
            if (!validatorEpochEvents?.data || !data) {
                return null;
            }
            const { stakeSubsidyStartEpoch, epoch, activeValidators } =
                data || {};
            // return 0 for all validators if current epoch is less than the stake subsidy start epoch
            if (Number(epoch) < Number(stakeSubsidyStartEpoch)) {
                return activeValidators.reduce((acc, validator) => {
                    acc[validator.suiAddress] = 0;
                    return acc;
                }, {} as ApyByValidator);
            }

            // The rolling average epoch is the current epoch - the stake subsidy start epoch
            const avgEpochNumberAfterSubsidy = Math.max(
                0,
                Math.min(
                    ROLLING_AVERAGE,
                    Number(epoch) - Number(stakeSubsidyStartEpoch)
                )
            );
            const apyGroups: ApyGroups = {};

            validatorEpochEvents.data.forEach(({ parsedJson }) => {
                const { stake, pool_staking_reward, validator_address } =
                    parsedJson as ParsedJson;

                if (!apyGroups[validator_address]) {
                    apyGroups[validator_address] = [];
                }
                const apyFloat = calculateApy(stake, pool_staking_reward);

                // If the APY is greater than 10000% or isNAN, set it to 0
                apyGroups[validator_address].push(
                    Number.isNaN(apyFloat) || apyFloat > 10_000 ? 0 : apyFloat
                );
            });

            const apyByValidator: ApyByValidator = Object.entries(
                apyGroups
            ).reduce((acc, [validatorAddr, apyArr]) => {
                // prevent negative rolling average epoch by setting it to 0
                const apys = apyArr
                    .slice(0, avgEpochNumberAfterSubsidy)
                    .map((entry) => entry);

                const avgApy =
                    apys.reduce((sum, apy) => sum + apy, 0) / apys.length;
                acc[validatorAddr] = roundFloat(
                    avgApy * 100,
                    DEFAULT_APY_DECIMALS
                );
                return acc;
            }, {} as ApyByValidator);
            // return object with validator address as key and APY as value
            // { '0x123': 0.1234, '0x456': 0.4567 }
            return apyByValidator;
        }, [validatorEpochEvents.data, data]) || null;

    return {
        ...validatorEpochEvents,
        isLoading: isLoading || validatorEpochEvents.isLoading,
        data: apyByValidator,
    };
}
