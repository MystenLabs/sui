// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin } from '@mysten/sui.js';
import { useCallback, useEffect, useState } from 'react';

import { ReactComponent as OpenIcon } from '../../../assets/SVGIcons/12px/ShowNHideDown.svg';
import { ReactComponent as ClosedIcon } from '../../../assets/SVGIcons/12px/ShowNHideRight.svg';
import Longtext from '../../longtext/Longtext';
import Pagination from '../../pagination/Pagination';
import { type DataType, ITEMS_PER_PAGE } from '../OwnedObjectConstants';

import styles from '../styles/OwnedCoin.module.css';

import { useFormatCoin } from '~/hooks/useFormatCoin';

function CoinItem({
    id,
    balance,
    coinType,
}: {
    id: string;
    balance?: bigint | null;
    coinType?: string | null;
}) {
    const [formattedBalance] = useFormatCoin(balance, coinType);

    return (
        <div className={styles.singlecoin}>
            <div className={styles.openrow}>
                <div className={styles.label}>Object ID</div>
                <div className={`${styles.oneline} ${styles.value}`}>
                    <Longtext text={id} category="objects" />
                </div>
            </div>
            <div className={styles.openrow}>
                <div className={styles.label}>Balance</div>
                <div className={styles.value}>{formattedBalance}</div>
            </div>
        </div>
    );
}

function SingleCoinView({
    results,
    coinType,
    currentPage,
}: {
    results: DataType;
    coinType: string;
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

    const subObjList = results.filter(({ Type }) => Type === coinType);

    const totalBalance =
        subObjList[0]._isCoin &&
        subObjList.every((el) => el.balance !== undefined)
            ? subObjList.reduce(
                  (prev, current) => prev + current.balance!,
                  Coin.getZero()
              )
            : null;

    const extractedCoinType = Coin.getCoinType(coinType);

    const [formattedTotalBalance, symbol] = useFormatCoin(
        totalBalance,
        extractedCoinType
    );

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
                        {symbol}
                    </div>
                </div>
                <div
                    className={styles.objcount}
                    data-testid="ownedcoinobjcount"
                >
                    {subObjList.length}
                </div>
                <div className={styles.balance} data-testid="ownedcoinbalance">
                    {formattedTotalBalance}
                </div>
            </div>

            {isOpen && (
                <div className={styles.openbody}>
                    {subObjList.map((subObj) => (
                        <CoinItem
                            key={subObj.id}
                            id={subObj.id}
                            coinType={extractedCoinType}
                            balance={subObj.balance}
                        />
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
                        .map((type) => (
                            <SingleCoinView
                                key={type}
                                results={results}
                                coinType={type}
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
