// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { SUI_TYPE_ARG, type SuiAddress } from '@mysten/sui.js';
import { cva, type VariantProps } from 'class-variance-authority';
import { Link } from 'react-router-dom';

import { ValidatorLogo } from '../validators/ValidatorLogo';
import { Text } from '_src/ui/app/shared/text';
import { IconTooltip } from '_src/ui/app/shared/tooltip';

import type { StakeObject } from '@mysten/sui.js';
import type { ReactNode } from 'react';

export enum StakeState {
    EARNING = 'EARNING',
    COOL_DOWN = 'COOL_DOWN',
    WITHDRAW = 'WITHDRAW',
    IN_ACTIVE = 'IN_ACTIVE',
}

const STATUS_COPY = {
    [StakeState.EARNING]: 'Staking Reward',
    [StakeState.COOL_DOWN]: 'Available to withdraw',
    [StakeState.WITHDRAW]: 'Withdraw',
    [StakeState.IN_ACTIVE]: 'Inactive',
};

const STATUS_VARIANT = {
    [StakeState.EARNING]: 'earning',
    [StakeState.COOL_DOWN]: 'coolDown',
    [StakeState.WITHDRAW]: 'withDraw',
    [StakeState.IN_ACTIVE]: 'inActive',
} as const;
interface DelegationObjectWithValidator extends StakeObject {
    validatorAddress: SuiAddress;
}

const cardStyle = cva(
    [
        'group flex no-underline flex-col p-3.75 pr-2 pt-3 box-border h-36 w-full rounded-2xl border group border-solid ',
    ],
    {
        variants: {
            variant: {
                earning:
                    'bg-white border border-gray-45 text-steel-dark hover:bg-sui/10 hover:border-sui/30',
                coolDown:
                    'bg-warning-light border-transparent text-steel-darker hover:border-warning',
                withDraw:
                    'bg-success-light border-transparent text-success-dark hover:border-success',
                inActive:
                    'bg-issue-light border-transparent text-issue hover:border-issue',
            },
        },
    }
);

export interface StakeCardContentProps extends VariantProps<typeof cardStyle> {
    statusLabel: string;
    statusText: string;
    children?: ReactNode;
}

function StakeCardContent({
    children,
    statusLabel,
    statusText,
    variant,
}: StakeCardContentProps) {
    return (
        <div className={cardStyle({ variant })}>
            {children}
            <div className="flex flex-col gap-1">
                <div className="text-subtitle font-medium">{statusLabel}</div>
                <div className="text-bodySmall font-semibold">{statusText}</div>
            </div>
        </div>
    );
}

interface StakeCardProps {
    delegationObject: DelegationObjectWithValidator;
    currentEpoch: number;
}

// For delegationsRequestEpoch n  through n + 2, show Start Earning
// Show epoch number or date/time for n + 3 epochs
// TODO: Change delegation to Stake
export function StakeCard({ delegationObject }: StakeCardProps) {
    const {
        stakedSuiId,
        principal,
        stakeRequestEpoch,
        estimatedReward,
        validatorAddress,
    } = delegationObject;
    const rewards = estimatedReward;

    const delegationState = StakeState.EARNING;
    const [stakeRewards, symbol] = useFormatCoin(
        principal + (rewards ?? 0),
        SUI_TYPE_ARG
    );

    const statusText = {
        // Epoch time before earning
        [StakeState.EARNING]: `Epoch #${stakeRequestEpoch + 2}`,
        // Epoch time before redrawing
        [StakeState.COOL_DOWN]: `Epoch #`,
        [StakeState.WITHDRAW]: 'Now',
        [StakeState.IN_ACTIVE]: 'Not earning rewards',
    };

    return (
        <Link
            to={`/stake/delegation-detail?${new URLSearchParams({
                validator: validatorAddress,
                staked: stakedSuiId,
            }).toString()}`}
            className="no-underline"
        >
            <StakeCardContent
                variant={STATUS_VARIANT[delegationState]}
                statusLabel={STATUS_COPY[delegationState]}
                statusText={statusText[delegationState]}
            >
                <div className="flex justify-between items-start mb-1 ">
                    <ValidatorLogo
                        validatorAddress={validatorAddress}
                        size="subtitle"
                        iconSize="md"
                        stacked
                    />

                    <div className="text-steel text-p1 opacity-0 group-hover:opacity-100">
                        <IconTooltip
                            tip="Object containing the delegated staked SUI tokens, owned by each delegator"
                            placement="top"
                        />
                    </div>
                </div>
                <div className="flex-1 mb-4">
                    <div className="flex items-baseline gap-1">
                        <Text variant="body" weight="semibold" color="gray-90">
                            {stakeRewards}
                        </Text>

                        <Text
                            variant="subtitle"
                            weight="normal"
                            color="gray-90"
                        >
                            {symbol}
                        </Text>
                    </div>
                </div>
            </StakeCardContent>
        </Link>
    );
}
