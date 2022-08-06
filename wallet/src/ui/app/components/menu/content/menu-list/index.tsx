// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Link } from 'react-router-dom';
import Browser from 'webextension-polyfill';

import Item from './item';
import { API_ENV_TO_INFO } from '_app/ApiProvider';
import ExternalLink from '_components/external-link';
import { SuiIcons } from '_components/icon';
import { useNextMenuUrl } from '_components/menu/hooks';
import { useAppSelector, useMiddleEllipsis } from '_hooks';
import { ToS_LINK } from '_src/shared/constants';

import st from './MenuList.module.scss';

function MenuList() {
    const accountUrl = useNextMenuUrl(true, '/account');
    const networkUrl = useNextMenuUrl(true, '/network');
    const playgroundUrl = useNextMenuUrl(true, '/playground');
    const address = useAppSelector(({ account }) => account.address);
    const shortenAddress = useMiddleEllipsis(address, 10, 7);
    const apiEnv = useAppSelector((state) => state.app.apiEnv);
    const networkName = API_ENV_TO_INFO[apiEnv].name;
    const version = Browser.runtime.getManifest().version;

    return (
        <div className={st.container}>
            <Link to={accountUrl} className={st.item}>
                <Item
                    icon={SuiIcons.Person}
                    title="Account"
                    subtitle={shortenAddress}
                    indicator={SuiIcons.SuiChevronRight}
                />
            </Link>
            <Link to={networkUrl} className={st.item}>
                <Item
                    icon={SuiIcons.Globe}
                    title="Network"
                    subtitle={networkName}
                    indicator={SuiIcons.SuiChevronRight}
                />
            </Link>
            <Link to={playgroundUrl} className={st.item}>
                <Item
                    icon="joystick"
                    title="Playground"
                    indicator={SuiIcons.SuiChevronRight}
                />
            </Link>
            <ExternalLink className={st.item} href={ToS_LINK} showIcon={false}>
                <Item
                    icon="file-earmark-text"
                    title="Terms of Service"
                    indicator="link-45deg"
                />
            </ExternalLink>
            <div className={st.item}>
                <Item
                    // TODO: import and use the icon from Figma
                    icon={SuiIcons.Info}
                    title="Wallet version"
                    subtitle={'v' + version}
                />
            </div>
        </div>
    );
}

export default MenuList;
