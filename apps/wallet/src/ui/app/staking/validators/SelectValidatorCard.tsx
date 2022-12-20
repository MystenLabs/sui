// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObject } from '@mysten/sui.js';
import { useState, useMemo } from 'react';

import { getName, STATE_OBJECT } from '../usePendingDelegation';
import { ValidatorListItem } from './ValidatorListItem';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Text } from '_app/shared/text';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useGetObject } from '_hooks';

import type { ValidatorState } from '../ValidatorDataTypes';

export function SelectValidatorCard() {
    const [selectedValidator, setSelectedValidator] = useState<null | string>(
        null
    );
    const { data, isLoading, isError } = useGetObject(STATE_OBJECT);

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

                return {
                    name: getName(rawName),
                    apy: APY > 0 ? APY : 'N/A',
                    logo: null,
                    address: av.fields.metadata.fields.sui_address,
                };
            })
            .sort((a, b) => (a.name > b.name ? 1 : -1));
    }, [validatorsData]);

    const selectValidator = (address: string) => {
        setSelectedValidator((state) => (state !== address ? address : null));
    };

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
        <BottomMenuLayout className="flex flex-col w-full items-center m-0 p-0">
            <Content className="flex flex-col w-full items-center">
                <div className="flex items-start w-full mb-7">
                    <Text
                        variant="subtitle"
                        weight="medium"
                        color="steel-darker"
                    >
                        Select a validator to start staking SUI.
                    </Text>
                </div>

                {validators &&
                    validators.map(({ name, logo, address, apy }) => (
                        <div
                            className="cursor-pointer w-full relative"
                            key={address}
                            onClick={() => selectValidator(address)}
                        >
                            <ValidatorListItem
                                name={name}
                                address={address}
                                apy={apy}
                                logo={logo}
                                selected={selectedValidator === address}
                            />
                        </div>
                    ))}
            </Content>

            <Menu stuckClass="staked-cta" className="w-full px-0 pb-0 mx-0">
                {selectedValidator && (
                    <Button
                        size="large"
                        mode="primary"
                        href={`/stake/new?address=${encodeURIComponent(
                            selectedValidator
                        )}`}
                        className="w-full"
                    >
                        Select Amount
                        <Icon
                            icon={SuiIcons.ArrowRight}
                            className="text-captionSmall text-white font-normal"
                        />
                    </Button>
                )}
            </Menu>
        </BottomMenuLayout>
    );
}
