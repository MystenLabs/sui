// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useState, useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';

import { ReactComponent as ContentForwardArrowDark } from '../../assets/SVGIcons/forward-arrow-dark.svg';

import styles from './Pagination.module.css';
function generatePaginationArr(
    startAt: number,
    itemsPerPage: number,
    totalItems: number
) {
    // number of list items to show before truncating
    const range: number = 2;
    const max = Math.ceil((totalItems - 1) / itemsPerPage);
    const maxRange = (Math.floor(startAt / range) + 1) * range;
    // set the min range to be the max range minus the range if it is less than the max - range
    const minRange = startAt <= max - range ? maxRange - range : max - range;
    return {
        max,
        maxRange,
        // generate array of numbers to show in the pagination where the total number of pages is the total tx value / items per page
        // show only the range eg if startAt is 5 and range is 5 then show 5, 6, 7, 8, 9, 10
        listItems: Array.from({ length: max }, (_, i) => i + 1).filter(
            (x: number) => x >= minRange && x <= maxRange
        ),
        range,
    };
}

function Pagination({
    totalTxCount,
    txNum,
}: {
    totalTxCount: number;
    txNum: number;
}) {
    const [searchParams, setSearchParams] = useSearchParams();
    const [pageIndex, setPage] = useState(
        parseInt(searchParams.get('p') || '1', 10) || 1
    );
    const [pagiData, setPagiData] = useState(
        generatePaginationArr(pageIndex, txNum, totalTxCount)
    );

    const changePage = useCallback(
        (pageNum: number) => () => {
            setPage(pageNum);
            setSearchParams({ p: pageNum.toString() });
            setPagiData(generatePaginationArr(pageNum, txNum, totalTxCount));
        },
        [setSearchParams, txNum, totalTxCount]
    );

    return (
        <>
            <nav className={styles.pagination}>
                <ul>
                    {pageIndex > 1 && (
                        <li className={styles.arrow}>
                            <button
                                className={cl([
                                    pageIndex === 1 ? styles.activepag : '',
                                    styles.paginationleft,
                                ])}
                                onClick={changePage(pageIndex - 1)}
                            >
                                <ContentForwardArrowDark />
                            </button>
                        </li>
                    )}
                    {pageIndex > pagiData.range - 1 && (
                        <>
                            <li className="page-item">
                                <button
                                    className="page-link"
                                    onClick={changePage(1)}
                                >
                                    1
                                </button>
                            </li>
                        </>
                    )}
                    {pageIndex > pagiData.range && (
                        <li className="page-item">
                            <button className={styles.paginationdot}>
                                ...
                            </button>
                        </li>
                    )}
                    {pagiData.listItems.map((itm: any, index: number) => (
                        <li className="page-item" key={index}>
                            <button
                                className={
                                    pageIndex === itm ? styles.activepag : ''
                                }
                                onClick={changePage(itm)}
                            >
                                {itm}
                            </button>
                        </li>
                    ))}

                    {pageIndex < pagiData.max - 1 && (
                        <>
                            <li className="page-item">
                                <button
                                    className={cl(
                                        pageIndex === pagiData.max
                                            ? styles.activepag
                                            : '',
                                        styles.paginationdot
                                    )}
                                >
                                    ...
                                </button>
                            </li>

                            <li className="page-item">
                                <button
                                    className={
                                        pageIndex === pagiData.max
                                            ? styles.activepag
                                            : ''
                                    }
                                    onClick={changePage(pagiData.max)}
                                >
                                    {pagiData.max}
                                </button>
                            </li>
                        </>
                    )}
                    {pageIndex < pagiData.max && (
                        <li className={styles.arrow}>
                            <button
                                className="page-link"
                                onClick={changePage(pageIndex + 1)}
                            >
                                <ContentForwardArrowDark />
                            </button>
                        </li>
                    )}
                </ul>
            </nav>
        </>
    );
}

export default Pagination;
