// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';

import { DelegationAmount } from './DelegationAmount';

import type { Delegator } from './ValidatorDetails';

import Pagination from '~/components/pagination/Pagination';
import { AddressLink } from '~/ui/InternalLink';
import { TableCard } from '~/ui/TableCard';
import { TableHeader } from '~/ui/TableHeader';
import { Text } from '~/ui/Text';

const DELEGATORS_PER_PAGE = 20;

type DelegatorsListProps = {
    delegators: Delegator[];
};

export function DelegatorsList({ delegators }: DelegatorsListProps) {
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
