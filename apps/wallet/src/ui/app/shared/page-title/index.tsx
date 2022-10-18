// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';
import { Link } from 'react-router-dom';

import Button from '_app/shared/button';
import Icon, { SuiIcons } from '_components/icon';

import type { ButtonHTMLAttributes } from 'react';

import st from './PageTitle.module.scss';

export type PageTitleProps = {
    title?: string;
    stats?: string;
    backLink?: string;
    className?: string;
    hideBackLabel?: boolean;
    onClick?: ButtonHTMLAttributes<HTMLButtonElement>['onClick'];
};

function PageTitle({
    title,
    backLink,
    onClick,
    className,
    stats,
    hideBackLabel,
}: PageTitleProps) {
    const withBackLink = !!backLink;

    const BlackLinkText = !hideBackLabel && (
        <span className={st.backText}>Back</span>
    );

    const BackButton = onClick && (
        <Button className={st.backButton} onClick={onClick}>
            <Icon icon={SuiIcons.ArrowLeft} className={st.backIcon} />{' '}
            {BlackLinkText}
        </Button>
    );

    return (
        <div className={cl(st.container, className)}>
            {backLink && !onClick && (
                <Link to={backLink} className={st.back}>
                    <Icon icon={SuiIcons.ArrowLeft} className={st.backIcon} />{' '}
                    {!hideBackLabel && (
                        <span className={st.backText}>Back</span>
                    )}
                </Link>
            )}
            {BackButton}
            {title ? (
                <h1
                    className={cl(st.title, {
                        [st.withBackLink]: withBackLink,
                    })}
                >
                    {title} {stats && <span className={st.stats}>{stats}</span>}
                </h1>
            ) : null}
        </div>
    );
}

export default memo(PageTitle);
