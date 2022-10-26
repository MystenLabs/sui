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
    amount?: bigint[];
};
//TODO: Add date format function
function SendReceiveView({ sender, recipient, amount }: TxAddress) {
    if (recipient && recipient.length === 1) {
        return (
            <div className={styles.txaddress} data-testid="transaction-sender">
                <h4 className={styles.oneheading}>Sender &#x26; Recipient</h4>
                <div className={styles.oneaddress}>
                    <div className="z-0">
                        <StartIcon />
                    </div>
                    <Longtext
                        text={sender}
                        category="addresses"
                        isLink={true}
                    />
                </div>
                <div>
                    {recipient.map((add: string, idx: number) => (
                        <div key={idx} className="flex ml-[7px] mt-[-7px] z-10">
                            <LShapeIcon />
                            <div
                                className={cl([styles.oneaddress, 'mt-[20px]'])}
                            >
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
                        text={sender}
                        category="addresses"
                        isLink={true}
                    />
                </div>
            </div>
            <div
                className={cl([
                    styles.txaddresssender,
                    recipient?.length ? styles.recipient : '',
                ])}
            >
                {recipient && (
                    <div className={styles.recipientbox}>
                        <div>
                            <h4>Recipients</h4>
                        </div>
                        {recipient.map((add: string, idx: number) => (
                            <div key={idx}>
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
                                <div className={styles.sui}>
                                    <span className={styles.suiamount}>
                                        {amount?.[idx].toString()}
                                    </span>
                                    <span className={styles.suilabel}>SUI</span>
                                </div>
                            </div>
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}

export default SendReceiveView;
