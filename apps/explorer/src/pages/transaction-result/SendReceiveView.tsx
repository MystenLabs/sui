// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'clsx';

import { ReactComponent as LShapeIcon } from '../../assets/LShape.svg';
import { ReactComponent as DoneIcon } from '../../assets/SVGIcons/16px/CheckFill.svg';
import { ReactComponent as StartIcon } from '../../assets/SVGIcons/Start.svg';
import Longtext from '../../components/longtext/Longtext';

import styles from './SendReceiveView.module.css';

type TxAddress = {
    sender: string;
    recipient?: string[];
};
//TODO: Add date format function
function SendRecieveView({ data }: { data: TxAddress }) {
    if (data?.recipient && data.recipient.length === 1) {
        return (
            <div className={styles.txaddress} data-testid="transaction-sender">
                <h4 className={styles.oneheading}>Sender &#x26; Recipient</h4>
                <div className={styles.oneaddress}>
                    <div className="z-0">
                        <StartIcon />
                    </div>
                    <Longtext
                        text={data.sender}
                        category="addresses"
                        isLink={true}
                    />
                </div>
                <div>
                    {data.recipient.map((add: string, idx: number) => (
                        <div key={idx} className="flex ml-[7px] mt-[-7px] z-10">
                            <LShapeIcon />
                            <div className={styles.oneaddress}>
                                <div className={styles.doneicon}>
                                    <DoneIcon />
                                </div>
                                <Longtext
                                    text={add}
                                    category="addresses"
                                    isLink={true}
                                    alttext={add}
                                />
                            </div>
                        </div>
                    ))}
                </div>
            </div>
        );
    }
    return (
        <div className={styles.txaddress} data-testid="transaction-sender">
            <div className={styles.senderbox}>
                <h4>Sender</h4>
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
                ])}
            >
                {data.recipient && (
                    <div className={styles.recipientbox}>
                        <div>
                            <h4>Recipients</h4>
                        </div>
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
