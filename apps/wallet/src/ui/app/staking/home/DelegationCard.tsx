// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '../../hooks';
import { GAS_TYPE_ARG } from '../../redux/slices/sui-objects/Coin';

export enum DelegationState {
    WARM_UP = 'WARM_UP',
    EARNING = 'EARNING',
    COOL_DOWN = 'COOL_DOWN',
}

interface DelegationCardProps {
    name: string;
    staked: number | bigint;
    state: DelegationState;
    rewards?: number | bigint;
}

const STATE_TO_COPY = {
    [DelegationState.WARM_UP]: 'In Warm-up',
    [DelegationState.EARNING]: 'Staking Reward',
    [DelegationState.COOL_DOWN]: 'In Cool-down',
};

// TODO: Add these classes when we add delegation detail page.
// cursor-pointer hover:bg-sui/10 hover:border-sui/30

export function DelegationCard({
    name,
    staked,
    rewards,
    state,
}: DelegationCardProps) {
    const [stakedFormatted] = useFormatCoin(staked, GAS_TYPE_ARG);
    const [rewardsFormatted] = useFormatCoin(rewards, GAS_TYPE_ARG);

    return (
        <div className="flex flex-col p-4 box-border h-36 w-full rounded-2xl border border-solid border-gray-45 bg-transparent">
            <div className="flex-1 text-gray-90">
                <div className="text-subtitle font-semibold">{name}</div>
                <div className="flex items-baseline gap-1 mt-1">
                    <div className="text-body font-semibold">
                        {stakedFormatted}
                    </div>
                    <div className="text-subtitle font-normal">SUI</div>
                </div>
            </div>
            <div>
                <div className="text-subtitle font-medium text-steel-dark">
                    {STATE_TO_COPY[state]}
                </div>
                {!!rewards && (
                    <div className="mt-1 text-success-dark text-bodySmall font-semibold">
                        {rewardsFormatted} SUI
                    </div>
                )}
            </div>
        </div>
    );
}
