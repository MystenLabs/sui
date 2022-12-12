// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import { Text } from '../../text';

import type { ReactNode } from 'react';

import st from './StatsItem.module.scss';

export type CardItemProps = {
    className?: string;
    title: string | ReactNode;
    value: string | ReactNode;
};

function CardItem({ className, title, value }: CardItemProps) {
    return (
        <div
            className={cl(
                className,
                'flex flex-col flex-nowrap max-w-full p-3.5 gap-1.5 flex-1 justify-center items-center'
            )}
        >
            <Text variant="captionSmall" weight="semibold" color="steel-darker">
                {title}
            </Text>

            <div className={st.value}>{value}</div>
        </div>
    );
}

export default memo(CardItem);
