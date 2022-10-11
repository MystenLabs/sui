// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo, useState, useCallback, useEffect, useRef } from 'react';

import { numberSuffix } from '../../utils/numberUtil';

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
    const NUMBER_OF_TX_PER_PAGE_OPTIONS = [20, 40, 60];

    // Connects pageIndex to input page value

    const [pageIndex, setPageIndex] = useState(currentPage - 1);
    const previousPageIndex = useRef(pageIndex);

    useEffect(() => {
        setPageIndex(currentPage - 1);
    }, [currentPage]);

    useEffect(() => {
        if (pageIndex !== previousPageIndex.current) {
            previousPageIndex.current = pageIndex;
            onPagiChangeFn?.(pageIndex + 1);
        }
    }, [pageIndex, onPagiChangeFn]);

    const finalPageNo =
        Math.floor(totalItems / itemsPerPage) +
        (totalItems % itemsPerPage !== 0 ? 1 : 0);

    // Connects inputted items per page to selected page length

    const pageLengthChange = useCallback(
        (event: React.ChangeEvent<HTMLSelectElement>) => {
            if (updateItemsPerPage) {
                const selectedNum = parseInt(event.target.value);
                updateItemsPerPage(selectedNum);
            }
        },
        [updateItemsPerPage]
    );

    // Handle Button clicks

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

    // Mini-components shared across the different views

    const BackButton = (
        <button
            className={
                pageIndex === 0
                    ? `${styles.nointeract} ${styles.gone}`
                    : styles.btncontainer
            }
            data-testid="backBtn"
            onClick={handleBackClick}
            disabled={pageIndex === 0}
        >
            &larr;
        </button>
    );

    const NextButton = (
        <button
            data-testid="nextBtn"
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

    const Stats = stats ? (
        <div>
            {typeof stats.count === 'number'
                ? numberSuffix(stats.count)
                : stats.count}{' '}
            {stats.stats_text}
        </div>
    ) : (
        <></>
    );

    const PageLengthSelect = updateItemsPerPage ? (
        <select value={itemsPerPage} onChange={pageLengthChange}>
            {NUMBER_OF_TX_PER_PAGE_OPTIONS.map((item) => (
                <option value={item} key={item}>
                    {item} Per Page
                </option>
            ))}
        </select>
    ) : (
        <></>
    );

    const IndexZeroButton = (label: string = '1') => (
        <button
            className={
                pageIndex === 0 ? styles.pagenumber : styles.btncontainer
            }
            data-testid="firstBtn"
            onClick={handleBtnClick(0)}
            disabled={pageIndex === 0}
        >
            {label}
        </button>
    );

    const FinalPageButton = (
        finalPageNo: number,
        label: string = String(finalPageNo)
    ) => (
        <button
            data-testid="lastBtn"
            disabled={pageIndex === finalPageNo - 1}
            onClick={handleBtnClick(finalPageNo - 1)}
            className={
                pageIndex === finalPageNo - 1
                    ? styles.pagenumber
                    : styles.btncontainer
            }
        >
            {label}
        </button>
    );

    // View when Total Number of Pages is one, which is an empty div:

    if (finalPageNo <= 1) return <div />;

    // View when Total Number of Pages is at most 5, all values are listed:

    if (finalPageNo <= 5) {
        return (
            <div className={styles.under6footer}>
                <div>
                    {BackButton}
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
                                onClick={handleBtnClick(arrayIndex)}
                                disabled={pageIndex === arrayIndex}
                            >
                                {arrayIndex + 1}
                            </button>
                        ))}
                    {NextButton}
                </div>
                <div className={styles.rhs}>
                    {Stats}
                    {PageLengthSelect}
                </div>
            </div>
        );
    }

    // View when more than 5 pages in Desktop:

    const desktopPagination = (
        <div>
            {BackButton}
            {IndexZeroButton()}

            <button
                className={
                    pageIndex === 1 ? styles.pagenumber : styles.btncontainer
                }
                data-testid="secondBtn"
                onClick={handleBtnClick(1)}
                disabled={pageIndex === 1}
            >
                2
            </button>

            {pageIndex > 2 && (
                <button className={styles.nointeract}>...</button>
            )}

            {pageIndex > 1 && pageIndex < finalPageNo - 2 && (
                <button className={styles.pagenumber}>{pageIndex + 1}</button>
            )}

            {pageIndex >= 1 && pageIndex < finalPageNo - 3 && (
                <button
                    className={styles.btncontainer}
                    onClick={handleBtnClick(pageIndex + 1)}
                >
                    {pageIndex + 2}
                </button>
            )}

            {pageIndex < finalPageNo - 4 && (
                <button className={styles.nointeract}>...</button>
            )}

            <button
                className={
                    pageIndex === finalPageNo - 2
                        ? styles.pagenumber
                        : styles.btncontainer
                }
                data-testid="secondLastBtn"
                onClick={handleBtnClick(finalPageNo - 2)}
                disabled={pageIndex === finalPageNo - 2}
            >
                {finalPageNo - 1}
            </button>

            {FinalPageButton(finalPageNo)}

            {NextButton}
        </div>
    );

    // View when more than 5 pages in mobile:

    const mobilePagination = (
        <div>
            <div className={styles.mobiletoprow}>
                {IndexZeroButton()}
                <button className={styles.basecontainer}>
                    Page {pageIndex + 1}
                </button>
                {FinalPageButton(finalPageNo)}
            </div>
            <div className={styles.mobilebottomrow}>
                {BackButton}
                {NextButton}
            </div>
            <div className={styles.rhs}>{Stats}</div>
        </div>
    );

    return (
        <>
            <div className={styles.mobilefooter}>{mobilePagination}</div>
            <div className={styles.desktopfooter}>
                {desktopPagination}
                <div className={styles.rhs}>
                    {Stats}
                    {PageLengthSelect}
                </div>
            </div>
        </>
    );
}

export default memo(Pagination);
