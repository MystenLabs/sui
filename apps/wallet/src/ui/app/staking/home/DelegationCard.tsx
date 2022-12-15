// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Link } from 'react-router-dom';

import { GAS_TYPE_ARG } from '../../redux/slices/sui-objects/Coin';
import { useFormatCoin } from '_app/hooks';
import { ImageIcon } from '_app/shared/image-icon';
import { Text } from '_src/ui/app/shared/text';
import { IconTooltip } from '_src/ui/app/shared/tooltip';

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
    icon?: string | null;
    address: string;
}

const STATE_TO_COPY = {
    [DelegationState.WARM_UP]: 'In Warm-up',
    [DelegationState.EARNING]: 'Staking Reward',
    [DelegationState.COOL_DOWN]: 'In Cool-down',
};

const APY_TOOLTIP = 'Annual Percentage Yield';

// TODO: Add these classes when we add delegation detail page.

export function DelegationCard({
    name,
    staked,
    rewards,
    state,
    icon,
    address,
}: DelegationCardProps) {
    const [stakedFormatted] = useFormatCoin(staked, GAS_TYPE_ARG);
    const [rewardsFormatted] = useFormatCoin(rewards, GAS_TYPE_ARG);

    return (
        <Link
            to={`/stake/validator-details?address=${encodeURIComponent(
                address
            )}`}
            className="flex no-underline flex-col p-3.5 box-border h-36 w-full rounded-2xl border hover:bg-sui/10  border-solid border-gray-45 hover:border-sui/10 bg-transparent"
        >
            <div className="flex justify-between items-center mb-2">
                <ImageIcon src={icon} alt={name} />

                <div className="text-gray-60 text-p1">
                    <IconTooltip tip={`${APY_TOOLTIP}`} placement="top" />
                </div>
            </div>

            <div className="flex-1 capitalize">
                <Text variant="subtitle" weight="semibold" color="gray-90">
                    {name}
                </Text>

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
