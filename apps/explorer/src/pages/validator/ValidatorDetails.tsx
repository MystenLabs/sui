// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObject, Base64DataBuffer } from '@mysten/sui.js';
import { useMemo } from 'react';
import { useParams } from 'react-router-dom';

import { DelegationAmount } from './DelegationAmount';

import ErrorResult from '~/components/error-result/ErrorResult';
import { useGetObject } from '~/hooks/useGetObject';
import {
    VALIDATORS_OBJECT_ID,
    type Validator,
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





type ValidatorMetaProps = {
    validatorData: Validator;
}

function ValidatorMeta({validatorData}:ValidatorMetaProps) {
    const validatorName = useMemo(() => {
        return getName(validatorData.fields.metadata.fields.name)
     }, [validatorData]);

    const logo = null

    const validatorPublicKey = useMemo(() => new Base64DataBuffer(
        new Uint8Array(validatorData.fields.metadata.fields.pubkey_bytes)
    ).toString(),
    [validatorData]);

    return (
        <>
        <div className="flex basis-full gap-5 capitalize md:basis-1/4 md:mr-7.5 border-r border-r-gray-45 border-transparent border-solid">
                     <ImageIcon
                        src={logo}
                        alt={validatorName}
                        size="xl"
                    />
                    <div className="mt-1 flex flex-col gap-2.5 md:gap-3.5 pl-2">
                        <Heading
                            as="h1"
                            variant="heading2/bold"
                            color="gray-100"
                        >
                            {validatorName}
                        </Heading>
                    </div>
                </div>
                <div className="basis-full break-all md:basis-2/3 ">
                    <DescriptionItem title="Address">
                        <AddressLink
                            address={validatorData.fields.metadata.fields.sui_address}
                            noTruncate
                        />
                    </DescriptionItem>
                    <DescriptionList>
                   
                            <DescriptionItem title="Public Key">
                                <Text variant="p1/medium" color="gray-90">
                                    {validatorPublicKey}
                                </Text>
                            </DescriptionItem>
                    
                    </DescriptionList>
                </div>
            </>    
    )
}



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
            address: sui_address,

            // TODO: add missing fields
            numberOfDelegators: '--',
            selfStake:0,
            lastEpoch: '--',
            totalRewards: '--',
            networkStakingParticipation: '--',
            votedLastRound: '--',
            tallyingScore: '--',
            lastNarwhalRound: '--',
        };
    }, [validatorData, validatorsData]);


    if (isLoading) {
        return (
            <div className="mt-5 mb-10 flex items-center justify-center">
                <LoadingSpinner />
            </div>
        );
    }

    if (!validatorData || !validator) {
        return (
            <div className="mt-5 mb-10 flex items-center justify-center">
                <ErrorResult id={id} errorMsg="No validator data found" />
            </div>
        );
    }

    return (
        <div className="mt-5 mb-10">
            <div className="flex flex-col flex-nowrap md:flex-row gap-5 md:gap-0">
                <ValidatorMeta validatorData={validatorData}/>
            </div>
            <div className="mt-5 md:mt-8 flex w-full">
            <div className="md:mt-8 flex w-full flex-col gap-5 md:flex-row">
                    <div className="basis-full md:basis-2/5 max-w-[480px]">
                        <Card spacing="lg">
                            <div className="flex max-w-full flex-col flex-nowrap gap-8">
                                <Heading as="div" variant="heading4/semibold" color="steel-darker">
                                    SUI Staked on Validator
                                </Heading>
                                <div className="flex flex-col flex-nowrap gap-8 md:flex-row">
                                    <Stats
                                        label="Staking APY"
                                        tooltip="Coming soon"
                                    >
                                        <Heading
                                            as="h3"
                                            variant="heading2/semibold"
                                            color="steel-darker"
                                        >
                                            {validator.apy ? `${validator.apy}%` : '--'}
                                        </Heading>
                                    </Stats>
                                    <Stats
                                        label="Total Staked"
                                        tooltip="Coming soon"
                                    >
                                        <DelegationAmount
                                            amount={validator.totalStake}
                                            isStats
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
                                            {validator.numberOfDelegators || '--'}
                                        </Heading>
                                    </Stats>
                                    <Stats
                                        label="Delegated Staked"
                                        tooltip="Coming soon"
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
                                        tooltip="Coming soon"
                                    >
                                        <Heading
                                            as="h3"
                                            variant="heading3/semibold"
                                            color="steel-darker"
                                        >
                                            {validator.selfStake ? `${validator.selfStake}%` : '--'}
                                        </Heading>
                                    </Stats>
                                </div>
                            </div>
                        </Card>
                    </div>
                    <div className="basis-full md:basis-1/4">
                        <Card spacing="lg">
                            <div className="flex  max-w-full flex-col flex-nowrap gap-8">
                                <Heading as="div" variant="heading4/semibold" color="steel-darker">
                                    Validator Staking Rewards
                                </Heading>
                                <div className="flex flex-col flex-nowrap gap-8">
                                    <Stats
                                        label="Last Epoch"
                                        tooltip="Coming soon"
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
                                        tooltip="Coming soon"
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
                    <div className="basis-full md:basis-1/3 max-w-[432px]">
                        <Card spacing="lg">
                            <div className="flex  max-w-full flex-col flex-nowrap gap-8">
                                <Heading as="div" variant="heading4/semibold" color="steel-darker">
                                    Network Participation
                                </Heading>
                                <div className="flex flex-col flex-nowrap gap-8">
                                    <div className='flex flex-col flex-nowrap gap-8 md:flex-row'>
                                        <Stats
                                            label="Last Epoch"
                                            tooltip="Coming soon"
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
                                            tooltip="Coming soon"
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
                                    <div className='flex flex-col flex-nowrap gap-8 md:flex-row'>
                                        <Stats
                                            label="Staking Participation"
                                            tooltip="Coming soon"
                                        >
                                            <Heading
                                                as="h3"
                                                variant="heading3/semibold"
                                                color="steel-darker"
                                            >
                                                {validator.networkStakingParticipation}
                                            </Heading>
                                        </Stats>

                                        <Stats
                                            label="Total Reward"
                                            tooltip="Coming soon"
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
                                
                            </div>
                        </Card>
                    </div>
                </div>
            </div>
        </div>
    );
}

export { ValidatorDetails };
