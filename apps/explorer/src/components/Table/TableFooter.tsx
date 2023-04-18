// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight12 } from '@mysten/icons';

import { Link } from '~/ui/Link';
import {
    Pagination,
    type PaginationResponse,
    type usePaginationStack,
} from '~/ui/Pagination';
import { Text } from '~/ui/Text';
import { numberSuffix } from '~/utils/numberUtil';

interface Props {
    label: string;
    count?: number;
    disablePagination?: boolean;
    pagination: ReturnType<typeof usePaginationStack>;
    data?: PaginationResponse<any>;
    limit: number;
    onLimitChange(value: number): void;
    href: string;
}

export function TableFooter({
    data,
    label,
    pagination,
    disablePagination,
    count,
    limit,
    onLimitChange,
    href,
}: Props) {
    return (
        <div className="flex items-center justify-between">
            {disablePagination ? (
                <>
                    <Link to={href} after={<ArrowRight12 />}>
                        More {label}
                    </Link>
                    <Text variant="body/medium" color="steel-dark">
                        {count ? numberSuffix(count) : '-'} {label}
                    </Text>
                </>
            ) : (
                <>
                    <Pagination {...pagination.props(data)} />

                    <div className="flex items-center gap-4">
                        <Text variant="body/medium" color="steel-dark">
                            {count ? numberSuffix(count) : '-'} {label}
                        </Text>

                        <select
                            className="form-select rounded-md border border-gray-45 px-3 py-2 pr-8 text-bodySmall font-medium leading-[1.2] text-steel-dark shadow-button"
                            value={limit}
                            onChange={(e) =>
                                onLimitChange(Number(e.target.value))
                            }
                        >
                            <option value={20}>20 Per Page</option>
                            <option value={40}>40 Per Page</option>
                            <option value={60}>60 Per Page</option>
                        </select>
                    </div>
                </>
            )}
        </div>
    );
}
