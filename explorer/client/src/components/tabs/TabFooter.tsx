// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useState, useCallback } from 'react';

import { numberSuffix } from '../../utils/numberUtil';

import styles from './Tabs.module.css';

const NUMBER_OF_TX_PER_PAGE_OPTIONS = [20, 40, 60];
// Update this footer now accept React.ReactElement as a child
function TabFooter({
    stats,
    children,
    paging,
    itemsPerPageChange,
}: {
    children?: React.ReactElement;
    stats?: {
        count: number | string;
        stats_text: string;
    };
    paging?: number;
    itemsPerPageChange?: Function;
}) {
    const [currentItemsPerPage, setPurrentItemsPerPage] = useState(
        paging || NUMBER_OF_TX_PER_PAGE_OPTIONS[0]
    );
    const selectChange = useCallback(
        (event: React.ChangeEvent<HTMLSelectElement>) => {
            const selectedNum = parseInt(event.target.value);
            setPurrentItemsPerPage(selectedNum);
            itemsPerPageChange && itemsPerPageChange(selectedNum);
        },
        [itemsPerPageChange]
    );
    return (
        <section className={styles.tabsfooter}>
            {children ? (
                [...(Array.isArray(children) ? children : [children])]
            ) : (
                <></>
            )}
            {(stats || paging) && (
                <div className={styles.stats}>
                    {stats && (
                        <p>
                            {typeof stats.count === 'number'
                                ? numberSuffix(stats.count)
                                : stats.count}{' '}
                            {stats.stats_text}
                        </p>
                    )}
                    {paging && (
                        <div className={styles.pagedropdown}>
                            <select
                                value={currentItemsPerPage}
                                onChange={selectChange}
                            >
                                {NUMBER_OF_TX_PER_PAGE_OPTIONS.map((item) => (
                                    <option value={item} key={item}>
                                        {item} Per Page
                                    </option>
                                ))}
                            </select>
                        </div>
                    )}
                </div>
            )}
        </section>
    );
}

export default TabFooter;
