// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { SUI_TYPE_ARG } from '@mysten/sui.js';

import { FEATURES } from '../../experimentation/features';
import { ActiveDelegation } from '../home/ActiveDelegation';
import { DelegationCard, DelegationState } from '../home/DelegationCard';
import StakeAmount from '../home/StakeAmount';
import BottomMenuLayout, {
    Menu,
    Content,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Card, CardItem } from '_app/shared/card';
import { Text } from '_app/shared/text';
import Icon, { SuiIcons } from '_components/icon';

type ValidatorsProp = {
    name: string;
    apy: number | string;
    logo: string | null;
    address: string;
    pendingDelegationAmount: bigint;
};

type ValidatorsCardProp = {
    validators: ValidatorsProp[];
    totalStaked: bigint;
    earnedRewards: bigint;
    activeDelegationIDs: string[];
};

export function ValidatorsCard({
    validators,
    totalStaked,
    earnedRewards,
    activeDelegationIDs,
}: ValidatorsCardProp) {
    const numOfValidators = validators.filter(({ pendingDelegationAmount }) =>
        Boolean(pendingDelegationAmount)
    ).length;
    const stakingEnabled = useFeature(FEATURES.STAKING_ENABLED).on;

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
                                        STAKING ON {numOfValidators}
                                        {numOfValidators > 1
                                            ? ' VALIDATORS'
                                            : ' VALIDATOR'}
                                    </Text>
                                </div>
                            }
                        >
                            <div className="flex divide-x divide-solid divide-gray-45 divide-y-0">
                                <CardItem
                                    title="Your Stake"
                                    value={
                                        <StakeAmount
                                            balance={totalStaked}
                                            type={SUI_TYPE_ARG}
                                            diffSymbol
                                            size="heading4"
                                            color="gray-90"
                                            symbolColor="steel"
                                        />
                                    }
                                />
                                {/* TODO: show the actual Rewards Collected value https://github.com/MystenLabs/sui/issues/3605 */}
                                <CardItem
                                    title="EARNED"
                                    value={
                                        <StakeAmount
                                            balance={earnedRewards}
                                            type={SUI_TYPE_ARG}
                                            diffSymbol
                                            symbolColor="gray-60"
                                            size="heading4"
                                            color="gray-60"
                                        />
                                    }
                                />
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
