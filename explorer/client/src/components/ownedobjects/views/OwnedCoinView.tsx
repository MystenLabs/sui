// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin } from '@mysten/sui.js';
import React, { useCallback, useEffect, useState } from 'react';

import { ReactComponent as ContentIcon } from '../../../assets/SVGIcons/closed-content.svg';
import { handleCoinType } from '../../../utils/stringUtils';
import Longtext from '../../longtext/Longtext';
import Pagination from '../../pagination/Pagination';
import { type DataType, ITEMS_PER_PAGE } from '../OwnedObjectConstants';

import styles from './OwnedObjects.module.css';

export default function OwnedCoinView({ results }: { results: DataType }) {
    const CLOSED_TYPE_STRING = '';

    const [openedType, setOpenedType] = useState(CLOSED_TYPE_STRING);

    const [currentPage, setCurrentPage] = useState(1);

    const openThisType = useCallback(
        (thisType: string) => () => {
            setOpenedType(thisType);
        },
        []
    );

    const goBack = useCallback(() => setOpenedType(CLOSED_TYPE_STRING), []);

    const uniqueTypes = Array.from(new Set(results.map(({ Type }) => Type)));

    // Switching the page closes any open group:
    useEffect(() => {
        setOpenedType(CLOSED_TYPE_STRING);
    }, [currentPage]);

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
                        .map((typeV) => {
                            const subObjList = results.filter(
                                ({ Type }) => Type === typeV
                            );
                            return (
                                <div
                                    key={typeV}
                                    className={
                                        openedType === typeV
                                            ? styles.openedgroup
                                            : styles.closedgroup
                                    }
                                >
                                    <div
                                        onClick={
                                            openedType === typeV
                                                ? goBack
                                                : openThisType(typeV)
                                        }
                                        className={styles.summary}
                                    >
                                        <div>
                                            <span className={styles.icon}>
                                                <ContentIcon />
                                            </span>
                                        </div>
                                        <div>{handleCoinType(typeV)}</div>
                                        <div>{subObjList.length}</div>
                                        <div>
                                            {subObjList[0]._isCoin &&
                                            subObjList.every(
                                                (el) => el.balance !== undefined
                                            )
                                                ? `${subObjList.reduce(
                                                      (prev, current) =>
                                                          prev.add(
                                                              current.balance!
                                                          ),
                                                      Coin.getZero()
                                                  )}`
                                                : ''}
                                        </div>
                                    </div>
                                    {openedType === typeV &&
                                        subObjList.map((subObj, index) => (
                                            <React.Fragment
                                                key={`${typeV}${index}`}
                                            >
                                                <tr>
                                                    <td />
                                                    <td>Object ID</td>
                                                    <td colSpan={2}>
                                                        <Longtext
                                                            text={subObj.id}
                                                            category="objects"
                                                            isCopyButton={false}
                                                        />
                                                    </td>
                                                    <td />
                                                </tr>
                                                <tr>
                                                    <td />
                                                    <td>Balance</td>
                                                    <td colSpan={2}>
                                                        {subObj.balance?.toString()}
                                                    </td>
                                                    <td />
                                                </tr>
                                            </React.Fragment>
                                        ))}
                                </div>
                            );
                        })}
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
