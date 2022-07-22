var __assign = (this && this.__assign) || function () {
    __assign = Object.assign || function(t) {
        for (var s, i = 1, n = arguments.length; i < n; i++) {
            s = arguments[i];
            for (var p in s) if (Object.prototype.hasOwnProperty.call(s, p))
                t[p] = s[p];
        }
        return t;
    };
    return __assign.apply(this, arguments);
};
import { jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment } from "react/jsx-runtime";
import { Box, Button, List, Modal, Typography, ListItemButton, ListItemText } from "@mui/material";
import { useEffect, useState } from "react";
import { useWallet } from "sui-wallet-adapter-react";
import SettingsIcon from '@mui/icons-material/Settings';
export function ManageWalletModal(props) {
    var _a = useWallet(), connected = _a.connected, disconnect = _a.disconnect, wallet = _a.wallet, getAccounts = _a.getAccounts;
    var _b = useState(false), open = _b[0], setOpen = _b[1];
    var _c = useState(""), account = _c[0], setAccount = _c[1];
    var PK_DISPLAY_LENGTH = 10;
    useEffect(function () {
        getAccounts().then(function (accounts) {
            if (accounts && accounts.length) {
                setAccount(accounts[0]);
            }
        });
    }, [wallet, getAccounts]);
    var handleClickOpen = function () {
        setOpen(true);
    };
    var handleClickDisconnect = function () {
        disconnect();
        setOpen(false);
    };
    var handleClose = function (value) {
        setOpen(false);
    };
    var style = {
        position: 'absolute',
        top: '50%',
        left: '50%',
        transform: 'translate(-50%, -50%)',
        width: 300,
        bgcolor: 'background.paper',
        border: '2px solid #000',
        boxShadow: 24,
        p: 4,
        borderRadius: 2
    };
    var ManageWalletButtonStyle = {
        borderRadius: 7,
        backgroundColor: '#6fbcf0',
        fontWeight: 600
    };
    var handleCopyAddress = function () {
        navigator.clipboard.writeText(account);
    };
    return (_jsx(_Fragment, { children: (connected && wallet) &&
            _jsxs(_Fragment, { children: [_jsxs(Button, __assign({ color: "primary", variant: "contained", style: ManageWalletButtonStyle, onClick: handleClickOpen }, { children: [_jsx(SettingsIcon, {}), " ", account.slice(0, PK_DISPLAY_LENGTH), "..."] })), _jsx(Modal, __assign({ open: open, onClose: handleClose }, { children: _jsxs(Box, __assign({ sx: style }, { children: [_jsx(Typography, { id: "modal-modal-title", variant: "h6", component: "h2" }), _jsxs(List, { children: [_jsx(ListItemButton, __assign({ onClick: handleClickDisconnect }, { children: _jsx(ListItemText, { primary: "Disconnect" }) })), _jsx(ListItemButton, __assign({ onClick: handleCopyAddress }, { children: _jsx(ListItemText, { primary: "Copy Address" }) }))] })] })) }))] }) }));
}
