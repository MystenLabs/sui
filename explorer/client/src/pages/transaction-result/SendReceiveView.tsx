// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import Longtext from '../../components/longtext/Longtext';

import styles from './SendReceiveView.module.css';

type TxAddress = {
    timestamp_ms?: number;
    sender: string;
    recipient?: string[];
};
//TODO: Add date format function
function SendRecieveView({ data }: { data: TxAddress }) {
    return (
        <div className={styles.txaddress}>
            <div className={styles.txaddressheader}>
                <h3 className={styles.label}>
                    Sender {data.recipient?.length ? '& Recipients' : ''}{' '}
                    {data.timestamp_ms && (
                        <span>{new Date(data.timestamp_ms).toUTCString()}</span>
                    )}
                </h3>
            </div>
            <div className={styles.txaddresssender}>
                <Longtext
                    text={data.sender}
                    category="addresses"
                    isLink={true}
                />
                {data.recipient && (
                    <ul className={styles.txrecipents}>
                        {data.recipient.map((add: string, idx: number) => (
                            <li key={idx}>
                                <Longtext
                                    text={add}
                                    category="addresses"
                                    isLink={true}
                                    alttext={add}
                                />
                            </li>
                        ))}
                    </ul>
                )}
            </div>
        </div>
    );
}

export default SendRecieveView;
