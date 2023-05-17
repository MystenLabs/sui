// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { Toaster as ToasterLib } from 'react-hot-toast';
import { useLocation } from 'react-router-dom';

import { Portal } from '../Portal';
import { useMenuIsOpen } from '_components/menu/hooks';
import { useAppSelector } from '_hooks';
import { getNavIsVisible } from '_redux/slices/app';

export type ToasterProps = {
    bottomNavEnabled: boolean;
};
const commonToastClasses =
    '!px-0 !py-1 !text-pBodySmall !font-medium !rounded-2lg !shadow-notification';
export function Toaster({ bottomNavEnabled }: ToasterProps) {
    const { pathname } = useLocation();
    const isExtraNavTabsVisible = pathname.startsWith('/apps');
    const menuVisible = useMenuIsOpen();
    const isBottomNavVisible = useAppSelector(getNavIsVisible);
    const includeBottomNavSpace =
        !menuVisible && isBottomNavVisible && bottomNavEnabled;
    const includeExtraBottomNavSpace =
        includeBottomNavSpace && isExtraNavTabsVisible;
    return (
        <Portal containerId="toaster-portal-container">
            <ToasterLib
                containerClassName={cl(
                    '!absolute !z-[99999] transition-all',
                    includeBottomNavSpace &&
                        'mb-[var(--sizing-navigation-placeholder-height)]',
                    includeExtraBottomNavSpace && '!bottom-10'
                )}
                position="bottom-center"
                toastOptions={{
                    loading: {
                        icon: null,
                        className: `${commonToastClasses} !bg-steel !text-white`,
                    },
                    error: {
                        icon: null,
                        className: `${commonToastClasses} !border !border-solid !border-issue-dark/20 !bg-issue-light !text-issue-dark`,
                    },
                    success: {
                        icon: null,
                        className: `${commonToastClasses} !border !border-solid !border-success-dark/20 !bg-success-light !text-success-dark`,
                    },
                }}
            />
        </Portal>
    );
}
