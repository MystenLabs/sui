// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import st from './LoadingIndicator.module.scss';

export type LoadingIndicatorProps = {
    className?: string;
};

const LoadingIndicator = ({ className }: LoadingIndicatorProps) => {
    return <span className={cl(st.spinner, className)} />;
};

export default LoadingIndicator;
