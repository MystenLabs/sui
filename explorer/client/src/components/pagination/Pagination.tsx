// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo, useState, useCallback, useEffect } from 'react';

import TabFooter from '../../components/tabs/TabFooter';

import styles from './Pagination.module.css';

function Pagination({
    totalItems,
    itemsPerPage,
    updateItemsPerPage,
    currentPage = 0,
    onPagiChangeFn,
    stats,
}: {
    totalItems: number;
    itemsPerPage: number;
    updateItemsPerPage?: (index: number) => void;
    currentPage: number;
    onPagiChangeFn?: (index: number) => void;
    stats?: {
        stats_text: string;
        count: number;
    };
}) {
    const [pageIndex, setPageIndex] = useState(currentPage - 1);
    const NUMBER_OF_TX_PER_PAGE_OPTIONS = [20, 40, 60];

    useEffect(() => {
        if (onPagiChangeFn) {
            onPagiChangeFn(pageIndex + 1);
        }
    }, [pageIndex, onPagiChangeFn]);

    const finalPageNo =
        Math.floor(totalItems / itemsPerPage) +
        (totalItems % itemsPerPage !== 0 ? 1 : 0);

    const handleBtnClick = useCallback(
        (pageIndex: number) => () => setPageIndex(pageIndex),
        []
    );

    const handleBackClick = useCallback(
        () => pageIndex - 1 >= 0 && setPageIndex(pageIndex - 1),
        [pageIndex]
    );

    const handleNextClick = useCallback(
        () =>
            (pageIndex + 1) * itemsPerPage < totalItems &&
            setPageIndex(pageIndex + 1),
        [pageIndex, itemsPerPage, totalItems]
    );

    const pageLengthChange = useCallback(
        (event: React.ChangeEvent<HTMLSelectElement>) => {
            if (updateItemsPerPage) {
                const selectedNum = parseInt(event.target.value);
                updateItemsPerPage(selectedNum);
            }
        },
        [updateItemsPerPage]
    );

    const FirstButton = (
        <button
            className={
                pageIndex === 0
                    ? `${styles.nointeract} ${styles.gone}`
                    : styles.btncontainer
            }
            id="backBtn"
            onClick={handleBackClick}
            disabled={pageIndex === 0}
        >
            &larr;
        </button>
    );

    const LastButton = (
        <button
            id="nextBtn"
            className={
                pageIndex === finalPageNo - 1
                    ? `${styles.nointeract} ${styles.gone}`
                    : styles.btncontainer
            }
            disabled={pageIndex === finalPageNo - 1}
            onClick={handleNextClick}
        >
            &rarr;
        </button>
    );

    const RHSInfo = (
        <div className={styles.rhs}>
            {stats && <TabFooter stats={stats} />}
            {updateItemsPerPage && (
                <select value={itemsPerPage} onChange={pageLengthChange}>
                    {NUMBER_OF_TX_PER_PAGE_OPTIONS.map((item) => (
                        <option value={item} key={item}>
                            {item} Per Page
                        </option>
                    ))}
                </select>
            )}
        </div>
    );

    // When Total Number of Pages at most 5, list all always:

    if (finalPageNo > 1 && finalPageNo <= 5) {
        return (
            <div className={styles.footer}>
                <div>
                    {FirstButton}
                    {Array(finalPageNo)
                        .fill(0)
                        .map((_: number, arrayIndex: number) => (
                            <button
                                key={`page-${arrayIndex}`}
                                className={
                                    pageIndex === arrayIndex
                                        ? styles.pagenumber
                                        : styles.btncontainer
                                }
                                id="firstBtn"
                                onClick={handleBtnClick(arrayIndex)}
                                disabled={pageIndex === arrayIndex}
                            >
                                {arrayIndex + 1}
                            </button>
                        ))}
                    {LastButton}
                </div>
                {RHSInfo}
            </div>
        );
    }

    return (
        <div className={styles.footer}>
            <div>
                {finalPageNo > 1 && (
                    <>
                        {FirstButton}
                        <button
                            className={
                                pageIndex === 0
                                    ? styles.pagenumber
                                    : styles.btncontainer
                            }
                            id="firstBtn"
                            onClick={handleBtnClick(0)}
                            disabled={pageIndex === 0}
                        >
                            1
                        </button>

                        <button
                            className={
                                pageIndex === 1
                                    ? styles.pagenumber
                                    : pageIndex <= 2 ||
                                      pageIndex >= finalPageNo - 3
                                    ? styles.btncontainer
                                    : styles.secondbtn
                            }
                            id="secondBtn"
                            onClick={handleBtnClick(1)}
                            disabled={pageIndex === 1}
                        >
                            2
                        </button>

                        {pageIndex > 2 && (
                            <button
                                className={
                                    pageIndex > 2
                                        ? styles.nointeract
                                        : styles.nointeractsecond
                                }
                            >
                                ...
                            </button>
                        )}

                        {pageIndex > 1 && pageIndex < finalPageNo - 2 && (
                            <button className={styles.pagenumber}>
                                {pageIndex + 1}
                            </button>
                        )}

                        {pageIndex >= 1 && pageIndex < finalPageNo - 3 && (
                            <button
                                className={styles.nextbtnnumber}
                                onClick={handleBtnClick(pageIndex + 1)}
                            >
                                {pageIndex + 2}
                            </button>
                        )}

                        {pageIndex < finalPageNo - 3 && (
                            <button
                                className={
                                    pageIndex < finalPageNo - 4
                                        ? styles.nointeract
                                        : styles.nointeractsecond
                                }
                            >
                                ...
                            </button>
                        )}

                        <button
                            className={
                                pageIndex === finalPageNo - 2
                                    ? styles.pagenumber
                                    : pageIndex <= 2 ||
                                      pageIndex >= finalPageNo - 3
                                    ? styles.btncontainer
                                    : styles.secondbtn
                            }
                            id="secondLastBtn"
                            onClick={handleBtnClick(finalPageNo - 2)}
                            disabled={pageIndex === finalPageNo - 2}
                        >
                            {finalPageNo - 1}
                        </button>
                        <button
                            id="lastBtn"
                            disabled={pageIndex === finalPageNo - 1}
                            onClick={handleBtnClick(finalPageNo - 1)}
                            className={
                                pageIndex === finalPageNo - 1
                                    ? styles.pagenumber
                                    : styles.btncontainer
                            }
                        >
                            {finalPageNo}
                        </button>

                        {LastButton}
                    </>
                )}
            </div>
            {RHSInfo}
        </div>
    );
}

export default memo(Pagination);
