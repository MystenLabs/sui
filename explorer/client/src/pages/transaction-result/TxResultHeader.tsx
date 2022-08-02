// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'classnames';

import { ReactComponent as CallTypeIcon } from '../../assets/SVGIcons/Call.svg';
import { ReactComponent as PublishTypeIcon } from '../../assets/SVGIcons/Publish.svg';
import { ReactComponent as TransferObjectTypeIcon } from '../../assets/SVGIcons/TransferObject.svg';
import { ReactComponent as ContentFailedStatus } from '../../assets/SVGIcons/failed.svg';
import { ReactComponent as ContentSuccessStatus } from '../../assets/SVGIcons/success.svg';
import GoBack from '../../components/goback/GoBack';
import Longtext from '../../components/longtext/Longtext';

import type { ExecutionStatusType, TransactionKindName } from '@mysten/sui.js';

import styles from './TxResultHeader.module.css';

// Show the <- Go Back button
// Display the transaction Type (e.g. TransferCoin,TransferObject, etc.)
// Display the transaction ID and Copy button and Status (e.g. Pending, Success, Failure)
type TxResultState = {
    txId: string;
    status: ExecutionStatusType;
    txKindName: TransactionKindName;
    error?: string;
};

function TxAddressHeader({ data }: { data: TxResultState }) {
    const TxTransferTypeIcon = {
        Publish: PublishTypeIcon,
        TransferObject: TransferObjectTypeIcon,
        Call: CallTypeIcon,
        // TODO: use a different icon
        ChangeEpoch: CallTypeIcon,
        TransferSui: TransferObjectTypeIcon,
    };
    const TxKindName = data.txKindName;
    const Icon = TxTransferTypeIcon[TxKindName];
    const TxStatus = {
        success: ContentSuccessStatus,
        failed: ContentFailedStatus,
    };
    const statusName = data.status === 'success' ? 'success' : 'failed';
    const TxResultStatus = TxStatus[statusName];

    return (
        <div className={styles.txheader}>
            <GoBack />
            <div className={styles.txtypes}>
                <Icon /> {TxKindName}
            </div>
            <div className={styles.txid}>
                <div className={styles.txaddress}>
                    <Longtext
                        text={data.txId}
                        category="addresses"
                        isLink={false}
                    />
                    <div
                        className={cl([
                            styles.txresulttype,
                            styles[statusName],
                        ])}
                    >
                        {' '}
                        <TxResultStatus /> {statusName}
                    </div>
                </div>
                {data.error && (
                    <div
                        className={cl([
                            styles.txresulttype,
                            styles.failed,
                            styles.error,
                        ])}
                    >
                        {data.error}
                    </div>
                )}
            </div>
        </div>
    );
}

export default TxAddressHeader;
