// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import LoadingIndicator from '_components/loading/LoadingIndicator';

import st from './TransactionRequest.module.scss';

type TransactionTypeCardProps = {
    label: string;
    content: string | number | null;
    loading: boolean;
};

export function TransactionTypeCard({
    label,
    content,
    loading,
}: TransactionTypeCardProps) {
    return (
        <>
            <div className={st.label}>{label}</div>
            <div className={st.value}>
                {loading ? <LoadingIndicator /> : content}
            </div>
        </>
    );
}
