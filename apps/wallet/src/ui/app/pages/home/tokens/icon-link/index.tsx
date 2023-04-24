// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'classnames';
import { memo } from 'react';
import { Link } from 'react-router-dom';

import { Text } from '_app/shared/text';

import type { ReactNode } from 'react';

import st from './IconLink.module.scss';

export type IconLinkProps = {
    to: string;
    icon: ReactNode;
    disabled?: boolean;
    text: string;
};

function IconLink({ to, icon, disabled = false, text }: IconLinkProps) {
    return (
        <Link
            to={to}
            className={cl(st.container, { [st.disabled]: disabled })}
            tabIndex={disabled ? -1 : undefined}
        >
            <div className={cl(disabled ? 'text-gray-60' : 'text-hero-dark')}>
                {icon}
            </div>
            <Text
                color={disabled ? 'gray-60' : 'hero-dark'}
                weight="semibold"
                variant="bodySmall"
            >
                {text}
            </Text>
        </Link>
    );
}

export default memo(IconLink);
