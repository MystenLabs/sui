// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cn from 'classnames';
import { memo, useMemo } from 'react';

import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector } from '_hooks';
import { createDappStatusSelector } from '_redux/slices/permissions';

import st from './DappStatus.module.scss';

function DappStatus() {
    const activeOrigin = useAppSelector(({ app }) => app.activeOrigin);
    const dappStatusSelector = useMemo(
        () => createDappStatusSelector(activeOrigin),
        [activeOrigin]
    );
    const isConnected = useAppSelector(dappStatusSelector);
    const Component = isConnected ? 'button' : 'span';
    return (
        <Component
            type="button"
            className={cn(st.container, { [st.connected]: isConnected })}
        >
            <Icon
                icon="circle-fill"
                className={cn(st.icon, { [st.connected]: isConnected })}
            />
            <span className={st.label}>
                {isConnected && activeOrigin
                    ? new URL(activeOrigin).hostname
                    : 'Not connected'}
            </span>
            {isConnected ? (
                <Icon icon={SuiIcons.ChevronDown} className={st.chevron} />
            ) : null}
        </Component>
    );
}

export default memo(DappStatus);
