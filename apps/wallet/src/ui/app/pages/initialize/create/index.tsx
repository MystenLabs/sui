// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import Button from '_app/shared/button';
import ExternalLink from '_components/external-link';
import Icon, { SuiIcons } from '_components/icon';
import { PRIVACY_POLICY_LINK, ToS_LINK } from '_shared/constants';
import Loading from '_src/ui/app/components/loading';
import { useAppDispatch, useAppSelector } from '_src/ui/app/hooks';
import { createMnemonic } from '_src/ui/app/redux/slices/account';

import type { ChangeEventHandler } from 'react';

import st from './Create.module.scss';

const CreatePage = () => {
    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    const onHandleCreate = useCallback(async () => {
        await dispatch(createMnemonic());
        navigate('../backup');
    }, [dispatch, navigate]);
    const [termsAccepted, setTerms] = useState(false);
    const onHandleTerms = useCallback<ChangeEventHandler<HTMLInputElement>>(
        (event) => {
            const checked = event.target.checked;
            setTerms(checked);
        },
        []
    );
    const creatingMnemonic = useAppSelector(({ account }) => account.creating);
    return (
        <>
            <section className={st.content}>
                <div>
                    <h1 className={st.headerTitle}>Create New wallet</h1>
                    <div className={st.desc}>
                        Creating a wallet generates new recovery passphrase.
                        Using it you can backup and restore your wallet.
                    </div>
                    <label className={st.terms}>
                        <input
                            type="checkbox"
                            onChange={onHandleTerms}
                            checked={termsAccepted}
                        />
                        <span className={st.checkBox}></span>
                        <span className={st.checkboxLabel}>
                            I agree to the{' '}
                            <ExternalLink href={ToS_LINK} showIcon={false}>
                                Terms of Service
                            </ExternalLink>{' '}
                            and acknowledge the{' '}
                            <ExternalLink
                                href={PRIVACY_POLICY_LINK}
                                showIcon={false}
                            >
                                Privacy Policy
                            </ExternalLink>
                            .
                        </span>
                    </label>
                    <div>
                        <Loading loading={creatingMnemonic}>
                            <Button
                                type="button"
                                onClick={onHandleCreate}
                                disabled={!termsAccepted || creatingMnemonic}
                                mode="primary"
                                className={st.btn}
                                size="large"
                            >
                                Create Wallet Now
                                <Icon
                                    icon={SuiIcons.ArrowRight}
                                    className={st.next}
                                />
                            </Button>
                        </Loading>
                    </div>
                </div>
            </section>
        </>
    );
};

export default CreatePage;
