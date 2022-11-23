// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useFormatCoin } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

import st from './DappTxApprovalPage.module.scss';

type TransactionTypeCardProps = {
    label: string;
    content: string | number | null;
    loading: boolean;
};

const GAS_ESTIMATE_LABEL = 'Estimated Gas Fees';

export function TransactionTypeCard({
    label,
    content,
    loading,
}: TransactionTypeCardProps) {
    const isGasEstimate = label === GAS_ESTIMATE_LABEL;
    const [gasEstimate, symbol] = useFormatCoin(
        (isGasEstimate && content) || 0,
        GAS_TYPE_ARG
    );

    const valueContent =
        content === null
            ? '-'
            : isGasEstimate
            ? `${gasEstimate} ${symbol}`
            : content;
    return (
        <>
            <div className={st.label}>{label}</div>
            <div className={st.value}>
                {loading ? <LoadingIndicator /> : valueContent}
            </div>
        </>
    );
}
