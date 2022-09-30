// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useState, useCallback } from 'react';

import placeholdertheme from '../../styles/placeholder.module.css';
import { numberSuffix } from '../../utils/numberUtil';

import styles from './TabFooter.module.css';

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
        loadState?: string;
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
                    {stats && stats.loadState === 'pending' && (
                        <div
                            className={`${placeholdertheme.placeholder} ${styles.placeholder}`}
                        />
                    )}
                    {stats && stats.loadState === 'loaded' && (
                        <>
                            {typeof stats.count === 'number'
                                ? numberSuffix(stats.count)
                                : stats.count}{' '}
                            {stats.stats_text}
                        </>
                    )}

                    {stats && stats.loadState === 'fail' && <></>}

                    {paging && (
                        <div>
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
