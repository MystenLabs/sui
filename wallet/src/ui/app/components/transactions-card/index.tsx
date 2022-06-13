// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import AccountAddress from '_components/account-address';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';

import type { ExecutionStatusType, TransactionKindName } from '@mysten/sui.js';

import st from './TransactionsCard.module.scss';

type TxnData = {
    To?: string;
    seq: number;
    txId: string;
    status: ExecutionStatusType;
    txGas: number;
    kind: TransactionKindName | undefined;
    From: string;
};
const truncate = (txt: string, maxLength: number) => {
    if (txt.length < maxLength + 3) {
        return txt;
    }
    const beginningLength = Math.ceil(maxLength / 2);
    const endingLength = maxLength - beginningLength;
    return `${txt.substring(0, beginningLength)}...${txt.substring(
        txt.length - endingLength
    )}`;
};

function TransactionResult({ txresults }: { txresults: TxnData[] }) {
    return (
        <div className={st['tx-container']}>
            <h4>
                Last 5 transaction for <AccountAddress />
            </h4>
            {txresults.map((txn) => (
                <div className={st['card']} key={txn.txId}>
                    <div>
                        Tx:{' '}
                        <ExplorerLink
                            type={ExplorerLinkType.transaction}
                            transactionID={txn.txId}
                            title="View on Sui Explorer"
                            className={st['explorer-link']}
                        >
                            {truncate(txn.txId || '', 20)}
                        </ExplorerLink>
                    </div>
                    <div>TxType: Call </div>
                    <div>
                        {' '}
                        Gas : {txn.txGas} | Status:{' '}
                        <span
                            className={cl(
                                st[txn.status.toLowerCase()],
                                st.txstatus
                            )}
                        >
                            {txn.status === 'success' ? '\u2714' : '\u2716'}{' '}
                        </span>
                    </div>
                    <div>
                        From:
                        <ExplorerLink
                            type={ExplorerLinkType.address}
                            address={txn.From}
                            title="View on Sui Explorer"
                            className={st['explorer-link']}
                        >
                            {truncate(txn.From || '', 20)}
                        </ExplorerLink>
                    </div>
                    {txn.To && (
                        <div>
                            To:
                            <ExplorerLink
                                type={ExplorerLinkType.address}
                                address={txn.From}
                                title="View on Sui Explorer"
                                className={st['explorer-link']}
                            >
                                {truncate(txn.To || '', 20)}
                            </ExplorerLink>
                        </div>
                    )}
                </div>
            ))}
        </div>
    );
}

export default TransactionResult;
