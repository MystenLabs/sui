// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObject, Base64DataBuffer } from '@mysten/sui.js';
import { useMemo } from 'react';
import { useParams, Navigate } from 'react-router-dom';

import ErrorResult from '~/components/error-result/ErrorResult';
import { StatsCoin } from '~/components/top-validators-card/StatsCoin';
import { useGetObject } from '~/hooks/useGetObject';
import {
    VALIDATORS_OBJECT_ID,
    type ValidatorState,
} from '~/pages/validator/ValidatorDataTypes';
import { Card } from '~/ui/Card';
import { DescriptionList, DescriptionItem } from '~/ui/DescriptionList';
import { Heading } from '~/ui/Heading';
import { ImageIcon } from '~/ui/ImageIcon';
import { AddressLink } from '~/ui/InternalLink';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Stats } from '~/ui/Stats';
import { Text } from '~/ui/Text';
import { getName } from '~/utils/getName';
import { getStakedPercent } from '~/utils/getStakedPercent';

function ValidatorDetails() {
    const { id } = useParams();
    const { data, isLoading } = useGetObject(VALIDATORS_OBJECT_ID);

    const validatorsData =
        data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === 'moveObject'
            ? (data.details.data.fields as ValidatorState)
            : null;

    const validatorData = useMemo(() => {
        if (!validatorsData) return null;
        return (
            validatorsData.validators.fields.active_validators.find(
                (av) => av.fields.metadata.fields.sui_address === id
            ) || null
        );
    }, [id, validatorsData]);

    const validator = useMemo(() => {
        if (!validatorData || !validatorsData) return null;

        const { name, pubkey_bytes, sui_address } =
            validatorData.fields.metadata.fields;

        const { sui_balance, starting_epoch, delegation_token_supply } =
            validatorData.fields.delegation_staking_pool.fields;

        const num_epochs_participated = validatorsData.epoch - starting_epoch;

        const APY = Math.pow(
            1 +
                (sui_balance - delegation_token_supply.fields.value) /
                    delegation_token_supply.fields.value,
            365 / num_epochs_participated - 1
        );

        return {
            name: getName(name),
            pubkeyBytes: new Base64DataBuffer(
                new Uint8Array(pubkey_bytes)
            ).toString(),
            suiAddress: sui_address,
            apy: APY ? APY : 0,
            logo: null,
            delegatedStakePercentage: getStakedPercent(
                validatorData.fields.stake_amount,
                validatorsData.validators.fields.total_validator_stake
            ),
            totalStake: validatorData.fields.stake_amount,
            delegatedStake: validatorData.fields.stake_amount,
            address: sui_address,
            
            // TODO: add missing fields
            numberOfDelegators: 0,
            selfStake: 0,
            lastEpoch: 0,
            totalRewards: 0,
        };
    }, [validatorData, validatorsData]);

    if (!id) {
        return <Navigate to="/validators" />;
    }

    if (isLoading) {
        return (
            <div className="mt-5 mb-10 flex items-center justify-center">
                <LoadingSpinner />
            </div>
        );
    }

    if (!validator) {
        return (
            <div className="mt-5 mb-10 flex items-center justify-center">
                <ErrorResult id={id} errorMsg="No validator data found" />
            </div>
        );
    }

    return (
        <div className="mt-5 mb-10">
            <div className="flex flex-col flex-nowrap md:flex-row ">
                <div className="flex basis-full gap-5 capitalize md:basis-1/3">
                    <ImageIcon
                        src={validator.logo}
                        alt={validator.name}
                        variant="square"
                        size="xl"
                    />
                    <div className="mt-1 flex flex-col gap-2.5 md:gap-3.5">
                        <Heading
                            as="h1"
                            variant="heading2/bold"
                            color="gray-100"
                        >
                            {validator.name}
                        </Heading>
                    </div>
                </div>
                <div className="basis-full break-all md:basis-2/3">
                    <DescriptionItem title="Address">
                        <AddressLink
                            address={validator.suiAddress}
                            noTruncate
                        />
                    </DescriptionItem>
                    <DescriptionList>
                        {validator.pubkeyBytes && (
                            <DescriptionItem title="Public Key">
                                <Text variant="p1/medium" color="gray-90">
                                    {validator.pubkeyBytes}
                                </Text>
                            </DescriptionItem>
                        )}
                    </DescriptionList>
                </div>
            </div>
            <div className="mt-8 flex w-full">
                <div className="mt-8 flex w-full flex-col gap-5 md:flex-row">
                    <div className="basis-full md:basis-2/5">
                        <Card spacing="lg">
                            <div className="flex  max-w-full flex-col flex-nowrap gap-8">
                                <Heading as="div" variant="heading4/semibold">
                                    SUI Staked on Validator
                                </Heading>
                                <div className="flex flex-col flex-nowrap gap-8 md:flex-row">
                                    <Stats
                                        label="Staking APY"
                                        tooltip="Staking APY"
                                    >
                                        <Heading
                                            as="h3"
                                            variant="heading2/semibold"
                                            color="steel-darker"
                                        >
                                            {validator.apy}%
                                        </Heading>
                                    </Stats>
                                    <Stats
                                        label="Total Staked"
                                        tooltip="Total Staked"
                                    >
                                        <StatsCoin
                                            amount={validator.totalStake}
                                        />
                                    </Stats>
                                </div>
                                <div className="flex flex-col flex-nowrap gap-8 md:flex-row">
                                    <Stats
                                        label="Delegators"
                                        tooltip="Delegators"
                                    >
                                        <Heading
                                            as="h3"
                                            variant="heading3/semibold"
                                            color="steel-darker"
                                        >
                                            {validator.numberOfDelegators}
                                        </Heading>
                                    </Stats>
                                    <Stats
                                        label="Delegated Staked"
                                        tooltip="Delegated Staked"
                                    >
                                        <Heading
                                            as="h3"
                                            variant="heading3/semibold"
                                            color="steel-darker"
                                        >
                                            {validator.delegatedStakePercentage}
                                            %
                                        </Heading>
                                    </Stats>
                                    <Stats
                                        label="Self Staked"
                                        tooltip="Self Staked"
                                    >
                                        <Heading
                                            as="h3"
                                            variant="heading3/semibold"
                                            color="steel-darker"
                                        >
                                            {validator.selfStake}%
                                        </Heading>
                                    </Stats>
                                </div>
                            </div>
                        </Card>
                    </div>
                    <div className="basis-full md:basis-1/4">
                        <Card spacing="lg">
                            <div className="flex  max-w-full flex-col flex-nowrap gap-8">
                                <Heading as="div" variant="heading4/semibold">
                                    SUI Staked on Validator
                                </Heading>
                                <div className="flex flex-col flex-nowrap gap-8">
                                    <Stats
                                        label="Last Epoch"
                                        tooltip="Last Epoch"
                                    >
                                        <Heading
                                            as="h3"
                                            variant="heading3/semibold"
                                            color="steel-darker"
                                        >
                                            {validator.lastEpoch}
                                        </Heading>
                                    </Stats>
                                    <Stats
                                        label="Total Reward"
                                        tooltip="Total Reward"
                                    >
                                        <Heading
                                            as="h3"
                                            variant="heading3/semibold"
                                            color="steel-darker"
                                        >
                                            {validator.totalRewards}
                                        </Heading>
                                    </Stats>
                                </div>
                            </div>
                        </Card>
                    </div>
                </div>
            </div>
        </div>
    );
}

export { ValidatorDetails };
