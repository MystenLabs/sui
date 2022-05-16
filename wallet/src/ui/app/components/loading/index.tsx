// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo } from 'react';

import LoadingIndicator from './LoadingIndicator';

import type { ReactNode } from 'react';

type LoadingProps = {
    loading: boolean;
    children: ReactNode | ReactNode[];
};

const Loading = ({ loading, children }: LoadingProps) => {
    return loading ? <LoadingIndicator /> : <>{children}</>;
};

export default memo(Loading);
