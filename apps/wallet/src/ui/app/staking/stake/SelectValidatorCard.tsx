// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiObject, isSuiMoveObject } from '@mysten/sui.js';
import cl from 'classnames';
import { useCallback, useState, useMemo } from 'react';

// TODO: replace with useGetObject
import { useGetObjectData } from '_app/hooks/useGetObjectData';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { ImageIcon } from '_app/shared/image-icon';
import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import Alert from '_components/alert';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useMiddleEllipsis } from '_hooks';

import type { ValidatorState } from '../ValidatorDataTypes';

const TRUNCATE_MAX_LENGTH = 10;
const TRUNCATE_PREFIX_LENGTH = 6;
const APY_TOOLTIP = 'Annual Percentage Yield';
const VALIDATORS_OBJECT_ID = '0x5';

type ValidatorListItemProp = {
    name: string;
    logo?: string | null;
    address: string;
    selected?: boolean;
    // APY can be N/A
    apy: number | string;
};
function ValidatorListItem({
    name,
    address,
    apy,
    logo,
    selected,
}: ValidatorListItemProp) {
    const truncatedAddress = useMiddleEllipsis(
        address,
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );

    return (
        <div
            className={cl(
                selected && 'bg-sui/10 px-2.5',
                'flex justify-between w-full hover:bg-sui/10 py-3.5 px-1.5 hover:px-2.5 rounded-lg group'
            )}
            role="button"
        >
            <div className="flex gap-2.5">
                <div className="mb-2 relative">
                    {selected && (
                        <Icon
                            icon={SuiIcons.CheckFill}
                            className="absolute text-success text-heading6 translate-x-4 -translate-y-1 rounded-full bg-white"
                        />
                    )}

                    <ImageIcon src={logo} alt={name} />
                </div>

                <div className="flex flex-col gap-1.5 capitalize">
                    <Text variant="body" weight="semibold" color="gray-90">
                        {name}
                    </Text>
                    <ExplorerLink
                        type={ExplorerLinkType.address}
                        address={address}
                        className={cl(
                            selected && 'text-hero-dark',
                            'text-steel-dark no-underline font-mono font-medium group-hover:text-hero-dark'
                        )}
                        showIcon={false}
                    >
                        {truncatedAddress}
                    </ExplorerLink>
                </div>
            </div>
            <div className="flex gap-0.5 items-center ">
                {typeof apy !== 'string' && (
                    <Text variant="body" weight="semibold" color="steel-darker">
                        {apy}
                    </Text>
                )}
                <div className="flex gap-0.5 leading-none">
                    <Text
                        variant="subtitleSmall"
                        weight="medium"
                        color="steel-dark"
                    >
                        {typeof apy === 'string' ? apy : '% APY'}
                    </Text>
                    <div className="text-steel items-baseline text-subtitle h-3 flex opacity-0 group-hover:opacity-100">
                        <IconTooltip tip={`${APY_TOOLTIP}`} placement="top" />
                    </div>
                </div>
            </div>
        </div>
    );
}

// TODO: to unblock, replace with getName from usePendingDelegation
function getName(rawName: string | number[]) {
    let name: string;

    if (Array.isArray(rawName)) {
        name = String.fromCharCode(...rawName);
    } else {
        name = Buffer.from(rawName, 'base64').toString();
        if (!/^[A-Z-_.\s0-9]+$/i.test(name)) {
            name = rawName;
        }
    }
    return name;
}

export function SelectValidatorCard() {
    const [selectedValidator, setSelectedValidator] = useState<false | string>(
        false
    );
    const { data, isLoading, isError } = useGetObjectData(VALIDATORS_OBJECT_ID);

    const validatorsData =
        data && isSuiObject(data.details) && isSuiMoveObject(data.details.data)
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

    const selectValidator = useCallback(
        (address: string) => {
            setSelectedValidator((state) =>
                state !== address ? address : false
            );
        },
        [setSelectedValidator]
    );

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
                        className=" w-full"
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
