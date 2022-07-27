// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';
import { Link } from 'react-router-dom';

import Icon, { SuiIcons } from '_components/icon';

import st from './PageTitle.module.scss';

export type PageTitleProps = {
    title: string;
    backLink?: string;
    className?: string;
};

function PageTitle({ title, backLink, className }: PageTitleProps) {
    const withBackLink = !!backLink;
    return (
        <div className={cl(st.container, className)}>
            {backLink ? (
                <Link to={backLink} className={st.back}>
                    <Icon icon={SuiIcons.ArrowLeft} className={st.backIcon} />{' '}
                    <span className={st.backText}>Back</span>
                </Link>
            ) : null}
            <h1 className={cl(st.title, { [st.withBackLink]: withBackLink })}>
                {title}
            </h1>
        </div>
    );
}

export default memo(PageTitle);
