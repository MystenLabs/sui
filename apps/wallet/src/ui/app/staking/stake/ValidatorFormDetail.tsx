// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';

import { calculateAPY } from '../calculateAPY';
import { StakeAmount } from '../home/StakeAmount';
import { useGetDelegatedStake } from '../useGetDelegatedStake';
import { useSystemState } from '../useSystemState';
import { ValidatorLogo } from '../validators/ValidatorLogo';
import { Card } from '_app/shared/card';
import Alert from '_components/alert';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useAppSelector } from '_hooks';
import { Text } from '_src/ui/app/shared/text';
import { IconTooltip } from '_src/ui/app/shared/tooltip';

type ValidatorFormDetailProps = {
    validatorAddress: string;
    unstake?: boolean;
};

export function ValidatorFormDetail({
    validatorAddress,
    unstake,
}: ValidatorFormDetailProps) {
    const accountAddress = useAppSelector(({ account }) => account.address);

    const [searchParams] = useSearchParams();
    const stakeIdParams = searchParams.get('staked');
    const {
        data: system,
        isLoading: loadingValidators,
        isError: errorValidators,
    } = useSystemState();

    const {
        data: allDelegation,
        isLoading,
        isError,
        error,
    } = useGetDelegatedStake(accountAddress || '');

    const validatorData = useMemo(() => {
        if (!system) return null;
        return system.active_validators.find(
            (av) => av.sui_address === validatorAddress
        );
    }, [validatorAddress, system]);

    const totalValidatorStake = validatorData?.staking_pool_sui_balance || 0;

    const totalStake = useMemo(() => {
        if (!allDelegation) return 0n;
        let totalActiveStake = 0n;

        if (stakeIdParams) {
            const balance =
                allDelegation.find(
                    ({ staked_sui }) => staked_sui.id.id === stakeIdParams
                )?.staked_sui.principal.value || 0;
            return BigInt(balance);
        }

        allDelegation.forEach((event) => {
            if (event.staked_sui.validator_address === validatorAddress) {
                totalActiveStake += BigInt(event.staked_sui.principal.value);
            }
        });
        return totalActiveStake;
    }, [allDelegation, validatorAddress, stakeIdParams]);

    const apy = useMemo(() => {
        if (!validatorData || !system) return 0;
        return calculateAPY(validatorData, +system.epoch);
    }, [validatorData, system]);

    if (isLoading || loadingValidators) {
        return (
            <div className="p-2 w-full flex justify-center items-center h-full">
                <LoadingIndicator />
            </div>
        );
    }

    if (isError || errorValidators) {
        return (
            <div className="p-2">
                <Alert mode="warning">
                    <div className="mb-1 font-semibold">
                        {error?.message ?? 'Error loading validator data'}
                    </div>
                </Alert>
            </div>
        );
    }

    return (
        <div className="w-full">
            {validatorData && (
                <Card
                    titleDivider
                    header={
                        <div className="flex py-2.5 gap-2 items-center">
                            <ValidatorLogo
                                validatorAddress={validatorAddress}
                                iconSize="sm"
                                size="body"
                            />
                        </div>
                    }
                    footer={
                        !unstake && (
                            <>
                                <Text
                                    variant="body"
                                    weight="medium"
                                    color="steel-darker"
                                >
                                    Your Staked SUI
                                </Text>

                                <StakeAmount
                                    balance={totalStake}
                                    variant="body"
                                />
                            </>
                        )
                    }
                >
                    <div className="divide-x flex divide-solid divide-gray-45 divide-y-0 flex-col gap-3.5">
                        <div className="flex gap-2 items-center justify-between ">
                            <div className="flex gap-1 items-baseline text-steel">
                                <Text
                                    variant="body"
                                    weight="medium"
                                    color="steel-darker"
                                >
                                    Staking APY
                                </Text>
                                <IconTooltip tip="This is the Annualized Percentage Yield of the a specific validator’s past operations. Note there is no guarantee this APY will be true in the future." />
                            </div>

                            <Text
                                variant="body"
                                weight="semibold"
                                color="gray-90"
                            >
                                {apy > 0 ? `${apy}%` : '--'}
                            </Text>
                        </div>
                        {!unstake && (
                            <div className="flex gap-2 items-center justify-between mb-3.5">
                                <div className="flex gap-1 items-baseline text-steel">
                                    <Text
                                        variant="body"
                                        weight="medium"
                                        color="steel-darker"
                                    >
                                        Total Staked
                                    </Text>
                                    <IconTooltip tip="The total SUI staked on the network by this validator and its delegators, to validate the network and earn rewards." />
                                </div>
                                <StakeAmount
                                    balance={totalValidatorStake}
                                    variant="body"
                                />
                            </div>
                        )}
                    </div>
                </Card>
            )}
        </div>
    );
}
