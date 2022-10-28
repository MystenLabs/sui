// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'clsx';
import { useState, useEffect, useContext } from 'react';

import { ReactComponent as DoneIcon } from '../../assets/SVGIcons/16px/CheckFill.svg';
import { ReactComponent as StartIcon } from '../../assets/SVGIcons/Start.svg';
import Longtext from '../../components/longtext/Longtext';
import { NetworkContext } from '../../context';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import { parseObjectType } from '../../utils/objectUtils';

import styles from './SendReceiveView.module.css';

type TxAddress = {
    sender: string;
    recipient?: string[];
    amount?: bigint[];
    objects?: string[];
};

function MultipleRecipients({ sender, recipient, amount, objects }: TxAddress) {
    const [network] = useContext(NetworkContext);

    const [coinList, setCoinList] = useState({
        loadState: 'pending',
        data: [],
    });

    useEffect(() => {
        if (objects) {
            Promise.all(
                objects.map((objId) =>
                    rpc(network)
                        .getObject(objId)
                        .then((obj) => parseObjectType(obj))
                )
            )
                .then((objTypes) =>
                    setCoinList({
                        loadState: 'loaded',
                        data: objTypes,
                    })
                )
                .catch((error) => {
                    console.error(error);
                    setCoinList({
                        loadState: 'failed',
                        data: [],
                    });
                });
        }
    }, [network, objects]);

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
                                    <span className={styles.suilabel}>
                                        {coinList.loadState === 'loaded'
                                            ? coinList.data[idx]
                                            : ''}
                                    </span>
                                </div>
                            </div>
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}

//TODO: Add date format function
function SendReceiveView({ sender, recipient, amount, objects }: TxAddress) {
    const [isShortScreen, setIsShortScreen] = useState(false);

    const { innerWidth } = window;

    useEffect(() => {
        setIsShortScreen(innerWidth < 440);
    }, [innerWidth]);

    if (recipient && recipient.length === 1) {
        return (
            <div className={styles.txaddress}>
                <h4 className={styles.oneheading}>Sender &#x26; Recipient</h4>
                <div className={cl([styles.oneaddress, styles.senderwline])}>
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
                        <div key={idx} className="flex">
                            <div
                                className={cl([
                                    styles.oneaddress,
                                    'mt-[20px] ml-[10px] w-[90%]',
                                ])}
                            >
                                <div
                                    className={`${styles.doneicon} ${
                                        styles.doneiconwline
                                    }
                                  ${
                                      isShortScreen
                                          ? styles.doneiconwlongline
                                          : styles.doneiconwshortline
                                  }`}
                                >
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
        <MultipleRecipients
            sender={sender}
            recipient={recipient}
            amount={amount}
            objects={objects}
        />
    );
}

export default SendReceiveView;
