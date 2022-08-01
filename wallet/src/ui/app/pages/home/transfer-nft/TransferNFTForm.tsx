// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo, useCallback, useState } from 'react';

import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import AddressInput from '_components/address-input';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import { useMiddleEllipsis, useAppSelector } from '_hooks';
import {
    GAS_SYMBOL,
    DEFAULT_NFT_TRANSFER_GAS_FEE,
} from '_redux/slices/sui-objects/Coin';

import type { FormValues } from '.';
import type { MouseEventHandler, ButtonHTMLAttributes } from 'react';

import st from './TransferNFTForm.module.scss';

export type TransferNFTFormProps = {
    submitError: string | null;
    gasBalance: string;
    onClearSubmitError: () => void;
};

type RecentTxAd = {
    address: string;
    onClick?: ButtonHTMLAttributes<HTMLButtonElement>['onClick'];
};
function RecentTxAddress({ address, onClick }: RecentTxAd) {
    return (
        <button
            type="button"
            className={st.recentTxBtn}
            onClick={onClick}
            data-txaddess={address}
        >
            <div className={st.recentTxAddress}>
                <div className={st.imgContainer}>
                    <div className={st.img}></div>
                </div>
                <div className={st.recentTxAddressText}>
                    {useMiddleEllipsis(address, 28, 24)}
                </div>
            </div>
        </button>
    );
}

function TransferNFTForm({
    submitError,
    gasBalance,
    onClearSubmitError,
}: TransferNFTFormProps) {
    const {
        isSubmitting,
        isValid,
        values: { to, amount },
        setFieldValue,
    } = useFormikContext<FormValues>();

    const [sendStep, setSendStep] = useState({
        stepName: 'Send NFT',
        backButton: '/nfts',
        nextButton: 'Send',
    });
    const address = useAppSelector(({ account: { address } }) => address);

    const recentTxAddresses = useAppSelector(({ txresults }) =>
        // filter out wallet address
        txresults.recentAddresses.filter((itm) => itm !== address)
    );

    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [to, amount]);

    const selectRecentAddress = useCallback<
        MouseEventHandler<HTMLButtonElement>
    >(
        (e: React.MouseEvent<HTMLElement>) => {
            const address = e.currentTarget.dataset.txaddess;
            setFieldValue('to', address);
        },
        [setFieldValue]
    );

    const StepOne = (
        <>
            <Form className={st.container} autoComplete="off" noValidate={true}>
                <div className={st.group}>
                    <Field
                        component={AddressInput}
                        name="to"
                        className={st.input}
                    />
                    <div className={st.inputGroupAppend}></div>
                </div>
                <div className={st.error}>
                    <ErrorMessage
                        className={st.error}
                        name="to"
                        component="div"
                    />
                    {BigInt(gasBalance) < DEFAULT_NFT_TRANSFER_GAS_FEE && (
                        <div className={st.error}>
                            * Insufficient balance to cover transfer cost
                        </div>
                    )}
                </div>
            </Form>
            {recentTxAddresses.length && (
                <div className={st.recentAddresses}>
                    <div className={st.recentAddressesTitle}>Recent</div>
                    <div className={st.recentAddressesList}>
                        {recentTxAddresses
                            .slice(0, 5)
                            .map((txByAddress, index) => (
                                <RecentTxAddress
                                    onClick={selectRecentAddress}
                                    address={txByAddress}
                                    key={index}
                                />
                            ))}
                    </div>
                </div>
            )}
        </>
    );

    const StepTwo = (
        <>
            <div className={st.group}>
                * Total transaction fee estimate (gas cost):
                {DEFAULT_NFT_TRANSFER_GAS_FEE} {GAS_SYMBOL}
            </div>
        </>
    );

    const StepThreeResponse = (
        <>
            {submitError ? (
                <div className={st.group}>
                    <Alert>
                        <strong>Transfer failed.</strong>{' '}
                        <small>{submitError}</small>
                    </Alert>
                </div>
            ) : null}
        </>
    );

    return (
        <BottomMenuLayout>
            <Content>
                <div className={st.sendNft}>{StepOne}</div>
            </Content>
            <Menu stuckClass={st.shadow} className={st.shadow}>
                <Button
                    size="large"
                    mode="primary"
                    disabled={!isValid || isSubmitting}
                    className={cl(st.action, 'btn')}
                >
                    Continue
                    <Icon
                        icon={SuiIcons.ArrowRight}
                        className={st.arrowActionIcon}
                    />
                </Button>
            </Menu>
        </BottomMenuLayout>
    );
}

export default memo(TransferNFTForm);
