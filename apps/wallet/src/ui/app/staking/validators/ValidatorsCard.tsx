// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useMemo } from 'react';

import { FEATURES } from '../../experimentation/features';
import StakeAmount from '../home/StakeAmount';
import { useGetValidatorsByDelegator } from '../useGetValidatorsData';
import { ValidatorGridCard } from './ValidatorGridCard';
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

export function ValidatorsCard() {
    const accountAddress = useAppSelector(({ account }) => account.address);
    const {
        data: stakeValidators,
        isLoading,
        isError,
        error,
    } = useGetValidatorsByDelegator(accountAddress || '');

    const totalActivePendingStake = useMemo(() => {
        if (!stakeValidators) return 0n;
        return stakeValidators.reduce(
            (acc, { staked_sui }) => acc + BigInt(staked_sui.principal.value),
            0n
        );
    }, [stakeValidators]);

    // return staked validator addresses from stakeValidators and remove duplicates
    const validatorsAddr = useMemo(() => {
        if (!stakeValidators) return [];
        return stakeValidators.reduce((acc, delegator) => {
            return {
                ...acc,
                [delegator.staked_sui.validator_address]: {
                    validatorsAddr: delegator.staked_sui.validator_address,
                },
            };
        }, []);
    }, [stakeValidators]);

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
        <div className="flex flex-col flex-nowrap h-full overflow-x-scroll w-full">
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
                                        Staking on {stakeValidators.length}
                                        {stakeValidators.length > 1
                                            ? ' VALIDATORS'
                                            : ' VALIDATOR'}
                                    </Text>
                                </div>
                            }
                        >
                            <div className="flex divide-x divide-solid divide-gray-45 divide-y-0">
                                <CardItem title="Your Stake">
                                    <StakeAmount
                                        balance={totalActivePendingStake}
                                        variant="heading4"
                                    />
                                </CardItem>
                                {/* TODO: show the actual Rewards Collected value https://github.com/MystenLabs/sui/issues/3605 */}
                                <CardItem title="Earned">
                                    <StakeAmount
                                        balance={0n}
                                        variant="heading4"
                                        isEarnedRewards
                                    />
                                </CardItem>
                            </div>
                        </Card>

                        <div className="grid grid-cols-2 gap-2.5 mt-4">
                            {validatorsAddr.map((validatorAddr) => (
                                <ValidatorGridCard
                                    validatorAddress={validatorAddr}
                                    key={validatorAddr}
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
