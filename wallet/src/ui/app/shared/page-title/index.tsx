// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';
import { Link } from 'react-router-dom';

import Icon, { SuiIcons } from '_components/icon';

import st from './PageTitle.module.scss';

export type PageTitleProps = {
    title: string;
    stats?: string;
    backLink?: string;
    className?: string;
    hideBackLabel?: boolean;
};

function PageTitle({
    title,
    backLink,
    className,
    stats,
    hideBackLabel,
}: PageTitleProps) {
    const withBackLink = !!backLink;
    return (
        <div className={cl(st.container, className)}>
            {backLink ? (
                <Link to={backLink} className={st.back}>
                    <Icon icon={SuiIcons.ArrowLeft} className={st.backIcon} />{' '}
                    {!hideBackLabel && (
                        <span className={st.backText}>Back</span>
                    )}
                </Link>
            ) : null}
            <h1 className={cl(st.title, { [st.withBackLink]: withBackLink })}>
                {title} {stats && <span className={st.stats}>{stats}</span>}
            </h1>
        </div>
    );
}

export default memo(PageTitle);
