// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { Form, Field, useFormikContext } from 'formik';
import { useEffect, useRef, memo } from 'react';

import { Content } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import AddressInput from '_components/address-input';
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
        values: { to, amount },
    } = useFormikContext<FormValues>();

    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [to, amount]);

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
                    <div className={st.group}>
                        <Field
                            component={AddressInput}
                            name="to"
                            as="div"
                            id="to"
                            placeholder="Enter Address"
                            className={st.input}
                        />
                    </div>

                    {BigInt(gasBalance) < DEFAULT_NFT_TRANSFER_GAS_FEE && (
                        <div className={st.error}>
                            * Insufficient balance to cover transfer cost
                        </div>
                    )}

                    {submitError ? (
                        <div className={st.error}>{submitError}</div>
                    ) : null}

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
