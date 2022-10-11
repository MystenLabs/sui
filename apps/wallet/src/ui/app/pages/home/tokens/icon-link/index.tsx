// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';
import { Link } from 'react-router-dom';

import Icon from '_components/icon';

import type { IconProps } from '_components/icon';

import st from './IconLink.module.scss';

export type IconLinkProps = {
    to: string;
    icon: IconProps['icon'];
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
            <div className={st.iconContainer}>
                <Icon icon={icon} />
            </div>
            <span className={st.text}>{text}</span>
        </Link>
    );
}

export default memo(IconLink);
