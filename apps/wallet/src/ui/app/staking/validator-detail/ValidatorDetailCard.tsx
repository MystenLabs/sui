// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';

import BottomMenuLayout, { Content } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Card } from '_app/shared/card';
import { CardItem } from '_app/shared/card/CardItem';
import CoinBalance from '_app/shared/coin-balance';
import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import { totalActiveStakedSelector } from '_app/staking/selectors';
import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

type ValidatorDetailCardProps = {
    validatorAddress: string;
    pendingDelegationAmount: bigint;
    suiEarned: bigint;
    apy: number | string;
    commissionRate: number;
};

export function ValidatorDetailCard({
    validatorAddress,
    pendingDelegationAmount,
    suiEarned,
    apy,
    commissionRate,
}: ValidatorDetailCardProps) {
    const totalStaked = useAppSelector(totalActiveStakedSelector);
    const pendingStake = pendingDelegationAmount || 0n;
    const totalStakedIncludingPending = totalStaked + pendingStake;

    const stakeByValidatorAddress = `/stake/new?address=${encodeURIComponent(
        validatorAddress
    )}`;

    return (
        <div className="flex flex-col flex-nowrap flex-grow h-full">
            <BottomMenuLayout>
                <Content>
                    <div className="justify-center w-full flex flex-col items-center">
                        <div className="mb-4 w-full flex">
                            <Card
                                header={
                                    <div className="grid grid-cols-2 divide-x divide-solid divide-gray-45 divide-y-0 w-full">
                                        <CardItem
                                            title="Your Stake"
                                            value={
                                                <CoinBalance
                                                    balance={
                                                        totalStakedIncludingPending
                                                    }
                                                    type={GAS_TYPE_ARG}
                                                    diffSymbol
                                                />
                                            }
                                        />

                                        <CardItem
                                            title="EARNED"
                                            value={
                                                <CoinBalance
                                                    balance={suiEarned}
                                                    type={SUI_TYPE_ARG}
                                                    mode="neutral"
                                                    className="!text-gray-60"
                                                    diffSymbol
                                                />
                                            }
                                        />
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
                                        value={
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
                                        }
                                    />

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
                                        value={
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
                                        }
                                    />
                                </div>
                            </Card>
                        </div>
                        <div className="flex gap-2.5 mb-8 w-full mt-4">
                            <Button
                                size="large"
                                mode="outline"
                                href={stakeByValidatorAddress}
                                className="bg-gray-50 w-full"
                            >
                                <Icon icon={SuiIcons.Add} />
                                Stake SUI
                            </Button>
                            {totalStakedIncludingPending > 0 && (
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
                                    <Icon
                                        icon={SuiIcons.Remove}
                                        className="text-heading6"
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
                                    disabled={true}
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
