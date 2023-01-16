// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObject, type ValidatorsFields } from '@mysten/sui.js';
import { useState, useMemo } from 'react';

import { STATE_OBJECT, getName } from '../usePendingDelegation';
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

export function SelectValidatorCard() {
    const [selectedValidator, setSelectedValidator] = useState<null | string>(
        null
    );
    const { data, isLoading, isError } = useGetObject(STATE_OBJECT);

    const validatorsData =
        data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === 'moveObject'
            ? (data.details.data.fields as ValidatorsFields)
            : null;

    const selectValidator = (address: string) => {
        setSelectedValidator((state) => (state !== address ? address : null));
    };

    const validatorList = useMemo(() => {
        if (!validatorsData) return [];
        return validatorsData.validators.fields.active_validators.sort((a, b) =>
            getName(a.fields.metadata.fields.name) >
            getName(b.fields.metadata.fields.name)
                ? 1
                : -1
        );
    }, [validatorsData]);

    if (isLoading) {
        return (
            <div className="p-2 w-full flex justify-center items-center h-full">
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

                {validatorsData &&
                    validatorList.map((validators) => (
                        <div
                            className="cursor-pointer w-full relative"
                            key={validators.fields.metadata.fields.sui_address}
                            onClick={() =>
                                selectValidator(
                                    validators.fields.metadata.fields
                                        .sui_address
                                )
                            }
                        >
                            <ValidatorListItem
                                validator={validators}
                                selected={
                                    selectedValidator ===
                                    validators.fields.metadata.fields
                                        .sui_address
                                }
                                epoch={+validatorsData.epoch}
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
