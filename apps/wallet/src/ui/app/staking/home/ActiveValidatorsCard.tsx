// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer, isSuiObject, isSuiMoveObject } from '@mysten/sui.js';
import BigNumber from 'bignumber.js';
// import { cva, type VariantProps } from 'class-variance-authority';
import { useMemo, useCallback, useState } from 'react';

import { Text } from '../../shared/Text';
import { IconTooltip } from '../../shared/Tooltip';
import { ImageIcon } from '../../shared/image-icon';
import Alert from '_components/alert';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useGetObject, useMiddleEllipsis } from '_hooks';

import type { ValidatorState, Validator } from './ValidatorDataTypes';

const TRUNCATE_MAX_LENGTH = 10;
const TRUNCATE_PREFIX_LENGTH = 6;
const APY_TOOLTIP = 'Annual Percentage Yield';
const VALIDATORS_OBJECT_ID = '0x05';

const textDecoder = new TextDecoder();

type ValidatorListItemProp = {
    name: string;
    logo?: string | null;
    address: string;
    // APY can be N/A
    apy: number | string;
};
function ValidatorListItem({
    name,
    address,
    apy,
    logo,
}: ValidatorListItemProp) {
    const truncatedAddress = useMiddleEllipsis(
        address,
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );
    return (
        <div
            className="flex justify-between w-full hover:bg-sui/10 py-3.5 px-2.5  rounded-lg"
            role="button"
        >
            <div className="flex gap-2.5">
                <div className="mb-2">
                    <ImageIcon src={logo} alt={name} />
                </div>

                <div className="flex flex-col gap-1.5 capitalize">
                    <Text variant="body" weight="semibold" color="gray-90">
                        {name}
                    </Text>
                    <ExplorerLink
                        type={ExplorerLinkType.address}
                        address={address}
                        className="text-steel-dark no-underline font-mono font-medium "
                        showIcon={false}
                    >
                        {truncatedAddress}
                    </ExplorerLink>
                </div>
            </div>
            <div className="flex gap-0.5 items-center ">
                <Text variant="body" weight="semibold" color="steel-darker">
                    {apy}
                </Text>
                <div className="flex gap-0.5 items-baseline leading-none">
                    <Text
                        variant="subtitleSmall"
                        weight="medium"
                        color="steel-dark"
                    >
                        % APY
                    </Text>
                    <div className="text-steel items-baseline flex text-subtitle h-3">
                        <IconTooltip tip={`${APY_TOOLTIP}`} placement="top" />
                    </div>
                </div>
            </div>
        </div>
    );
}

export function processValidators(
    set: Validator[],
    totalStake: bigint,
    current_epoch: number
) {
    return set.map((av) => {
        const rawName = av.fields.metadata.fields.name;

        const name = textDecoder.decode(
            new Base64DataBuffer(rawName).getData()
        );

        /*
         const {
            sui_balance,
            starting_epoch,

            delegation_token_supply,
        } = av.fields.delegation_staking_pool.fields;
        const num_epochs_participated = current_epoch - starting_epoch;
        const APY =
            (1 +
                (sui_balance - delegation_token_supply.fields.value) /
                    delegation_token_supply.fields.value) ^
            (365 / num_epochs_participated - 1);*/

        return {
            name: name,
            address: av.fields.metadata.fields.sui_address,
            pubkeyBytes: av.fields.metadata.fields.pubkey_bytes,
            stake: av.fields.stake_amount,
            stakePercent: getStakePercent(av.fields.stake_amount, totalStake),
            delegation_count: av.fields.delegation_count || 0,
            apy: 'N/A',
            logo: null,
        };
    });
}

export const getStakePercent = (stake: bigint, total: bigint): number => {
    const bnStake = new BigNumber(stake.toString());
    const bnTotal = new BigNumber(total.toString());
    return bnStake.div(bnTotal).multipliedBy(100).toNumber();
};

export function ActiveValidatorsCard() {
    const { data, isLoading, isError } = useGetObject(VALIDATORS_OBJECT_ID);

    const [selectedValidator, setSelectedValidator] = useState<false | object>(
        false
    );
    const selectStakingValidator = useCallback(
        (e: React.MouseEvent<HTMLElement>) => {
            setSelectedValidator(e.currentTarget.dataset);
        },
        []
    );

    const validatorsData =
        data && isSuiObject(data.details) && isSuiMoveObject(data.details.data)
            ? (data.details.data.fields as ValidatorState)
            : null;

    const totalStake =
        validatorsData?.validators.fields.total_validator_stake || 0n;

    const validators = useMemo(
        () =>
            validatorsData
                ? processValidators(
                      validatorsData.validators.fields.active_validators,
                      totalStake,
                      validatorsData.epoch
                  ).sort((a, b) => (a.name > b.name ? 1 : -1))
                : null,
        [totalStake, validatorsData]
    );

    if (isError || (!isLoading && !validators)) {
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

    if (isLoading) {
        return (
            <div className="p-2 w-full flex justify-center item-center h-full">
                <LoadingIndicator />
            </div>
        );
    }

    return (
        <div className="flex flex-col w-full items-center">
            <div className="flex items-start w-full mb-7">
                <Text variant="subtitle" weight="medium" color="steel-darker">
                    Select a validator to start staking SUI.
                </Text>
            </div>

            {validators &&
                validators.map((v) => (
                    <div
                        className="cursor-pointer w-full"
                        key={v.address}
                        onClick={selectStakingValidator}
                    >
                        <ValidatorListItem
                            name={v.name}
                            address={v.address}
                            apy={v.apy}
                            logo={v?.logo}
                        />
                    </div>
                ))}
        </div>
    );
}
