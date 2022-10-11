// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo } from 'react';

import LoadingIndicator from './LoadingIndicator';

import type { ReactNode } from 'react';

type LoadingProps = {
    loading: boolean;
    children: ReactNode | ReactNode[];
    className?: string;
};

const Loading = ({ loading, children, className }: LoadingProps) => {
    return loading ? (
        className ? (
            <div className={className}>
                <LoadingIndicator />
            </div>
        ) : (
            <LoadingIndicator />
        )
    ) : (
        <>{children}</>
    );
};

export default memo(Loading);
