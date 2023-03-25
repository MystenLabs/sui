// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import { useGetValidatorsEvents } from './useGetValidatorsEvents';
import { roundFloat } from '../utils/roundFloat';

// recentEpochRewards is list of the last 30 epoch rewards for a specific validator
// APY_e = (1 + epoch_rewards / stake)^365-1
// APY_e_30rollingaverage = average(APY_e,APY_e-1,…,APY_e-29);

const ROLLING_AVERAGE = 30;
const DEFAULT_APY_DECIMALS = 4;

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

export function useGetRollingAverageApys(numberOfValidators: number | null) {
    // Set the limit to the number of validators  * the rolling average
    // Order the response in descending order so that the most recent epoch are at the top
    const validatorEpochEvents = useGetValidatorsEvents({
        limit: numberOfValidators ? numberOfValidators * ROLLING_AVERAGE : null,
        order: 'descending',
    });

    const apyCal = useMemo<ApyByValidator | null>(() => {
        if (!validatorEpochEvents?.data || !validatorEpochEvents?.data?.data) {
            return null;
        }
        const apyGroups: ApyGroups = {};
        validatorEpochEvents.data.data.forEach(({ parsedJson }) => {
            const { stake, pool_staking_reward, validator_address } =
                parsedJson as ParsedJson;
            const apy = Math.pow(1 + +pool_staking_reward / +stake, 365) - 1;

            if (!apyGroups[validator_address]) {
                apyGroups[validator_address] = [];
            }
            // If the APY is greater than 10000% or isNAN, set it to 0
            apyGroups[validator_address].push(
                Number.isNaN(apy) || apy > 10_000 ? 0 : apy
            );
        });

        const apyByValidator: ApyByValidator = Object.keys(apyGroups).reduce(
            (acc, validatorAddr) => {
                const apys = apyGroups[validatorAddr]
                    .slice(0, ROLLING_AVERAGE)
                    .map((entry) => entry);

                const avgApy =
                    (apys.reduce((sum, apy) => sum + apy, 0) / apys.length) *
                    100;

                acc[validatorAddr] = roundFloat(avgApy, DEFAULT_APY_DECIMALS);

                return acc;
            },
            {} as ApyByValidator
        );

        return apyByValidator;
    }, [validatorEpochEvents.data]);

    return {
        ...validatorEpochEvents,
        data: apyCal,
    };
}
