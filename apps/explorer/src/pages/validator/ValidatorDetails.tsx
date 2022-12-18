// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    isSuiObject,
    isSuiMoveObject,
    SUI_TYPE_ARG,
    Base64DataBuffer,
} from '@mysten/sui.js';
import { useMemo, useState } from 'react';
import { useParams, Navigate } from 'react-router-dom';

import { ReactComponent as ArrowRight } from '~/assets/SVGIcons/12px/ArrowRight.svg';
import ErrorResult from '~/components/error-result/ErrorResult';
import Pagination from '~/components/pagination/Pagination';
import { CoinFormat, useFormatCoin } from '~/hooks/useFormatCoin';
import { useGetObject } from '~/hooks/useGetObject';
import {
    VALIDATORS_OBJECT_ID,
    type ValidatorState,
} from '~/pages/validator/ValidatorDataTypes';
import { Button } from '~/ui/Button';
import { DescriptionList, DescriptionItem } from '~/ui/DescriptionList';
import { Heading } from '~/ui/Heading';
import { ImageIcon } from '~/ui/ImageIcon';
import { AddressLink } from '~/ui/InternalLink';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { TableCard } from '~/ui/TableCard';
import { TableHeader } from '~/ui/TableHeader';
import { Text } from '~/ui/Text';
import { getName } from '~/utils/getName';
import { getStakedPercent } from '~/utils/getStakedPercent';

const DELEGATORS_PER_PAGE = 20;

function DelegationAmount({ amount }: { amount?: bigint | number }) {
    const [formattedAmount, symbol] = useFormatCoin(
        amount,
        SUI_TYPE_ARG,
        CoinFormat.FULL
    );

    return (
        <div className="flex h-full items-center gap-1">
            <div className="flex items-baseline gap-0.5 text-gray-90">
                <Text variant="body">{formattedAmount}</Text>
                <Text variant="subtitleSmall">{symbol}</Text>
            </div>
        </div>
    );
}

type Delegators = {
    delegator: string;
    sui_amount: bigint;
    share: number;
    type: string;
}[];

