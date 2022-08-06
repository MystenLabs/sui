// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { ErrorMessage, Form, Field, useFormikContext } from 'formik';
import { useEffect, useRef, memo, useCallback } from 'react';
import TextareaAutosize from 'react-textarea-autosize';

import { Content } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { DEFAULT_NFT_TRANSFER_GAS_FEE } from '_redux/slices/sui-objects/Coin';

import type { FormValues } from '.';

import st from './TransferNFTForm.module.scss';

export type TransferNFTFormProps = {
    submitError: string | null;
    gasBalance: string;
    onClearSubmitError: () => void;
};

function TransferNFTForm({
    submitError,
    gasBalance,
    onClearSubmitError,
}: TransferNFTFormProps) {
    const {
        isSubmitting,
        isValid,
        dirty,
        values: { to, amount },
        setFieldValue,
    } = useFormikContext<FormValues>();

    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [to, amount]);

    // TODO: add QR code scanner
    const clearAddress = useCallback(() => {
        setFieldValue('to', '');
    }, [setFieldValue]);

    return (
        <div className={st.sendNft}>
            <Content>
                <Form
                    className={st.container}
                    autoComplete="off"
                    noValidate={true}
                >
                    <label className={st.labelInfo}>
                        Enter the address of the recipient to start sending the
                        NFT
                    </label>
                    <div
                        className={cl(
                            st.group,
                            dirty && to !== '' && !isValid ? st.invalidAddr : ''
                        )}
                    >
                        <div className={st.textarea}>
                            <Field as="div" id="to" placeholder="Enter Address">
                                <TextareaAutosize
                                    maxRows={2}
                                    minRows={1}
                                    name="to"
                                    value={to}
                                    placeholder="Enter Address"
                                    className={st.input}
                                />
                            </Field>
                        </div>
                        <div
                            onClick={clearAddress}
                            className={cl(
                                st.inputGroupAppend,
                                dirty && to !== ''
                                    ? st.changeAddrIcon + ' sui-icons-close'
                                    : st.qrCode
                            )}
                        ></div>
                    </div>

                    <ErrorMessage
                        className={st.error}
                        name="to"
                        component="div"
                    />
                    {isValid && (
                        <div className={st.validAddress}>
                            <Icon
                                icon={SuiIcons.Checkmark}
                                className={st.checkmark}
                            />
                        </div>
                    )}
                    {BigInt(gasBalance) < DEFAULT_NFT_TRANSFER_GAS_FEE && (
                        <div className={st.error}>
                            * Insufficient balance to cover transfer cost
                        </div>
                    )}

                    <div className={st.formcta}>
                        <Button
                            size="large"
                            mode="primary"
                            type="submit"
                            disabled={!isValid || isSubmitting}
                            className={cl(st.action, 'btn', st.sendNftBtn)}
                        >
                            Send NFT Now
                            {isSubmitting ? (
                                <LoadingIndicator />
                            ) : (
                                <Icon
                                    icon={SuiIcons.ArrowRight}
                                    className={st.arrowActionIcon}
                                />
                            )}
                        </Button>
                    </div>
                </Form>
            </Content>
        </div>
    );
}

export default memo(TransferNFTForm);
