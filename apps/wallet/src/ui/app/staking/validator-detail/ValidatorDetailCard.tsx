// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { is, SuiObject } from '@mysten/sui.js';
import { useMemo } from 'react';

import { FEATURES } from '../../experimentation/features';
import StakeAmount from '../home/StakeAmount';
import { getName, STATE_OBJECT } from '../usePendingDelegation';
import BottomMenuLayout, { Content } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Card } from '_app/shared/card';
import { CardItem } from '_app/shared/card/CardItem';
import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import { totalActiveStakedSelector } from '_app/staking/selectors';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useAppSelector, useGetObject } from '_hooks';

import type { ValidatorState } from '../ValidatorDataTypes';

type ValidatorDetailCardProps = {
    validatorAddress: string;
};

export function ValidatorDetailCard({
    validatorAddress,
}: ValidatorDetailCardProps) {
    const accountAddress = useAppSelector(({ account }) => account.address);
    const { data, isLoading, isError } = useGetObject(STATE_OBJECT);
    const validatorsData =
        data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === 'moveObject'
            ? (data.details.data.fields as ValidatorState)
            : null;

    const validatorData = useMemo(() => {
        if (!validatorsData) return null;

        const validator =
            validatorsData.validators.fields.active_validators.find(
                (av) =>
                    av.fields.metadata.fields.sui_address === validatorAddress
            );

        if (!validator) return null;

        const {
            sui_balance,
            starting_epoch,
            pending_delegations,
            delegation_token_supply,
        } = validator.fields.delegation_staking_pool.fields;

        const num_epochs_participated = validatorsData.epoch - starting_epoch;
        const { name: rawName, sui_address } = validator.fields.metadata.fields;

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
            commissionRate: validator.fields.commission_rate,
            apy: APY > 0 ? APY : 'N/A',
            logo: null,
            address: sui_address,
            totalStaked: pending_delegations.reduce(
                (acc, fields) =>
                    (acc += BigInt(fields.fields.sui_amount || 0n)),
                0n
            ),
            // TODO: Calculate suiEarned
            suiEarned: 0n,
            pendingDelegationAmount: pending_delegationsByAddress.reduce(
                (acc, fields) =>
                    (acc += BigInt(fields.fields.sui_amount || 0n)),
                0n
            ),
        };
    }, [accountAddress, validatorAddress, validatorsData]);

    const totalStaked = useAppSelector(totalActiveStakedSelector);
    const pendingStake = validatorData?.pendingDelegationAmount || 0n;
    const apy = validatorData?.apy || 0;
    const commissionRate = validatorData?.commissionRate || 0;
    const totalStakedIncludingPending = totalStaked + pendingStake;
    const suiEarned = validatorData?.suiEarned || 0n;

    const stakeByValidatorAddress = `/stake/new?address=${encodeURIComponent(
        validatorAddress
    )}`;

    const stakingEnabled = useFeature(FEATURES.STAKING_ENABLED).on;

    if (isLoading) {
        return (
            <div className="p-2 w-full flex justify-center item-center h-full">
                <LoadingIndicator />
            </div>
        );
    }

    if (isError) {
        return (
            <div className="p-2">
                <Alert mode="warning">
                    <div className="mb-1 font-semibold">
                        Something went wrong
                    </div>
                </Alert>
            </div>
        );
    }

    return (
        <div className="flex flex-col flex-nowrap flex-grow h-full">
            <BottomMenuLayout>
                <Content>
                    <div className="justify-center w-full flex flex-col items-center">
                        <div className="w-full flex">
                            <Card
                                header={
                                    <div className="grid grid-cols-2 divide-x divide-solid divide-gray-45 divide-y-0 w-full">
                                        <CardItem title="Your Stake">
                                            <StakeAmount
                                                balance={
                                                    totalStakedIncludingPending
                                                }
                                                variant="heading4"
                                            />
                                        </CardItem>

                                        <CardItem title="Earned">
                                            <StakeAmount
                                                balance={suiEarned}
                                                variant="heading4"
                                                isEarnedRewards
                                            />
                                        </CardItem>
                                    </div>
                                }
                                padding="none"
                            >
                                <div className="divide-x flex divide-solid divide-gray-45 divide-y-0">
                                    <CardItem
                                        title={
                                            <div className="flex text-steel-darker gap-1 items-start">
                                                APY
                                                <div className="text-steel">
                                                    <IconTooltip
                                                        tip="Annual Percentage Yield"
                                                        placement="top"
                                                    />
                                                </div>
                                            </div>
                                        }
                                    >
                                        <div className="flex gap-0.5 items-baseline">
                                            <Text
                                                variant="heading4"
                                                weight="semibold"
                                                color="gray-90"
                                            >
                                                {apy}
                                            </Text>

                                            <Text
                                                variant="subtitleSmall"
                                                weight="medium"
                                                color="steel-dark"
                                            >
                                                %
                                            </Text>
                                        </div>
                                    </CardItem>

                                    <CardItem
                                        title={
                                            <div className="flex text-steel-darker gap-1">
                                                Commission
                                                <div className="text-steel">
                                                    <IconTooltip
                                                        tip="Validator commission"
                                                        placement="top"
                                                    />
                                                </div>
                                            </div>
                                        }
                                    >
                                        <div className="flex gap-0.5 items-baseline">
                                            <Text
                                                variant="heading4"
                                                weight="semibold"
                                                color="gray-90"
                                            >
                                                {commissionRate}
                                            </Text>

                                            <Text
                                                variant="subtitleSmall"
                                                weight="medium"
                                                color="steel-dark"
                                            >
                                                %
                                            </Text>
                                        </div>
                                    </CardItem>
                                </div>
                            </Card>
                        </div>
                        <div className="flex gap-2.5  w-full my-3.75">
                            <Button
                                size="large"
                                mode="outline"
                                href={stakeByValidatorAddress}
                                className="bg-gray-50 w-full"
                                disabled={!stakingEnabled}
                            >
                                <Icon
                                    icon={SuiIcons.Add}
                                    className="font-normal"
                                />
                                Stake SUI
                            </Button>
                            {Boolean(totalStakedIncludingPending) && (
                                <Button
                                    size="large"
                                    mode="outline"
                                    href={
                                        stakeByValidatorAddress +
                                        '&unstake=true'
                                    }
                                    className="w-full"
                                >
                                    <Icon
                                        icon={SuiIcons.Remove}
                                        className="text-heading6 font-normal"
                                    />
                                    Unstake SUI
                                </Button>
                            )}
                        </div>
                        {totalStakedIncludingPending > 1 && (
                            <div className="w-full">
                                <Button
                                    size="large"
                                    mode="outline"
                                    disabled
                                    href={
                                        stakeByValidatorAddress +
                                        '&unstake=true'
                                    }
                                    className="w-full"
                                >
                                    Unstake All SUI
                                </Button>
                            </div>
                        )}
                    </div>
                </Content>
                <Button
                    size="large"
                    mode="neutral"
                    href="/stake"
                    className="!text-steel-darker"
                >
                    <Icon
                        icon={SuiIcons.ArrowLeft}
                        className="text-body text-gray-60 font-normal"
                    />
                    Back
                </Button>
            </BottomMenuLayout>
        </div>
    );
}
