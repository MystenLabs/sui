// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeatureIsOn } from '@growthbook/growthbook-react';
import { X12 } from '@mysten/icons';
import { useEffect } from 'react';
import toast from 'react-hot-toast';
import { useNavigate } from 'react-router-dom';
import Browser from 'webextension-polyfill';

import { trackEvent } from '../../../shared/plausible';
import { useMenuIsOpen, useNextMenuUrl } from '../components/menu/hooks';
import { AppType } from '../redux/slices/app/AppType';
import { ButtonOrLink } from '../shared/utils/ButtonOrLink';
import useAppSelector from './useAppSelector';

const HAS_ACKNOWLEDGED_LEDGER_NOTIFICATION_KEY =
    'has-acknowledged-ledger-notification';
const HAS_ACKNOWLEDGED_LEDGER_NOTIFICATION_VALUE = 'true';

const LEDGER_NOTIFICATION_TOAST_ID = 'ledger-notification-toast';

// TODO: Delete this *soon* because custom, imperative toasts shouldn't be used for notifications :)
export function useLedgerNotification() {
    const isLedgerNotificationEnabled = useFeatureIsOn(
        'wallet-ledger-notification-enabled'
    );
    const isMenuOpen = useMenuIsOpen();
    const appType = useAppSelector((state) => state.app.appType);
    const navigate = useNavigate();
    const connectLedgerModalUrl = useNextMenuUrl(
        true,
        '/accounts/connect-ledger-modal'
    );

    useEffect(() => {
        if (isMenuOpen) {
            toast.remove(LEDGER_NOTIFICATION_TOAST_ID);
        }
    }, [isMenuOpen]);

    useEffect(() => {
        const hasAcknowledgedVal = localStorage.getItem(
            HAS_ACKNOWLEDGED_LEDGER_NOTIFICATION_KEY
        );
        const hasAcknowledged =
            hasAcknowledgedVal === HAS_ACKNOWLEDGED_LEDGER_NOTIFICATION_VALUE;

        if (isLedgerNotificationEnabled && !hasAcknowledged && !isMenuOpen) {
            // If we don't have a timeout, the toast doesn't get rendered after initial render.
            // We'll do this for now since we don't have the time to figure out what exactly is going on
            setTimeout(() => {
                toast.success(
                    <div className="flex gap-2 items-center">
                        <div className="shrink-0">
                            <ButtonOrLink
                                className="font-medium appearance-none border-0 cursor-pointer p-0 bg-transparent text-inherit"
                                onClick={async () => {
                                    trackEvent('LedgerNotification');
                                    localStorage.setItem(
                                        HAS_ACKNOWLEDGED_LEDGER_NOTIFICATION_KEY,
                                        HAS_ACKNOWLEDGED_LEDGER_NOTIFICATION_VALUE
                                    );
                                    toast.remove(LEDGER_NOTIFICATION_TOAST_ID);

                                    if (appType === AppType.popup) {
                                        const { origin, pathname } =
                                            window.location;
                                        await Browser.tabs.create({
                                            url: `${origin}/${pathname}#${connectLedgerModalUrl}`,
                                        });
                                        window.close();
                                    } else {
                                        navigate(connectLedgerModalUrl);
                                    }
                                }}
                            >
                                New! Tap to Connect your Ledger
                            </ButtonOrLink>
                        </div>
                        <button
                            className="w-full flex appearance-none border-0 p-0 bg-transparent cursor-pointer text-success-dark"
                            onClick={() => {
                                localStorage.setItem(
                                    HAS_ACKNOWLEDGED_LEDGER_NOTIFICATION_KEY,
                                    HAS_ACKNOWLEDGED_LEDGER_NOTIFICATION_VALUE
                                );
                                toast.dismiss(LEDGER_NOTIFICATION_TOAST_ID);
                            }}
                        >
                            <div className="sr-only">Dismiss notification</div>
                            <X12 />
                        </button>
                    </div>,
                    {
                        id: LEDGER_NOTIFICATION_TOAST_ID,
                        className:
                            '!px-0 !py-1 !rounded-full !shadow-notification !border !border-solid !border-success-dark/20 !bg-success-light !text-success-dark',
                        duration: Infinity,
                    }
                );
            }, 0);
        }

        return () => {
            toast.remove(LEDGER_NOTIFICATION_TOAST_ID);
        };
    }, [
        appType,
        connectLedgerModalUrl,
        isLedgerNotificationEnabled,
        isMenuOpen,
        navigate,
    ]);
}
