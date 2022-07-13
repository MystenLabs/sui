// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useState, useCallback } from 'react';

import styles from './PaginationWrapper.module.css';

export default function PaginationWrapper({
    results,
    viewComponentFn,
}: {
    results: any;
    viewComponentFn: Function;
}) {
    const [pageIndex, setPageIndex] = useState(0);

    const ITEMS_PER_PAGE = 6;

    const FINAL_PAGE_NO =
        Math.floor(results.length / ITEMS_PER_PAGE) +
        (results.length % ITEMS_PER_PAGE !== 0 ? 1 : 0);

    const objectSample = results.slice(
        pageIndex * ITEMS_PER_PAGE,
        (pageIndex + 1) * ITEMS_PER_PAGE
    );

    const handleFirstClick = useCallback(() => setPageIndex(0), []);
    const handleSecondClick = useCallback(() => setPageIndex(1), []);
    const handleThirdClick = useCallback(() => setPageIndex(2), []);

    const handleBackClick = useCallback(
        () => pageIndex - 1 >= 0 && setPageIndex(pageIndex - 1),
        [pageIndex]
    );

    const handleNextClick = useCallback(
        () =>
            (pageIndex + 1) * ITEMS_PER_PAGE < results.length &&
            setPageIndex(pageIndex + 1),
        [pageIndex, results.length]
    );

    const handleLastClick = useCallback(
        () => setPageIndex(FINAL_PAGE_NO - 1),
        [FINAL_PAGE_NO]
    );
    const handlePenultimateClick = useCallback(
        () => setPageIndex(FINAL_PAGE_NO - 2),
        [FINAL_PAGE_NO]
    );

    return (
        <>
            {viewComponentFn(objectSample)}
            {FINAL_PAGE_NO > 1 && (
                <>
                    <span className={pageIndex === 0 ? styles.gone : ''}>
                        <button
                            className={styles.btncontainer}
                            id="backBtn"
                            onClick={handleBackClick}
                            disabled={pageIndex === 0}
                        >
                            &larr;
                        </button>

                        <button
                            className={styles.btncontainer}
                            id="firstBtn"
                            onClick={handleFirstClick}
                            disabled={pageIndex === 0}
                        >
                            1
                        </button>
                    </span>

                    {FINAL_PAGE_NO > 2 && pageIndex > 1 && (
                        <button
                            className={styles.btncontainer}
                            id="secondBtn"
                            onClick={handleSecondClick}
                            disabled={pageIndex === 1}
                        >
                            2
                        </button>
                    )}
                    {pageIndex > 2 && FINAL_PAGE_NO > 5 && (
                        <button className={styles.btncontainer}>...</button>
                    )}
                    {FINAL_PAGE_NO === 5 && pageIndex > 2 && (
                        <button
                            className={styles.btncontainer}
                            id="middleBtn"
                            onClick={handleThirdClick}
                        >
                            3
                        </button>
                    )}
                    {FINAL_PAGE_NO === 5 && pageIndex > 3 && (
                        <button
                            className={styles.btncontainer}
                            id="middleBtn"
                            onClick={handlePenultimateClick}
                        >
                            4
                        </button>
                    )}

                    <button className={styles.pagenumber}>
                        {pageIndex + 1}
                    </button>

                    {FINAL_PAGE_NO > 2 && pageIndex === 0 && (
                        <button
                            className={styles.btncontainer}
                            id="secondBtn"
                            onClick={handleSecondClick}
                        >
                            2
                        </button>
                    )}

                    {pageIndex < FINAL_PAGE_NO - 3 && FINAL_PAGE_NO > 5 && (
                        <button className={styles.btncontainer}>...</button>
                    )}
                    {FINAL_PAGE_NO === 5 && pageIndex < 2 && (
                        <button
                            className={styles.btncontainer}
                            id="middleBtn"
                            onClick={handleThirdClick}
                        >
                            3
                        </button>
                    )}

                    {FINAL_PAGE_NO > 3 && pageIndex < FINAL_PAGE_NO - 2 && (
                        <button
                            className={styles.btncontainer}
                            id="secondLastBtn"
                            onClick={handlePenultimateClick}
                        >
                            {FINAL_PAGE_NO - 1}
                        </button>
                    )}

                    <span
                        className={
                            pageIndex === FINAL_PAGE_NO - 1 ? styles.gone : ''
                        }
                    >
                        <button
                            id="lastBtn"
                            disabled={pageIndex === FINAL_PAGE_NO - 1}
                            onClick={handleLastClick}
                            className={styles.btncontainer}
                        >
                            {FINAL_PAGE_NO}
                        </button>
                        <button
                            id="nextBtn"
                            className={styles.btncontainer}
                            disabled={pageIndex === FINAL_PAGE_NO - 1}
                            onClick={handleNextClick}
                        >
                            &rarr;
                        </button>
                    </span>
                </>
            )}
        </>
    );
}
