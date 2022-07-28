// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useState, useCallback } from 'react';

import styles from './ViewMore.module.css';

type Props = {
    children: JSX.Element[];
    limitTo?: number;
    label?: string;
};

const DEFAULT_MIN_NUMBER_OF_ITEMS_TO_SHOW = 3;

function ViewMore({ children, limitTo, label }: Props) {
    const [viewMore, setVeiwMore] = useState(false);
    const numberOfListItemsToShow =
        limitTo || DEFAULT_MIN_NUMBER_OF_ITEMS_TO_SHOW;
    const viewAll = useCallback(() => {
        setVeiwMore(!viewMore);
    }, [viewMore]);

    const viewMoreLabel = label
        ? `View ${children.length} ${label}`
        : 'View More';
    return (
        <>
            {children
                .slice(0, viewMore ? children.length : numberOfListItemsToShow)
                .map((elem, _) => elem)}

            {children.length > numberOfListItemsToShow && (
                <div className={styles.viewmore}>
                    <button
                        type="button"
                        className={cl([
                            styles.moretxbtn,
                            viewMore && styles.viewless,
                        ])}
                        onClick={viewAll}
                    >
                        {viewMore ? 'View Less' : viewMoreLabel}
                    </button>
                </div>
            )}
        </>
    );
}

export default ViewMore;
