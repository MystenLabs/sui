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

import { useFormatCoin, CoinFormat } from '~/hooks/useFormatCoin';

type TxAddress = {
    sender: string;
    recipient?: string[];
    amount?: bigint[];
    objects?: string[];
};

const getObjType = (objId: string, network: string) =>
    rpc(network)
        .getObject(objId)
        .then((obj) => parseObjectType(obj));

function MultipleRecipients({ sender, recipient, amount, objects }: TxAddress) {
    const [network] = useContext(NetworkContext);

    const [coinList, setCoinList] = useState({
        loadState: 'pending',
        data: [],
    });

    const [isSingleCoin, setIsSingleCoin] = useState(false);

    useEffect(() => {
        if (objects) {
            Promise.all(objects.map((objId) => getObjType(objId, network)))
                .then((objTypes) => {
                    setCoinList({
                        loadState: 'loaded',
                        data: objTypes,
                    });

                    if (objTypes.every((val) => val === objTypes[0])) {
                        setIsSingleCoin(true);
                    }
                })
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
        <>
            {isSingleCoin && amount && (
                <div className={styles.amountbox}>
                    <div>Amount</div>
                    <SingleAmount
                        amount={amount.reduce((x, y) => x + y)}
                        objectId={objects![0]}
                    />
                </div>
            )}
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
                                    <>
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
                                        {amount?.[idx] && (
                                            <Amount
                                                amount={amount![idx]}
                                                label={
                                                    coinList.loadState ===
                                                    'loaded'
                                                        ? coinList.data[idx]
                                                        : ''
                                                }
                                            />
                                        )}
                                    </>
                                </div>
                            ))}
                        </div>
                    )}
                </div>
            </div>
        </>
    );
}

function Amount({ amount, label }: { amount: bigint; label: string }) {
    const coinBoilerPlateRemoved = /^0x2::coin::Coin<(.+)>$/.exec(label)?.[1];
    const formattedCoin = useFormatCoin(
        amount,
        coinBoilerPlateRemoved,
        CoinFormat.FULL
    );
    return (
        <div className={styles.sui}>
            <span className={styles.suiamount}>{formattedCoin[0]}</span>
            <span className={styles.suilabel}>{formattedCoin[1]}</span>
        </div>
    );
}

function SingleAmount({
    amount,
    objectId,
}: {
    amount: bigint;
    objectId: string;
}) {
    const [network] = useContext(NetworkContext);

    const [label, setLabel] = useState({
        data: '',
        loadState: 'pending',
    });

    useEffect(() => {
        getObjType(objectId, network)
            .then((res) =>
                setLabel({
                    data: /^0x2::coin::Coin<(.+)>$/.exec(res)?.[1]!,
                    loadState: 'loaded',
                })
            )
            .catch((err) =>
                setLabel({
                    data: '',
                    loadState: 'fail',
                })
            );
    }, [objectId, network]);

    const formattedAmount = useFormatCoin(amount, label.data, CoinFormat.FULL);

    return (
        <div>
            {formattedAmount[0]}
            <sup>{formattedAmount[1]}</sup>
        </div>
    );
}

//TODO: Add date format function
function SendReceiveView({ sender, recipient, amount, objects }: TxAddress) {
    if (recipient && recipient.length === 1 && amount) {
        return (
            <>
                <div className={styles.amountbox}>
                    <div>Amount</div>
                    <SingleAmount amount={amount[0]} objectId={objects![0]} />
                </div>
                <div className={styles.txaddress}>
                    <h4 className={styles.oneheading}>
                        Sender &#x26; Recipient
                    </h4>
                    <div
                        className={cl([styles.oneaddress, styles.senderwline])}
                    >
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
                                        className={`${styles.doneicon} ${styles.doneiconwline}`}
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
            </>
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
