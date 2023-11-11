// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Card } from '_app/shared/card';
import Alert from '_components/alert';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import {
	DELEGATED_STAKES_QUERY_REFETCH_INTERVAL,
	DELEGATED_STAKES_QUERY_STALE_TIME,
} from '_src/shared/constants';
import { Text } from '_src/ui/app/shared/text';
import { IconTooltip } from '_src/ui/app/shared/tooltip';
import {
	calculateStakeShare,
	formatPercentageDisplay,
	useGetDelegatedStake,
	useGetValidatorsApy,
} from '@mysten/core';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';

import { useActiveAddress } from '../../hooks/useActiveAddress';
import { getStakeSuiBySuiId } from '../getStakeSuiBySuiId';
import { getTokenStakeSuiForValidator } from '../getTokenStakeSuiForValidator';
import { StakeAmount } from '../home/StakeAmount';
import { ValidatorLogo } from '../validators/ValidatorLogo';

type ValidatorFormDetailProps = {
	validatorAddress: string;
	unstake?: boolean;
};

export function ValidatorFormDetail({ validatorAddress, unstake }: ValidatorFormDetailProps) {
	const accountAddress = useActiveAddress();

	const [searchParams] = useSearchParams();
	const stakeIdParams = searchParams.get('staked');
	const {
		data: system,
		isPending: loadingValidators,
		isError: errorValidators,
	} = useSuiClientQuery('getLatestSuiSystemState');

	const {
		data: stakeData,
		isPending,
		isError,
		error,
	} = useGetDelegatedStake({
		address: accountAddress || '',
		staleTime: DELEGATED_STAKES_QUERY_STALE_TIME,
		refetchInterval: DELEGATED_STAKES_QUERY_REFETCH_INTERVAL,
	});

	const { data: rollingAverageApys } = useGetValidatorsApy();

	const validatorData = useMemo(() => {
		if (!system) return null;
		return system.activeValidators.find((av) => av.suiAddress === validatorAddress);
	}, [validatorAddress, system]);

	//TODO: verify this is the correct validator stake balance
	const totalValidatorStake = validatorData?.stakingPoolSuiBalance || 0;

	const totalStake = useMemo(() => {
		if (!stakeData) return 0n;
		return unstake
			? getStakeSuiBySuiId(stakeData, stakeIdParams)
			: getTokenStakeSuiForValidator(stakeData, validatorAddress);
	}, [stakeData, stakeIdParams, unstake, validatorAddress]);

	const totalValidatorsStake = useMemo(() => {
		if (!system) return 0;
		return system.activeValidators.reduce(
			(acc, curr) => (acc += BigInt(curr.stakingPoolSuiBalance)),
			0n,
		);
	}, [system]);

	const totalStakePercentage = useMemo(() => {
		if (!system || !validatorData) return null;

		return calculateStakeShare(
			BigInt(validatorData.stakingPoolSuiBalance),
			BigInt(totalValidatorsStake),
		);
	}, [system, totalValidatorsStake, validatorData]);

	const { apy, isApyApproxZero } = rollingAverageApys?.[validatorAddress] ?? {
		apy: null,
	};

	if (isPending || loadingValidators) {
		return (
			<div className="p-2 w-full flex justify-center items-center h-full">
				<LoadingIndicator />
			</div>
		);
	}

	if (isError || errorValidators) {
		return (
			<div className="p-2">
				<Alert>
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
						<div className="flex py-2.5 px-3.75 gap-2 items-center">
							<ValidatorLogo validatorAddress={validatorAddress} iconSize="sm" size="body" />
						</div>
					}
					footer={
						!unstake && (
							<>
								<Text variant="body" weight="medium" color="steel-darker">
									Your Staked SUI
								</Text>

								<StakeAmount balance={totalStake} variant="body" />
							</>
						)
					}
				>
					<div className="flex flex-col gap-3.5">
						<div className="flex gap-2 items-center justify-between">
							<div className="flex gap-1 items-center text-steel">
								<Text variant="body" weight="medium" color="steel-darker">
									Staking APY
								</Text>
								<IconTooltip
									noFullWidth
									tip="This is the Annualized Percentage Yield of the a specific validator’s past operations. Note there is no guarantee this APY will be true in the future."
								/>
							</div>

							<Text variant="body" weight="semibold" color="gray-90">
								{formatPercentageDisplay(apy, '--', isApyApproxZero)}
							</Text>
						</div>
						<div className="flex gap-2 items-center justify-between">
							<div className="flex gap-1 items-center text-steel">
								<Text variant="body" weight="medium" color="steel-darker">
									Stake Share
								</Text>
								<IconTooltip
									noFullWidth
									tip="The percentage of total stake managed by this validator"
								/>
							</div>

							<Text variant="body" weight="semibold" color="gray-90">
								{formatPercentageDisplay(totalStakePercentage)}
							</Text>
						</div>

						{!unstake && (
							<div className="flex gap-2 items-center justify-between mb-3.5">
								<div className="flex gap-1 items-center text-steel">
									<Text variant="body" weight="medium" color="steel-darker">
										Total Staked
									</Text>
									<IconTooltip
										noFullWidth
										tip="The total SUI staked on the network by this validator and its delegators, to validate the network and earn rewards."
									/>
								</div>
								<StakeAmount balance={totalValidatorStake} variant="body" />
							</div>
						)}
					</div>
				</Card>
			)}
		</div>
	);
}
