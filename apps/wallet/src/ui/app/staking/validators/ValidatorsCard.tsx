// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { Plus12 } from '@mysten/icons';
import { useMemo } from 'react';

import { useActiveAddress } from '../../hooks/useActiveAddress';
import { getAllStakeSui } from '../getAllStakeSui';
import { StakeAmount } from '../home/StakeAmount';
import { StakeCard } from '../home/StakedCard';
import { useGetDelegatedStake } from '../useGetDelegatedStake';
import { useSystemState } from '../useSystemState';
import { Button } from '_app/shared/ButtonUI';
import BottomMenuLayout, {
    Menu,
    Content,
} from '_app/shared/bottom-menu-layout';
import { Card, CardItem } from '_app/shared/card';
import { Text } from '_app/shared/text';
import Alert from '_components/alert';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { FEATURES } from '_src/shared/experimentation/features';

export function ValidatorsCard() {
    const accountAddress = useActiveAddress();
    const {
        data: delegatedStake,
        isLoading,
        isError,
        error,
    } = useGetDelegatedStake(accountAddress || '');

    const { data: system } = useSystemState();
    const activeValidators = system?.activeValidators;

    // Total active stake for all Staked validators
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
            delegatedStake.reduce(
                (acc, curr) =>
                    curr.stakes.reduce(
                        (total, { estimatedReward }) =>
                            total + BigInt(estimatedReward || 0),
                        acc
                    ),
                0n
            ) || 0n
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
                                <div className="py-2.5 flex px-3.75 justify-center w-full">
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
                                        currentEpoch={Number(system.epoch)}
                                        key={delegation.stakedSuiId}
                                    />
                                ))}
                        </div>
                    </div>
                </Content>
                <Menu stuckClass="staked-cta" className="w-full px-0 pb-0 mx-0">
                    <Button
                        size="tall"
                        variant="secondary"
                        to="new"
                        disabled={!stakingEnabled}
                        before={<Plus12 />}
                        text="Stake SUI"
                    />
                </Menu>
            </BottomMenuLayout>
        </div>
    );
}
