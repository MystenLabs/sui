// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useMemo } from 'react';

import { getAllStakeSui } from '../getAllStakeSui';
import { getStakingRewards } from '../getStakingRewards';
import { StakeAmount } from '../home/StakeAmount';
import { StakeCard } from '../home/StakedCard';
import { useGetDelegatedStake } from '../useGetDelegatedStake';
import { useSystemState } from '../useSystemState';
import BottomMenuLayout, {
    Menu,
    Content,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Card, CardItem } from '_app/shared/card';
import { Text } from '_app/shared/text';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useAppSelector } from '_hooks';
import { FEATURES } from '_src/shared/experimentation/features';

export function ValidatorsCard() {
    const accountAddress = useAppSelector(({ account }) => account.address);
    const {
        data: delegatedStake,
        isLoading,
        isError,
        error,
    } = useGetDelegatedStake(accountAddress || '');

    const { data: system } = useSystemState();
    const activeValidators = system?.activeValidators;

    // Total active stake for all delegations
    const totalStake = useMemo(() => {
        if (!delegatedStake) return 0n;
        return getAllStakeSui(delegatedStake);
    }, [delegatedStake]);

    const delegations = useMemo(() => {
        return delegatedStake?.flatMap((delegation) => {
            return delegation.stakes.map((d) => ({
                ...d,
                validatorAddress: delegation.validatorAddress,
            }));
        });
    }, [delegatedStake]);

    // Get total rewards for all delegations
    const totalEarnTokenReward = useMemo(() => {
        if (!delegatedStake || !activeValidators) return 0n;
        return (
            delegatedStake.flatMap((delegation) => {
                return delegation.stakes.reduce((acc, d) => {
                    const validator = activeValidators.find(
                        ({ suiAddress }) =>
                            suiAddress === delegation.validatorAddress
                    );
                    return (
                        acc +
                        BigInt(validator ? getStakingRewards(validator, d) : 0)
                    );
                }, 0n);
            })[0] || 0n
        );
    }, [delegatedStake, activeValidators]);

    const numberOfValidators = delegatedStake?.length || 0;

    const stakingEnabled = useFeature(FEATURES.STAKING_ENABLED).on;

    if (isLoading) {
        return (
            <div className="p-2 w-full flex justify-center items-center h-full">
                <LoadingIndicator />
            </div>
        );
    }

    if (isError) {
        return (
            <div className="p-2 w-full flex justify-center items-center h-full">
                <Alert className="mb-2">
                    <strong>{error?.message}</strong>
                </Alert>
            </div>
        );
    }

    return (
        <div className="flex flex-col flex-nowrap h-full w-full">
            <BottomMenuLayout>
                <Content>
                    <div className="mb-4">
                        <Card
                            padding="none"
                            header={
                                <div className="py-2.5">
                                    <Text
                                        variant="captionSmall"
                                        weight="semibold"
                                        color="steel-darker"
                                    >
                                        Staking on {numberOfValidators}
                                        {numberOfValidators > 1
                                            ? ' Validators'
                                            : ' Validator'}
                                    </Text>
                                </div>
                            }
                        >
                            <div className="flex divide-x divide-solid divide-gray-45 divide-y-0">
                                <CardItem title="Your Stake">
                                    <StakeAmount
                                        balance={totalStake}
                                        variant="heading5"
                                    />
                                </CardItem>
                                <CardItem title="Earned">
                                    <StakeAmount
                                        balance={totalEarnTokenReward}
                                        variant="heading5"
                                        isEarnedRewards
                                    />
                                </CardItem>
                            </div>
                        </Card>

                        <div className="grid grid-cols-2 gap-2.5 mt-4">
                            {system &&
                                delegations?.map((delegation) => (
                                    <StakeCard
                                        delegationObject={delegation}
                                        currentEpoch={+system.epoch}
                                        key={delegation.stakedSuiId}
                                    />
                                ))}
                        </div>
                    </div>
                </Content>
                <Menu stuckClass="staked-cta" className="w-full px-0 pb-0 mx-0">
                    <Button
                        size="large"
                        mode="neutral"
                        href="new"
                        disabled={!stakingEnabled}
                        className="!text-steel-darker w-full"
                    >
                        <Icon
                            icon={SuiIcons.Plus}
                            className="text-body text-gray-65 font-normal"
                        />
                        Stake SUI
                    </Button>
                </Menu>
            </BottomMenuLayout>
        </div>
    );
}
