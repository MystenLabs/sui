// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import ExternalLink from '_components/external-link';
import Icon, { SuiIcons } from '_components/icon';

import st from './SuiApp.module.scss';

export type SuiAppProps = {
    title: string;
    description: string;
    icon: string;
    displaytype: 'full' | 'half';
    tags?: string[];
    link: string;
};

function SuiApp({
    title,
    description,
    icon,
    displaytype,
    link,
    tags,
}: SuiAppProps) {
    return (
        <ExternalLink
            href={link}
            title={title}
            className={st.ecosystemApp}
            showIcon={false}
        >
            <div className={cl(st.suiApp, st[displaytype])}>
                <div className={st.icon}>
                    <img src={icon} className={st.icon} alt={title} />
                </div>
                <div className={st.info}>
                    <div className={st.title}>
                        {title}{' '}
                        <Icon
                            icon={SuiIcons.ArrowRight}
                            className={cl(st.arrowActionIcon, st.angledArrow)}
                        />
                    </div>
                    <div className={st.description}>{description}</div>
                    {tags?.length && (
                        <div className={st.tags}>
                            {tags?.map((tag, index) => (
                                <div className={st.tag} key={index}>
                                    {tag}
                                </div>
                            ))}
                        </div>
                    )}
                </div>
            </div>
        </ExternalLink>
    );
}

export default memo(SuiApp);
