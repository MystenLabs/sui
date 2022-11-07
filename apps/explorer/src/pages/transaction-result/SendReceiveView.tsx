// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import cl from 'clsx';

import { ReactComponent as DoneIcon } from '../../assets/SVGIcons/16px/CheckFill.svg';
import { ReactComponent as StartIcon } from '../../assets/SVGIcons/Start.svg';
import Longtext from '../../components/longtext/Longtext';
import { parseObjectType } from '../../utils/objectUtils';

import styles from './SendReceiveView.module.css';

import { useFormatCoin, CoinFormat } from '~/hooks/useFormatCoin';
import { useRpc } from '~/hooks/useRpc';
import { Heading } from '~/ui/Heading';

type TxAddress = {
    sender: string;
    recipient?: string[];
    amount?: bigint[];
    objects?: string[];
};

function MultipleRecipients({ sender, recipient, amount, objects }: TxAddress) {
    const rpc = useRpc();

    const { data: coinList, isSuccess } = useQuery(
        ['get-coin-types-for-pay-tx', objects],
        async () =>
            Promise.all(
                objects!.map((objId) =>
                    rpc.getObject(objId).then((obj) => parseObjectType(obj))
                )
            ),
        {
            enabled: !!objects,
        }
    );

    const isSingleCoin = coinList?.every((val) => val === coinList[0]) || false;

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
                    <Heading as="h4" variant="heading4" weight="semibold">
                        Sender
                    </Heading>
                    <div className={styles.oneaddress}>
                        <StartIcon />
                        <Longtext text={sender} category="addresses" isLink />
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
                                <Heading
                                    as="h4"
                                    variant="heading4"
                                    weight="semibold"
                                >
                                    Recipients
                                </Heading>
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
                                            isLink
                                            alttext={add}
                                        />
                                    </div>
                                    {!!amount?.[idx] && (
                                        <Amount
                                            amount={amount![idx]}
                                            label={
                                                isSuccess && coinList
                                                    ? coinList[idx]
                                                    : ''
                                            }
                                        />
                                    )}
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
    const coinType = Coin.getCoinType(label);
    const [formattedAmount, suffix] = useFormatCoin(
        amount,
        coinType,
        CoinFormat.FULL
    );
    return (
        <div className={styles.sui}>
            <span className={styles.suiamount}>{formattedAmount}</span>
            <span className={styles.suilabel}>{suffix}</span>
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
    const rpc = useRpc();

    const { data: label } = useQuery(
        ['get-coin-type-for-pay-tx', objectId],
        async () => {
            const obj = await rpc.getObject(objectId);

            return Coin.getCoinType(parseObjectType(obj));
        }
    );

    const [formattedAmount, suffix] = useFormatCoin(
        amount,
        label,
        CoinFormat.FULL
    );

    return (
        <div>
            {formattedAmount}
            <sup>{suffix}</sup>
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
                    <div className={styles.oneheading}>
                        <Heading as="h4" variant="heading4" weight="semibold">
                            Sender &#x26; Recipient
                        </Heading>
                    </div>
                    <div
                        className={cl([styles.oneaddress, styles.senderwline])}
                    >
                        <div className="z-0">
                            <StartIcon />
                        </div>
                        <Longtext text={sender} category="addresses" isLink />
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
                                        isLink
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
