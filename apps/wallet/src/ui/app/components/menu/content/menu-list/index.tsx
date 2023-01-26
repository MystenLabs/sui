// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Link, useNavigate } from 'react-router-dom';
import Browser from 'webextension-polyfill';

import Item from './item';
import { API_ENV_TO_INFO } from '_app/ApiProvider';
import Button from '_app/shared/button';
import FaucetRequestButton from '_app/shared/faucet/request-button';
import { lockWallet } from '_app/wallet/actions';
import ExternalLink from '_components/external-link';
import Icon, { SuiIcons } from '_components/icon';
import { useNextMenuUrl } from '_components/menu/hooks';
import { useAppDispatch, useAppSelector, useMiddleEllipsis } from '_hooks';
import { ToS_LINK } from '_src/shared/constants';

import st from './MenuList.module.scss';

function MenuList() {
    const accountUrl = useNextMenuUrl(true, '/account');
    const networkUrl = useNextMenuUrl(true, '/network');
    const address = useAppSelector(({ account }) => account.address);
    const shortenAddress = useMiddleEllipsis(address, 10, 7);
    const apiEnv = useAppSelector((state) => state.app.apiEnv);
    const networkName = API_ENV_TO_INFO[apiEnv].name;
    const version = Browser.runtime.getManifest().version;
    const dispatch = useAppDispatch();
    const navigate = useNavigate();

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
                    icon={SuiIcons.VersionIcon}
                    title="Wallet version"
                    subtitle={'v' + version}
                />
            </div>
            <div className={st.actionsContainer}>
                <FaucetRequestButton
                    mode="secondary"
                    trackEventSource="settings"
                />
                <Button
                    mode="secondary"
                    size="large"
                    onClick={async () => {
                        try {
                            await dispatch(lockWallet()).unwrap();
                            navigate('/locked', { replace: true });
                        } catch (e) {
                            // Do nothing
                        }
                    }}
                >
                    <Icon icon={SuiIcons.Lock} />
                    Lock Wallet
                </Button>
            </div>
        </div>
    );
}

export default MenuList;
