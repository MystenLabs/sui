// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo, useCallback, useMemo, useState } from 'react';

import { Button } from '../../shared/ButtonUI';
import AccountAddress from '_components/account-address';
import ExternalLink from '_components/external-link';

import type { ReactNode } from 'react';

import st from './UserApproveContainer.module.scss';

type UserApproveContainerProps = {
    children: ReactNode | ReactNode[];
    origin: string;
    originFavIcon?: string;
    rejectTitle: string;
    approveTitle: string;
    approveDisabled?: boolean;
    onSubmit: (approved: boolean) => void;
    isConnect?: boolean;
    isWarning?: boolean;
    addressHidden?: boolean;
};

function UserApproveContainer({
    origin,
    originFavIcon,
    children,
    rejectTitle,
    approveTitle,
    approveDisabled = false,
    onSubmit,
    isConnect,
    isWarning,
    addressHidden = false,
}: UserApproveContainerProps) {
    const [submitting, setSubmitting] = useState(false);
    const handleOnResponse = useCallback(
        async (allowed: boolean) => {
            setSubmitting(true);
            await onSubmit(allowed);
            setSubmitting(false);
        },
        [onSubmit]
    );

    const parsedOrigin = useMemo(() => new URL(origin), [origin]);

    return (
        <div className={st.container}>
            <div className={st.scrollBody}>
                <div className={st.originContainer}>
                    <div className={st.originMeta}>
                        {originFavIcon ? (
                            <img
                                className={st.favIcon}
                                src={originFavIcon}
                                alt="Site favicon"
                            />
                        ) : null}
                        <div className={st.host}>
                            {parsedOrigin.host.split('.')[0]}
                            <ExternalLink
                                href={origin}
                                className={st.origin}
                                showIcon={false}
                            >
                                {parsedOrigin.host}
                            </ExternalLink>
                        </div>
                    </div>
                    {!addressHidden ? (
                        <div className={st.cardFooter}>
                            <div className={st.label}>Your address</div>
                            <AccountAddress
                                showLink={false}
                                mode="normal"
                                copyable
                                className={st.address}
                            />
                        </div>
                    ) : null}
                </div>
                <div className={st.children}>{children}</div>
            </div>
            <div className={st.actionsContainer}>
                <div className={cl(st.actions, isWarning && st.flipActions)}>
                    <Button
                        size="tall"
                        variant="warning"
                        onClick={() => {
                            handleOnResponse(false);
                        }}
                        disabled={submitting}
                        text={rejectTitle}
                    />
                    <Button
                        // recreate the button when changing the variant to avoid animating to the new styles
                        key={`approve_${isWarning}`}
                        size="tall"
                        variant={isWarning ? 'secondary' : 'primary'}
                        onClick={() => {
                            handleOnResponse(true);
                        }}
                        disabled={approveDisabled}
                        loading={submitting}
                        text={approveTitle}
                    />
                </div>
            </div>
        </div>
    );
}

export default memo(UserApproveContainer);
