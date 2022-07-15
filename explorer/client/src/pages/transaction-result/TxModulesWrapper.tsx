// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useMemo, useState, useCallback } from 'react';

import TxModuleView from './TxModuleView';

import styles from './TxModuleView.module.css';

type TxModules = {
    title: string;
    content: any[];
};

// TODO: Include Pagination for now use viewMore and viewLess
function TxModuleViewWrapper({ data }: { data: TxModules }) {
    const moduleData = useMemo(() => data, [data]);
    const [viewMore, setVeiwMore] = useState(false);
    const totalModulesCount = moduleData.content.length;
    const numOfMudulesToShow = 3;
    const viewAll = useCallback(() => {
        setVeiwMore(!viewMore);
    }, [viewMore]);
    return (
        <>
            <h3 className={styles.txtitle}>Modules </h3>
            <div className={styles.txmodule}>
                {moduleData.content
                    .slice(0, viewMore ? totalModulesCount : numOfMudulesToShow)
                    .map((item, idx) => (
                        <TxModuleView itm={item} key={idx} />
                    ))}
            </div>
            {totalModulesCount > numOfMudulesToShow && (
                <div className={styles.viewmore}>
                    <button
                        type="button"
                        className={cl([
                            styles.moretxbtn,
                            viewMore && styles.viewless,
                        ])}
                        onClick={viewAll}
                    >
                        {viewMore
                            ? 'View Less'
                            : `View all (${totalModulesCount})`}
                    </button>
                </div>
            )}
        </>
    );
}
export default TxModuleViewWrapper;