function ValidatorDetails() {
    const { id } = useParams();

    const { data, isLoading } = useGetObject(VALIDATORS_OBJECT_ID);

    const validatorsData =
        data && isSuiObject(data.details) && isSuiMoveObject(data.details.data)
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
        const {
            sui_balance,
            starting_epoch,
            pending_delegations,
            delegation_token_supply,
        } = validatorData.fields.delegation_staking_pool.fields;

        const num_epochs_participated = validatorsData.epoch - starting_epoch;

        const APY = Math.pow(
            1 +
                (sui_balance - delegation_token_supply.fields.value) /
                    delegation_token_supply.fields.value,
            365 / num_epochs_participated - 1
        );

        const totalStaked = pending_delegations.reduce(
            (acc, fields) => (acc += BigInt(fields.fields.sui_amount || 0n)),
            0n
        );

        return {
            name: getName(name),
            pubkeyBytes: new Base64DataBuffer(
                new Uint8Array(pubkey_bytes)
            ).toString(),
            suiAddress: sui_address,
            apy: APY > 0 ? APY : 'N/A',
            logo: null,
            link: null,
            address: sui_address,
            totalStaked,
            delegators: Object.values(
                [...pending_delegations].reduce(
                    (acc, delegation) => {
                        return {
                            ...acc,
                            [`${delegation.fields.delegator}`]: {
                                delegator: delegation.fields.delegator,
                                type: delegation.type,
                                sui_amount:
                                    BigInt(
                                        delegation.fields?.sui_amount || 0n
                                    ) +
                                    BigInt(
                                        acc[`${delegation.fields.delegator}`]
                                            ?.sui_amount || 0n
                                    ),
                                share: getStakedPercent(
                                    BigInt(
                                        delegation.fields?.sui_amount || 0n
                                    ) +
                                        BigInt(
                                            acc[
                                                `${delegation.fields.delegator}`
                                            ]?.sui_amount || 0n
                                        ),
                                    totalStaked
                                ),
                            },
                        };
                    },
                    {} as {
                        [delegator: string]: {
                            sui_amount: bigint;
                            delegator: string;
                            type: string;
                            share: number;
                        };
                    }
                )
            ),
        };
    }, [validatorData, validatorsData]);

    const delegatorsData = useMemo(
        () =>
            validator
                ? validator.delegators.sort((a, b) =>
                      b.share > a.share ? 1 : -1
                  )
                : null,
        [validator]
    );

    if (!id) {
        return <Navigate to="/validators" />;
    }

    if (isLoading) {
        return (
            <div className="mt-5 mb-10">
                <LoadingSpinner />
            </div>
        );
    }

    if (!validator) {
        return <ErrorResult id={id} errorMsg="No validator data found" />;
    }

    return (
        <div className="mt-5 mb-10">
            <div className="flex gap-5 capitalize">
                <ImageIcon
                    src={validator.logo}
                    alt={validator.name}
                    variant="square"
                    size="xl"
                />
                <div className="flex flex-col gap-3.5">
                    <Heading
                        as="h1"
                        variant="heading2"
                        weight="bold"
                        color="gray-100"
                    >
                        {validator.name}
                    </Heading>

                    <Button type="button" variant="outline">
                        Stake Coins{' '}
                        <ArrowRight fill="currentColor" className="ml-2" />
                    </Button>
                </div>
            </div>

            <div className="mt-8 break-all">
                <TableHeader>Details</TableHeader>
                <DescriptionList>
                    <DescriptionItem
                        title={
                            <Text
                                variant="body"
                                weight="medium"
                                color="gray-80"
                            >
                                Public Key
                            </Text>
                        }
                    >
                        <Text variant="body" weight="medium" color="gray-90">
                            {validator.pubkeyBytes}
                        </Text>
                    </DescriptionItem>

                    <DescriptionItem
                        title={
                            <Text
                                variant="body"
                                weight="medium"
                                color="gray-80"
                            >
                                Sui Address
                            </Text>
                        }
                    >
                        <Text
                            variant="body"
                            weight="semibold"
                            color="gray-90"
                            mono
                        >
                            {validator.suiAddress}
                        </Text>
                    </DescriptionItem>
                </DescriptionList>
            </div>

            {!!delegatorsData?.length && (
                <DelegatorsList delegators={delegatorsData} />
            )}
        </div>
    );
}

interface DelegatorsListProps {
    delegators: Delegators;
}

function DelegatorsList({ delegators }: DelegatorsListProps) {
    const [delegatorsPageNumber, setDelegatorsPageNumber] = useState(1);
    const [delegatorsPerPage, setDelegatorsPerPage] =
        useState(DELEGATORS_PER_PAGE);
    const totalDelegatorsCount = delegators.length;
    const columns = [
        {
            headerLabel: 'Staker Address',
            accessorKey: 'address',
        },
        {
            headerLabel: 'Amount',
            accessorKey: 'amount',
        },
        {
            headerLabel: 'Share',
            accessorKey: 'share',
        },
    ];

    const stats = {
        stats_text: 'Delegators',
        count: totalDelegatorsCount,
    };

    return (
        <div className="mt-16">
            <TableHeader>Delegators</TableHeader>
            <TableCard
                data={delegators
                    .filter(
                        (_, index) =>
                            index >=
                                (delegatorsPageNumber - 1) *
                                    delegatorsPerPage &&
                            index < delegatorsPageNumber * delegatorsPerPage
                    )
                    .map(({ delegator, sui_amount, share }) => {
                        return {
                            share: (
                                <Text
                                    variant="bodySmall"
                                    color="steel-darker"
                                    weight="medium"
                                >
                                    {share} %
                                </Text>
                            ),
                            amount: <DelegationAmount amount={sui_amount} />,

                            address: (
                                <AddressLink address={delegator} noTruncate />
                            ),
                        };
                    })}
                columns={columns}
            />
            {totalDelegatorsCount > delegatorsPerPage && (
                <Pagination
                    totalItems={totalDelegatorsCount}
                    itemsPerPage={delegatorsPerPage}
                    currentPage={delegatorsPageNumber}
                    onPagiChangeFn={setDelegatorsPageNumber}
                    updateItemsPerPage={setDelegatorsPerPage}
                    stats={stats}
                />
            )}
        </div>
    );
}

export { ValidatorDetails };
