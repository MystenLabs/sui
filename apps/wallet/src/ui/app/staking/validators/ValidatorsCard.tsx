// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { is, SuiObject } from '@mysten/sui.js';
import { useMemo } from 'react';

import { FEATURES } from '../../experimentation/features';
import { ActiveDelegation } from '../home/ActiveDelegation';
import { DelegationCard, DelegationState } from '../home/DelegationCard';
import StakeAmount from '../home/StakeAmount';
import { getName, STATE_OBJECT } from '../usePendingDelegation';
import BottomMenuLayout, {
    Menu,
    Content,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Card, CardItem } from '_app/shared/card';
import { Text } from '_app/shared/text';
import {
    activeDelegationIDsSelector,
    totalActiveStakedSelector,
} from '_app/staking/selectors';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useAppSelector, useGetObject } from '_hooks';

import type { ValidatorState } from '../ValidatorDataTypes';

export function ValidatorsCard() {
    const accountAddress = useAppSelector(({ account }) => account.address);
    const { data, isLoading } = useGetObject(STATE_OBJECT);
    const totalStaked = useAppSelector(totalActiveStakedSelector);
    const activeDelegationIDs = useAppSelector(activeDelegationIDsSelector);

    const validatorsData =
        data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === 'moveObject'
            ? (data.details.data.fields as ValidatorState)
            : null;

    const validators = useMemo(() => {
        if (!validatorsData) return [];
        return validatorsData.validators.fields.active_validators
            .map((av) => {
                const rawName = av.fields.metadata.fields.name;
                const {
                    sui_balance,
                    starting_epoch,
                    pending_delegations,
                    delegation_token_supply,
                } = av.fields.delegation_staking_pool.fields;

                const num_epochs_participated =
                    validatorsData.epoch - starting_epoch;

                const APY = Math.pow(
                    1 +
                        (sui_balance - delegation_token_supply.fields.value) /
                            delegation_token_supply.fields.value,
                    365 / num_epochs_participated - 1
                );

                const pending_delegationsByAddress = pending_delegations
                    ? pending_delegations.filter(
                          (d) => d.fields.delegator === accountAddress
                      )
                    : [];

                return {
                    name: getName(rawName),
                    apy: APY > 0 ? APY : 'N/A',
                    logo: null,
                    address: av.fields.metadata.fields.sui_address,
                    pendingDelegationAmount:
                        pending_delegationsByAddress.reduce(
                            (acc, fields) =>
                                (acc += BigInt(fields.fields.sui_amount || 0n)),
                            0n
                        ),
                };
            })
            .sort((a, b) => (a.name > b.name ? 1 : -1));
    }, [accountAddress, validatorsData]);

    // TODO - get this from the metadata
    const earnedRewards = BigInt(0);

    const numOfValidators = validators.filter(({ pendingDelegationAmount }) =>
        Boolean(pendingDelegationAmount)
    ).length;
    const stakingEnabled = useFeature(FEATURES.STAKING_ENABLED).on;

    if (isLoading) {
        return (
            <div className="p-2 w-full flex justify-center item-center h-full">
                <LoadingIndicator />
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
                                        Staking on {numOfValidators}
                                        {numOfValidators > 1
                                            ? ' VALIDATORS'
                                            : ' VALIDATOR'}
                                    </Text>
                                </div>
                            }
                        >
                            <div className="flex divide-x divide-solid divide-gray-45 divide-y-0">
                                <CardItem title="Your Stake">
                                    <StakeAmount
                                        balance={totalStaked}
                                        variant="heading4"
                                    />
                                </CardItem>
                                {/* TODO: show the actual Rewards Collected value https://github.com/MystenLabs/sui/issues/3605 */}
                                <CardItem title="Earned">
                                    <StakeAmount
                                        balance={earnedRewards}
                                        variant="heading4"
                                        isEarnedRewards
                                    />
                                </CardItem>
                            </div>
                        </Card>

                        <div className="grid grid-cols-2 gap-2.5 mt-4">
                            {validators
                                .filter(
                                    ({ pendingDelegationAmount }) =>
                                        pendingDelegationAmount > 0
                                )
                                .map(
                                    (
                                        {
                                            name,
                                            pendingDelegationAmount,
                                            address,
                                        },
                                        index
                                    ) => (
                                        <DelegationCard
                                            key={index}
                                            name={name}
                                            staked={pendingDelegationAmount}
                                            state={DelegationState.WARM_UP}
                                            address={address}
                                        />
                                    )
                                )}

                            {activeDelegationIDs.map((delegationID) => (
                                <ActiveDelegation
                                    key={delegationID}
                                    id={delegationID}
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
