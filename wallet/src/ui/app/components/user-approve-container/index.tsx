// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo, useCallback, useState } from 'react';

import AccountAddress from '_components/account-address';
import LoadingIndicator from '_components/loading/LoadingIndicator';

import type { MouseEventHandler, ReactNode } from 'react';

import st from './UserApproveContainer.module.scss';

type UserApproveContainerProps = {
    title: string;
    children: ReactNode | ReactNode[];
    origin: string;
    originFavIcon?: string;
    rejectTitle: string;
    approveTitle: string;
    onSubmit: (approved: boolean) => void;
};

function UserApproveContainer({
    title,
    origin,
    originFavIcon,
    children,
    rejectTitle,
    approveTitle,
    onSubmit,
}: UserApproveContainerProps) {
    const [submitting, setSubmitting] = useState(false);
    const handleOnResponse = useCallback<MouseEventHandler<HTMLButtonElement>>(
        async (e) => {
            setSubmitting(true);
            const allowed = e.currentTarget.dataset.allow === 'true';
            await onSubmit(allowed);
            setSubmitting(false);
        },
        [onSubmit]
    );
    return (
        <div className={st.container}>
            <h2 className={st.title}>{title}</h2>
            <label className={st.label}>Site</label>
            <div className={st.originContainer}>
                {originFavIcon ? (
                    <img
                        className={st.favIcon}
                        src={originFavIcon}
                        alt="Site favicon"
                    />
                ) : null}
                <span className={st.origin}>{origin}</span>
            </div>
            <label className={st.label}>Account</label>
            <AccountAddress showLink={false} />
            {children}
            <div className={st.actions}>
                <button
                    type="button"
                    data-allow="false"
                    onClick={handleOnResponse}
                    className="btn link"
                    disabled={submitting}
                >
                    {rejectTitle}
                </button>
                <button
                    type="button"
                    className="btn"
                    data-allow="true"
                    onClick={handleOnResponse}
                    disabled={submitting}
                >
                    {submitting ? <LoadingIndicator /> : approveTitle}
                </button>
            </div>
        </div>
    );
}

export default memo(UserApproveContainer);
