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
import { Box, Button, List, Modal, Typography, ListItemButton, ListItemText, CircularProgress } from "@mui/material";
import { useState } from "react";
import { useWallet } from "sui-wallet-adapter-react";
export function ConnectWalletModal(props) {
    var connected = useWallet().connected;
    var _a = useState(false), open = _a[0], setOpen = _a[1];
    var handleClickOpen = function () {
        setOpen(true);
    };
    var handleClose = function () {
        setOpen(false);
    };
    var _b = useWallet(), supportedWallets = _b.supportedWallets, wallet = _b.wallet, select = _b.select, connecting = _b.connecting;
    var handleConnect = function (walletName) {
        select(walletName);
        handleClose();
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
    var connectButtonStyle = {
        borderRadius: 7,
        backgroundColor: '#6fbcf0',
        fontWeight: 600
    };
    return (_jsx(_Fragment, { children: (!connected) && _jsxs(_Fragment, { children: [_jsx(Button, __assign({ style: connectButtonStyle, variant: "contained", onClick: handleClickOpen }, { children: "Connect To Wallet" })), console.log(open), _jsx(Modal, __assign({ open: open, onClose: handleClose }, { children: _jsxs(_Fragment, { children: [!connecting && _jsxs(Box, __assign({ sx: style }, { children: [_jsx(Typography, __assign({ id: "modal-modal-title", variant: "h6", component: "h2", align: "center" }, { children: "Select Wallet" })), _jsx(List, { children: supportedWallets.map(function (w) {
                                            return _jsx(ListItemButton, __assign({ onClick: function () { return handleConnect(w.adapter.name); } }, { children: _jsx(ListItemText, { primary: w.adapter.name }) }));
                                        }) })] })), connecting && _jsxs(Box, __assign({ sx: style }, { children: [_jsxs(Typography, __assign({ id: "modal-modal-title", variant: "h6", component: "h2" }, { children: ["Connecting to ", wallet ? wallet.adapter.name : "Wallet"] })), _jsx(CircularProgress, {})] }))] }) }))] }) }));
}
