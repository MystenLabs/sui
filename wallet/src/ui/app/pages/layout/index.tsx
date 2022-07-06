// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import Loading from '_components/loading';
import { useFullscreenGuard } from '_hooks';

import type { ReactNode } from 'react';

import st from './Layout.module.scss';

export type PageLayoutProps = {
    limitToPopUpSize?: boolean;
    forceFullscreen?: boolean;
    children: ReactNode | ReactNode[];
};

function PageLayout({
    limitToPopUpSize = false,
    forceFullscreen = false,
    children,
}: PageLayoutProps) {
    const guardLoading = useFullscreenGuard(forceFullscreen);
    return (
        <Loading loading={guardLoading}>
            <div
                className={cl(st.container, {
                    [st.forcedPopupDimensions]: limitToPopUpSize,
                })}
            >
                {children}
            </div>
        </Loading>
    );
}

export default memo(PageLayout);
