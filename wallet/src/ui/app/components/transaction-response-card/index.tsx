// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import Icon, { SuiIcons } from '_components/icon';
import { GAS_SYMBOL } from '_redux/slices/sui-objects/Coin';

import type { ReactNode } from 'react';

import st from './TxResponse.module.scss';

type TxResponseProps = {
    children?: ReactNode;
    gasFee?: number;
    date?: string | null;
    address?: string;
    errorMessage?: string | null;
    status: 'success' | 'failure';
};

//TODO extend this card to include other transaction types
function TxResponseCard({
    children,
    gasFee,
    date,
    address,
    status,
    errorMessage,
}: TxResponseProps) {
    const SuccessCard = (
        <>
            <div className={st.successIcon}>
                <Icon
                    icon={SuiIcons.ArrowLeft}
                    className={cl(st.arrowActionIcon, st.angledArrow)}
                />
            </div>
            <div className={st.successText}>Successfully Sent!</div>
        </>
    );

    const failedCard = (
        <>
            <div className={st.failedIcon}>
                <div className={st.iconBg}>
                    <Icon icon={SuiIcons.Close} className={cl(st.close)} />
                </div>
            </div>
            <div className={st.failedText}>NFT Transfer Failed</div>
            <div className={st.errorMessage}>{errorMessage}</div>
        </>
    );

    return (
        <>
            <div className={st.nftResponse}>
                {status === 'success' ? SuccessCard : failedCard}
                <div className={cl(st.responseCard)}>
                    {children}
                    <div className={st.txInfo}>
                        <div className={st.txInfoLabel}>Your Wallet</div>
                        <div
                            className={cl(
                                st.txInfoValue,
                                status === 'success' ? st.success : st.failed
                            )}
                        >
                            {address}
                        </div>
                    </div>

                    {gasFee && (
                        <div className={st.txFees}>
                            <div className={st.txInfoLabel}>
                                Estimated Gas Fee
                            </div>
                            <div className={st.walletInfoValue}>
                                {gasFee} {GAS_SYMBOL}
                            </div>
                        </div>
                    )}
                    {date && (
                        <div className={st.txDate}>
                            <div className={st.txInfoLabel}>Date</div>
                            <div className={st.walletInfoValue}>{date}</div>
                        </div>
                    )}
                </div>
            </div>
        </>
    );
}

export default TxResponseCard;
