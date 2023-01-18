// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Link } from 'react-router-dom';

import { GAS_TYPE_ARG } from '../../redux/slices/sui-objects/Coin';
import { ValidatorLogo } from '../validators/ValidatorLogo';
import { useFormatCoin } from '_app/hooks';
import { Text } from '_src/ui/app/shared/text';
import { IconTooltip } from '_src/ui/app/shared/tooltip';

export enum DelegationState {
    WARM_UP = 'WARM_UP',
    EARNING = 'EARNING',
    COOL_DOWN = 'COOL_DOWN',
}

interface DelegationCardProps {
    staked: number | bigint;
    state: DelegationState;
    rewards?: number | bigint;
    address: string;
    stakedId: string;
}

export const STATE_TO_COPY = {
    [DelegationState.WARM_UP]: 'Starts Earning',
    [DelegationState.EARNING]: 'Staking Reward',
    [DelegationState.COOL_DOWN]: 'In Cool-down',
};

// TODO: Add these classes when we add delegation detail page.

export function DelegationCard({
    staked,
    rewards,
    state,
    address,
    stakedId,
}: DelegationCardProps) {
    const [stakedFormatted] = useFormatCoin(staked, GAS_TYPE_ARG);
    const [rewardsFormatted] = useFormatCoin(rewards, GAS_TYPE_ARG);

    return (
        <Link
            to={`/stake/delegation-detail?${new URLSearchParams({
                validator: address,
                staked: stakedId,
            }).toString()}`}
            className="flex no-underline flex-col py-3 px-3.75 box-border h-36 w-full rounded-2xl border hover:bg-sui/10 group border-solid border-gray-45 hover:border-sui/10 bg-transparent"
        >
            <div className="flex justify-between items-start mb-2">
                <ValidatorLogo
                    validatorAddress={address}
                    size="subtitle"
                    iconSize="md"
                    stacked
                />

                <div className="text-gray-60 text-p1 opacity-0 group-hover:opacity-100">
                    <IconTooltip
                        tip="Annual Percentage Yield"
                        placement="top"
                    />
                </div>
            </div>

            <div className="flex-1">
                <div className="flex items-baseline gap-1 mt-1">
                    <Text variant="body" weight="semibold" color="gray-90">
                        {stakedFormatted}
                    </Text>

                    <Text variant="subtitle" weight="normal" color="gray-90">
                        SUI
                    </Text>
                </div>
            </div>
            <div>
                <Text variant="subtitle" weight="medium" color="steel-dark">
                    {STATE_TO_COPY[state]}
                </Text>
                {!!rewards && (
                    <div className="mt-1 text-success-dark text-bodySmall font-semibold">
                        {rewardsFormatted} SUI
                    </div>
                )}
            </div>
        </Link>
    );
}
