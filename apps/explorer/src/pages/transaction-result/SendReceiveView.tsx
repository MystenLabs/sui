// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'clsx';

import { ReactComponent as DoneIcon } from '../../assets/SVGIcons/Done.svg';
import { ReactComponent as StartIcon } from '../../assets/SVGIcons/Start.svg';
import Longtext from '../../components/longtext/Longtext';

import styles from './SendReceiveView.module.css';

type TxAddress = {
    sender: string;
    recipient?: string[];
};
//TODO: Add date format function
function SendRecieveView({ data }: { data: TxAddress }) {
    return (
        <div className={styles.txaddress} data-testid="transaction-sender">
            <div className={styles.senderbox}>
                <div>
                    <h4>
                        Sender{' '}
                        {data.recipient?.length === 1 ? '& Recipient' : ''}{' '}
                    </h4>
                </div>
                <div className={styles.oneaddress}>
                    <StartIcon />
                    <Longtext
                        text={data.sender}
                        category="addresses"
                        isLink={true}
                    />
                </div>
            </div>
            <div
                className={cl([
                    styles.txaddresssender,
                    data.recipient?.length ? styles.recipient : '',
                    data.recipient?.length === 1
                        ? styles.txaddresssenderonerecipent
                        : '',
                ])}
            >
                {data.recipient && (
                    <div className={styles.recipientbox}>
                        {data.recipient.length > 1 && (
                            <div>
                                <h4>Recipients</h4>
                            </div>
                        )}
                        {data.recipient.map((add: string, idx: number) => (
                            <div className={styles.oneaddress} key={idx}>
                                <DoneIcon />
                                <Longtext
                                    text={add}
                                    category="addresses"
                                    isLink={true}
                                    alttext={add}
                                />
                            </div>
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}

export default SendRecieveView;
