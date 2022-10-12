// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import Loading from '_components/loading';
import { useAppSelector, useFullscreenGuard } from '_hooks';
import { getNavIsVisible } from '_redux/slices/app';

import type { ReactNode } from 'react';

import st from './Layout.module.scss';

export type PageLayoutProps = {
    limitToPopUpSize?: boolean;
    forceFullscreen?: boolean;
    children: ReactNode | ReactNode[];
    className?: string;
};

function PageLayout({
    limitToPopUpSize = false,
    forceFullscreen = false,
    children,
    className,
}: PageLayoutProps) {
    const guardLoading = useFullscreenGuard(forceFullscreen);
    const isNavVisible = useAppSelector(getNavIsVisible);
    return (
        <Loading loading={guardLoading}>
            <div
                className={cl(
                    st.container,
                    className,
                    limitToPopUpSize ? st.forcedPopupSize : st.dynamicSize,
                    {
                        [st.navHidden]: !isNavVisible,
                    }
                )}
            >
                {children}
            </div>
        </Loading>
    );
}

export default memo(PageLayout);
