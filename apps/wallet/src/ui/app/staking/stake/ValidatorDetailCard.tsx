// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { isSuiObject, isSuiMoveObject, SUI_TYPE_ARG } from '@mysten/sui.js';
import { useMemo } from 'react';

import { getName, STATE_OBJECT } from '../usePendingDelegation';
import { Card } from '_app/shared/card';
import CoinBalance from '_app/shared/coin-balance';
import { ImageIcon } from '_app/shared/image-icon';
import Alert from '_components/alert';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useGetObject, useAppSelector } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import { Text } from '_src/ui/app/shared/text';
import { IconTooltip } from '_src/ui/app/shared/tooltip';

import type { ValidatorState } from '../ValidatorDataTypes';
import type { ReactNode } from 'react';

function SplitList({ children }: { children: ReactNode[] }) {
    return <div className="flex py-2.5 gap-2 items-center">{children}</div>;
}

export function ValidateDetailFormCard({
    validatorAddress,
    unstake,
}: {
    validatorAddress: string;
    unstake?: boolean;
}) {
    const accountAddress = useAppSelector(({ account }) => account.address);
    const { data, isLoading, isError } = useGetObject(STATE_OBJECT);

    const validatorsData =
        data && isSuiObject(data.details) && isSuiMoveObject(data.details.data)
            ? (data.details.data.fields as ValidatorState)
            : null;

    const validatorDataByAddress = useMemo(() => {
        if (!validatorsData) return null;
        return validatorsData.validators.fields.active_validators
            .filter(
                (av) =>
                    av.fields.metadata.fields.sui_address === validatorAddress
            )
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
                    totalStaked: pending_delegations.reduce(
                        (acc, fields) =>
                            (acc += BigInt(fields.fields.sui_amount || 0n)),
                        0n
                    ),
                    pendingDelegationAmount:
                        pending_delegationsByAddress.reduce(
                            (acc, fields) =>
                                (acc += BigInt(fields.fields.sui_amount || 0n)),
                            0n
                        ),
                };
            })[0];
    }, [accountAddress, validatorAddress, validatorsData]);

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
        <div className="w-full">
            {validatorDataByAddress && (
                <Card
                    header={
                        <SplitList>
                            <ImageIcon
                                src={validatorDataByAddress.logo}
                                alt={validatorDataByAddress.name}
                                size="small"
                                variant="circle"
                            />
                            <Text variant="body" weight="semibold">
                                {validatorDataByAddress.name}
                            </Text>
                        </SplitList>
                    }
                    footer={
                        <>
                            <Text
                                variant="body"
                                weight="medium"
                                color="steel-darker"
                            >
                                Your Staked SUI
                            </Text>

                            <CoinBalance
                                balance={
                                    validatorDataByAddress.pendingDelegationAmount
                                }
                                className="text-body steel-darker"
                                type={SUI_TYPE_ARG}
                                diffSymbol
                            />
                        </>
                    }
                >
                    <div className="divide-x flex divide-solid divide-gray-45 divide-y-0 flex-col gap-3.5 mb-3.5">
                        <div className="flex gap-2 items-center justify-between ">
                            <div className="flex gap-1 items-baseline text-steel">
                                <Text
                                    variant="body"
                                    weight="medium"
                                    color="steel-darker"
                                >
                                    Staking APY
                                </Text>
                                <IconTooltip tip="Annual Percentage Yield" />
                            </div>

                            <Text
                                variant="body"
                                weight="semibold"
                                color="gray-90"
                            >
                                {validatorDataByAddress.apy}{' '}
                                {typeof validatorDataByAddress.apy !==
                                    'string' && '%'}
                            </Text>
                        </div>
                        {!unstake && (
                            <div className="flex gap-2 items-center justify-between">
                                <div className="flex gap-1 items-baseline text-steel">
                                    <Text
                                        variant="body"
                                        weight="medium"
                                        color="steel-darker"
                                    >
                                        Total Staked
                                    </Text>
                                </div>

                                <CoinBalance
                                    balance={validatorDataByAddress.totalStaked}
                                    className="text-body font-medium steel-darker"
                                    type={GAS_TYPE_ARG}
                                    diffSymbol
                                />
                            </div>
                        )}
                    </div>
                </Card>
            )}
        </div>
    );
}
