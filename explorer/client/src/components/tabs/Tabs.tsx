// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'classnames';
import { useCallback, useState } from 'react';

import styles from './Tabs.module.css';

type Props = {
    title?: string;
    children: JSX.Element[];
    selected?: number;
};

function Tabs({ title, children, selected }: Props) {
    const [activeTab, setActivetab] = useState(selected || 0);
    const selectActiveTab = useCallback((e: React.MouseEvent<HTMLElement>) => {
        if (e.currentTarget.dataset.activetab)
            setActivetab(parseInt(e.currentTarget.dataset.activetab));
    }, []);
    return (
        <div className={styles.tabs}>
            <ul className={styles.tablist}>
                {children.map((elem, index) => {
                    return (
                        <li
                            className={cl([
                                index === activeTab && styles.selected,
                                styles.tab,
                            ])}
                            key={index}
                            data-activetab={index}
                            onClick={selectActiveTab}
                        >
                            {elem.props.title}
                        </li>
                    );
                })}
            </ul>
            <div className={styles.tabContent}>{children[activeTab]}</div>
        </div>
    );
}

export default Tabs;
