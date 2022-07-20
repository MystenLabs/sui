// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useState } from 'react';

import Pagination from '../../components/pagination/Pagination';

export default function PaginationLogic({
    results,
    viewComponentFn,
    itemsPerPage,
    canVaryItemsPerPage = false,
    stats,
}: {
    results: any;
    viewComponentFn: Function;
    itemsPerPage: number;
    canVaryItemsPerPage?: boolean;
    stats?: {
        stats_text: string;
        count: number;
    };
}) {
    const count = results.length;

    const MAX_ITEMS_PER_PAGE = 60;

    const [pageIndex, setPageIndex] = useState(0);

    const [pageLength, setPageLength] = useState(itemsPerPage);

    const objectSample = results.slice(
        pageIndex * itemsPerPage,
        (pageIndex + 1) * itemsPerPage
    );

    // If the total number of items is at most the max items per page, the ability to vary items per page number is removed:
    const allowVaryItemsPerPage =
        count <= MAX_ITEMS_PER_PAGE ? false : canVaryItemsPerPage;

    return (
        <>
            {viewComponentFn(objectSample)}
            <Pagination
                totalItems={count}
                itemsPerPage={pageLength}
                updateItemsPerPage={
                    allowVaryItemsPerPage ? setPageLength : undefined
                }
                currentPage={pageIndex}
                onPagiChangeFn={setPageIndex}
                stats={stats}
            />
        </>
    );
}
