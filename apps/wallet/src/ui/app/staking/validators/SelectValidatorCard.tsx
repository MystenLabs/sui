// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { calculateAPY } from '@mysten/core';
import cl from 'classnames';
import { useState, useMemo } from 'react';

import { calculateStakeShare } from '../calculateStakeShare';
import { useSystemState } from '../useSystemState';
import { ValidatorListItem } from './ValidatorListItem';
import { Content, Menu } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Text } from '_app/shared/text';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';

type SortKeys = 'name' | 'stakeShare' | 'apy';
const sortKeys: Record<SortKeys, string> = {
    name: 'Name',
    stakeShare: 'Stake Share',
    apy: 'APY',
};

export function SelectValidatorCard() {
    const [selectedValidator, setSelectedValidator] = useState<null | string>(
        null
    );
    const [sortKey, setSortKey] = useState<SortKeys>('stakeShare');
    const [sortAscending, setSortAscending] = useState(true);
    const { data, isLoading, isError } = useSystemState();

    const selectValidator = (address: string) => {
        setSelectedValidator((state) => (state !== address ? address : null));
    };

    const handleSortByKey = (key: SortKeys) => {
        if (key === sortKey) {
            setSortAscending(!sortAscending);
        }
        setSortKey(key);
    };

    const totalStake = useMemo(() => {
        if (!data) return 0;
        return data.activeValidators.reduce(
            (acc, curr) => (acc += BigInt(curr.stakingPoolSuiBalance)),
            0n
        );
    }, [data]);

    const validatorList = useMemo(() => {
        if (!data) return [];

        const sortedAsc = data.activeValidators
            .map((validator) => ({
                name: validator.name,
                address: validator.suiAddress,
                apy: calculateAPY(validator, +data.epoch),
                stakeShare: calculateStakeShare(
                    BigInt(validator.stakingPoolSuiBalance),
                    BigInt(totalStake)
                ),
            }))
            .sort((a, b) => {
                if (sortKey === 'name') {
                    return a[sortKey].localeCompare(b[sortKey], 'en', {
                        sensitivity: 'base',
                        numeric: true,
                    });
                }
                return a[sortKey] - b[sortKey];
            });
        return sortAscending ? sortedAsc : sortedAsc.reverse();
    }, [sortAscending, sortKey, data, totalStake]);

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
        <div className="flex flex-col w-full -my-5">
            <Content className="flex flex-col w-full items-center">
                <div className="flex flex-col w-full items-center -top-5 bg-white sticky pt-5 pb-2.5 z-50 mt-0">
                    <div className="flex items-start w-full mb-2">
                        <Text
                            variant="subtitle"
                            weight="medium"
                            color="steel-darker"
                        >
                            Sort by:
                        </Text>
                        <div className="flex items-center ml-2 gap-1.5">
                            {Object.entries(sortKeys).map(([key, value]) => {
                                return (
                                    <button
                                        key={key}
                                        className="bg-transparent border-0 p-0 flex gap-1 cursor-pointer"
                                        onClick={() =>
                                            handleSortByKey(key as SortKeys)
                                        }
                                    >
                                        <Text
                                            variant="caption"
                                            weight="medium"
                                            color={
                                                sortKey === key
                                                    ? 'hero'
                                                    : 'steel-darker'
                                            }
                                        >
                                            {value}
                                        </Text>
                                        {sortKey === key && (
                                            <Icon
                                                icon={SuiIcons.ArrowLeft}
                                                className={cl(
                                                    'text-captionSmall font-thin  text-hero',
                                                    sortAscending
                                                        ? 'rotate-90'
                                                        : '-rotate-90'
                                                )}
                                            />
                                        )}
                                    </button>
                                );
                            })}
                        </div>
                    </div>
                    <div className="flex items-start w-full">
                        <Text
                            variant="subtitle"
                            weight="medium"
                            color="steel-darker"
                        >
                            Select a validator to start staking SUI.
                        </Text>
                    </div>
                </div>
                <div className="flex items-start flex-col w-full mt-1 flex-1">
                    {data &&
                        validatorList.map((validator) => (
                            <div
                                className="cursor-pointer w-full relative"
                                key={validator.address}
                                onClick={() =>
                                    selectValidator(validator.address)
                                }
                            >
                                <ValidatorListItem
                                    selected={
                                        selectedValidator === validator.address
                                    }
                                    validatorAddress={validator.address}
                                    value={
                                        sortKey === 'name'
                                            ? '-'
                                            : `${validator[sortKey]}%`
                                    }
                                />
                            </div>
                        ))}
                </div>
            </Content>
            {selectedValidator && (
                <Menu
                    stuckClass="staked-cta"
                    className="w-full px-0 pb-5 mx-0 -bottom-5"
                >
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
                </Menu>
            )}
        </div>
    );
}
