// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin } from '@mysten/sui.js';
import React, { useCallback, useEffect, useState } from 'react';

import { ReactComponent as OpenIcon } from '../../../assets/SVGIcons/12px/ShowNHideDown.svg';
import { ReactComponent as ClosedIcon } from '../../../assets/SVGIcons/12px/ShowNHideRight.svg';
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
        <div
            className={isOpen ? styles.openedgroup : styles.closedgroup}
            data-testid="ownedcoinsummary"
        >
            <div onClick={switchIsOpen} className={styles.summary}>
                <div className={styles.coinname}>
                    <div>{isOpen ? <OpenIcon /> : <ClosedIcon />}</div>
                    <div
                        className={styles.oneline}
                        data-testid="ownedcoinlabel"
                    >
                        {handleCoinType(coinLabel)}
                    </div>
                </div>
                <div
                    className={styles.objcount}
                    data-testid="ownedcoinobjcount"
                >
                    {subObjList.length}
                </div>
                <div className={styles.balance} data-testid="ownedcoinbalance">
                    {subObjList[0]._isCoin &&
                    subObjList.every((el) => el.balance !== undefined)
                        ? `${subObjList.reduce(
                              (prev, current) => prev + current.balance!,
                              Coin.getZero()
                          )}`
                        : ''}
                </div>
            </div>
            {isOpen && (
                <div className={styles.openbody}>
                    {subObjList.map((subObj, index) => (
                        <div key={index} className={styles.singlecoin}>
                            <div className={styles.openrow}>
                                <div className={styles.label}>Object ID</div>
                                <div
                                    className={`${styles.oneline} ${styles.value}`}
                                >
                                    <Longtext
                                        text={subObj.id}
                                        category="objects"
                                    />
                                </div>
                            </div>
                            <div className={styles.openrow}>
                                <div className={styles.label}>Balance</div>
                                <div className={styles.value}>
                                    {subObj.balance?.toString()}
                                </div>
                            </div>
                        </div>
                    ))}
                </div>
            )}
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
