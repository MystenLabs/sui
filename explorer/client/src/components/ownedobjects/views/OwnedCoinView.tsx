// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin } from '@mysten/sui.js';
import React, { useCallback, useEffect, useState } from 'react';

import { ReactComponent as ContentIcon } from '../../../assets/SVGIcons/closed-content.svg';
import { handleCoinType } from '../../../utils/stringUtils';
import Longtext from '../../longtext/Longtext';
import Pagination from '../../pagination/Pagination';
import { type DataType, ITEMS_PER_PAGE } from '../OwnedObjectConstants';

import styles from '../styles/OwnedCoin.module.css';

function SingleCoinView({
    results,
    coinLabel,
    currentPage,
}: {
    results: DataType;
    coinLabel: string;
    currentPage: number;
}) {
    const [isOpen, setIsOpen] = useState(false);

    const switchIsOpen = useCallback(
        () => setIsOpen((prevOpen) => !prevOpen),
        []
    );

    // Switching the page closes any open group:
    useEffect(() => {
        setIsOpen(false);
    }, [currentPage]);

    const subObjList = results.filter(({ Type }) => Type === coinLabel);

    return (
        <div className={isOpen ? styles.openedgroup : styles.closedgroup}>
            <div onClick={switchIsOpen} className={styles.summary}>
                <div className={isOpen ? styles.openicon : styles.closedicon}>
                    <ContentIcon />
                </div>
                <div>{handleCoinType(coinLabel)}</div>
                <div>{subObjList.length}</div>
                <div>
                    {subObjList[0]._isCoin &&
                    subObjList.every((el) => el.balance !== undefined)
                        ? `${subObjList.reduce(
                              (prev, current) => prev.add(current.balance!),
                              Coin.getZero()
                          )}`
                        : ''}
                </div>
                <div />
            </div>
            <div className={styles.openbody}>
                {isOpen &&
                    subObjList.map((subObj, index) => (
                        <React.Fragment key={index}>
                            <div className={styles.objectid}>
                                <div />
                                <div>Object ID</div>
                                <div>
                                    <Longtext
                                        text={subObj.id}
                                        category="objects"
                                        isCopyButton={false}
                                    />
                                </div>
                                <div />
                            </div>
                            <div className={styles.balance}>
                                <div />
                                <div>Balance</div>
                                <div>{subObj.balance?.toString()}</div>
                                <div />
                            </div>
                        </React.Fragment>
                    ))}
            </div>
        </div>
    );
}

export default function OwnedCoinView({ results }: { results: DataType }) {
    const [currentPage, setCurrentPage] = useState(1);

    const uniqueTypes = Array.from(new Set(results.map(({ Type }) => Type)));

    return (
        <>
            <div id="groupCollection" className={styles.groupview}>
                <div className={styles.firstrow}>
                    <div>Type</div>
                    <div>Objects</div>
                    <div>Balance</div>
                </div>
                <div className={styles.body}>
                    {uniqueTypes
                        .slice(
                            (currentPage - 1) * ITEMS_PER_PAGE,
                            currentPage * ITEMS_PER_PAGE
                        )
                        .map((typeV) => (
                            <SingleCoinView
                                key={typeV}
                                results={results}
                                coinLabel={typeV}
                                currentPage={currentPage}
                            />
                        ))}
                </div>
            </div>
            <Pagination
                totalItems={uniqueTypes.length}
                itemsPerPage={ITEMS_PER_PAGE}
                currentPage={currentPage}
                onPagiChangeFn={setCurrentPage}
            />
        </>
    );
}
