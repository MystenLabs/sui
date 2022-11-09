// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';
import { Link } from 'react-router-dom';

import Icon, { SuiIcons } from '_components/icon';

import type { ReactNode } from 'react';

import st from './Layout.module.scss';

export type LayoutProps = {
    backUrl?: string;
    title: string;
    isSettings?: boolean;
    children: ReactNode | ReactNode[];
};

function Layout({ backUrl, title, children, isSettings }: LayoutProps) {
    return (
        <div className={st.container}>
            <div className={cl(st.header, isSettings && st.isSettings)}>
                {backUrl ? (
                    <Link to={backUrl} className={st.arrowBack}>
                        <Icon icon={SuiIcons.ArrowLeft} />
                    </Link>
                ) : null}
                <div className={st.title}>{title}</div>
            </div>
            {children}
        </div>
    );
}

export default memo(Layout);
