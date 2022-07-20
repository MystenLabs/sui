// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useState } from 'react';

import Pagination from '../../components/pagination/Pagination';

export default function PaginationLogic({
    results,
    viewComponentFn,
    itemsPerPage,
    stats,
}: {
    results: any;
    viewComponentFn: Function;
    itemsPerPage: number;
    stats?: {
        stats_text: string;
        count: number;
    };
}) {
    const count = results.length;

    const [pageIndex, setPageIndex] = useState(0);

    const objectSample = results.slice(
        pageIndex * itemsPerPage,
        (pageIndex + 1) * itemsPerPage
    );

    return (
        <>
            {viewComponentFn(objectSample)}
            <Pagination
                totalItems={count}
                itemsPerPage={itemsPerPage}
                currentPage={pageIndex}
                onPagiChangeFn={setPageIndex}
            />
        </>
    );
}
