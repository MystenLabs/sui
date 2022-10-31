// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo, useCallback, useMemo, useState } from 'react';

import AccountAddress from '_components/account-address';
import ExternalLink from '_components/external-link';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';

import type { MouseEventHandler, ReactNode } from 'react';

import st from './UserApproveContainer.module.scss';

type UserApproveContainerProps = {
    children: ReactNode | ReactNode[];
    origin: string;
    originFavIcon?: string;
    rejectTitle: string;
    approveTitle: string;
    onSubmit: (approved: boolean) => void;
    isConnect?: boolean;
    isWarning?: boolean;
};

function UserApproveContainer({
    origin,
    originFavIcon,
    children,
    rejectTitle,
    approveTitle,
    onSubmit,
    isConnect,
    isWarning,
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

    const parsedOrigin = useMemo(() => new URL(origin), [origin]);

    const isSecure = parsedOrigin.protocol === 'https:';

    return (
        <div className={st.container}>
            <div className={st.scrollBody}>
                <div className={st.originContainer}>
                    {originFavIcon ? (
                        <img
                            className={st.favIcon}
                            src={originFavIcon}
                            alt="Site favicon"
                        />
                    ) : null}
                    <div className={st.host}>
                        {parsedOrigin.host.split('.')[0]}
                    </div>
                    <ExternalLink
                        href={origin}
                        className={cl(st.origin, !isSecure && st.warning)}
                        showIcon={isSecure}
                    >
                        {origin}
                    </ExternalLink>
                    <AccountAddress showLink={false} />
                </div>
                <div className={st.children}>{children}</div>
            </div>
            <div className={st.actionsContainer}>
                <div className={cl(st.actions, isWarning && st.flipActions)}>
                    <button
                        type="button"
                        data-allow="false"
                        onClick={handleOnResponse}
                        className={cl(
                            st.button,
                            isWarning
                                ? st.reject
                                : isConnect
                                ? st.cancel
                                : st.reject
                        )}
                        disabled={submitting}
                    >
                        <Icon icon={SuiIcons.CloseFill} />
                        {rejectTitle}
                    </button>
                    <button
                        type="button"
                        className={cl(
                            st.button,
                            isWarning ? st.cancel : st.approve,
                            submitting && st.loading
                        )}
                        data-allow="true"
                        onClick={handleOnResponse}
                        disabled={submitting}
                    >
                        {!submitting &&
                            !isWarning &&
                            (isConnect ? (
                                <Icon icon="plus" />
                            ) : (
                                <Icon icon={SuiIcons.CheckFill} />
                            ))}

                        <span>
                            {submitting ? (
                                <LoadingIndicator className={st.loader} />
                            ) : (
                                approveTitle
                            )}
                        </span>
                        {isWarning && <Icon icon="arrow-right" />}
                    </button>
                </div>
            </div>
        </div>
    );
}

export default memo(UserApproveContainer);
