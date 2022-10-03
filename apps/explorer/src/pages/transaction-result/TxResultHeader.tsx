// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'clsx';

import { ReactComponent as ContentSuccessStatus } from '../../assets/SVGIcons/12px/Check.svg';
import { ReactComponent as ContentFailedStatus } from '../../assets/SVGIcons/12px/X.svg';
import { ReactComponent as CallTypeIcon } from '../../assets/SVGIcons/Call.svg';
import { ReactComponent as InfoIcon } from '../../assets/SVGIcons/Info.svg';
import { ReactComponent as PublishTypeIcon } from '../../assets/SVGIcons/Publish.svg';
import { ReactComponent as TransferObjectTypeIcon } from '../../assets/SVGIcons/TransferObject.svg';
import Longtext from '../../components/longtext/Longtext';
import resultheaderstyle from '../../styles/resultheader.module.css';

import type { ExecutionStatusType, TransactionKindName } from '@mysten/sui.js';

import styles from './TxResultHeader.module.css';

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
        Pay: TransferObjectTypeIcon,
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
            <div className={resultheaderstyle.category}>
                <Icon /> {TxKindName}
            </div>
            <div>
                <div
                    data-testid="transaction-id"
                    className={cl(resultheaderstyle.address, styles.txaddress)}
                >
                    <Longtext
                        text={data.txId}
                        category="addresses"
                        isLink={false}
                        copyButton="24"
                    />
                    <div
                        className={cl([styles[statusName], styles.statuslabel])}
                    >
                        <TxResultStatus />
                        <span>
                            {statusName === 'failed' ? 'failure' : 'success'}
                        </span>
                    </div>
                </div>
                {data.error && (
                    <div className={cl([styles.failed, styles.explain])}>
                        <InfoIcon /> <span>{data.error}</span>
                    </div>
                )}
            </div>
        </div>
    );
}

export default TxAddressHeader;
