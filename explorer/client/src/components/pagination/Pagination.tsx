// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo, useState, useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';

import { ReactComponent as ContentForwardArrowDark } from '../../assets/SVGIcons/forward-arrow-dark.svg';

import styles from './Pagination.module.css';

const generatePaginationArr = (
    startAt: number,
    itemsPerPage: number,
    totalItems: number
) => {
    // number of list items to show before truncating
    const range: number = 2;
    const max = Math.ceil((totalItems - 1) / itemsPerPage);
    const maxRange = (Math.floor(startAt / range) + 1) * range;
    // set the min range to be the max range minus the range if it is less than the max - range
    const minRange = startAt <= max - range ? maxRange - range : max - range;

    // generate array of numbers to show in the pagination where the total number of pages is the total tx value / items per page
    // show only the range eg if startAt is 5 and range is 5 then show 5, 6, 7, 8, 9, 10
    // generate an array of numbers of length range, starting at startAt and ending at max,
    const rangelength = maxRange <= range + 1 ? range + 3 : range;

    const listItems = Array.from({ length: max }, (_, i) => i + 1).filter(
        (x: number, i) =>
            (x >= minRange && x <= maxRange) || (i + 1 < rangelength && i > 0)
    );

    return {
        max,
        maxRange,
        listItems,
        range,
    };
};

function Pagination({
    totalTxCount,
    txNum,
}: {
    totalTxCount: number;
    txNum: number;
}) {
    const [searchParams, setSearchParams] = useSearchParams();
    const pageIndex = parseInt(searchParams.get('p') || '1', 10) || 1;
    const initData = generatePaginationArr(pageIndex, txNum, totalTxCount);
    const [pagiData, setPagiData] = useState(initData);

    const changePage = useCallback(
        (e: React.MouseEvent<HTMLElement>) => {
            const pageNum = parseInt(e.currentTarget.dataset.pagidata || '0');
            // don't allow page to be less than 1 or equal to current page index
            if (pageNum < 1 || pageNum === pageIndex || pageNum > pagiData.max)
                return;
            setSearchParams({ p: pageNum.toString() });
            setPagiData(generatePaginationArr(pageNum, txNum, totalTxCount));
        },
        [pageIndex, pagiData.max, setSearchParams, txNum, totalTxCount]
    );

    return (
        <>
            <nav className={styles.pagination}>
                <ul>
                    <li
                        className={cl(
                            styles.arrow,
                            pageIndex > 1 ? styles.activearrow : ''
                        )}
                    >
                        <button
                            className={styles.paginationleft}
                            data-pagidata={Math.max(0, pageIndex - 1)}
                            onClick={changePage}
                        >
                            <ContentForwardArrowDark />
                        </button>
                    </li>
                    <li>
                        <button
                            className={pageIndex === 1 ? styles.activepag : ''}
                            onClick={changePage}
                            data-pagidata={1}
                        >
                            1
                        </button>
                    </li>

                    {pageIndex > pagiData.range &&
                        pageIndex > pagiData.range + 1 && (
                            <li className="page-item">
                                <button className={styles.paginationdot}>
                                    ...
                                </button>
                            </li>
                        )}
                    {pagiData.listItems
                        .filter((itm) => itm !== pagiData.max && itm !== 1)
                        .map((itm: any, index: number) => (
                            <li className="page-item" key={index}>
                                <button
                                    className={
                                        pageIndex === itm
                                            ? styles.activepag
                                            : ''
                                    }
                                    data-pagidata={itm}
                                    onClick={changePage}
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
                        </>
                    )}

                    <li className="page-item">
                        <button
                            className={
                                pageIndex === pagiData.max
                                    ? styles.activepag
                                    : ''
                            }
                            data-pagidata={pagiData.max}
                            onClick={changePage}
                        >
                            {pagiData.max}
                        </button>
                    </li>
                    <li
                        className={cl(
                            styles.arrow,
                            pageIndex < pagiData.max ? styles.activearrow : ''
                        )}
                    >
                        <button
                            className="page-link"
                            data-pagidata={pageIndex + 1}
                            onClick={changePage}
                        >
                            <ContentForwardArrowDark />
                        </button>
                    </li>
                </ul>
            </nav>
        </>
    );
}

export default memo(Pagination);
