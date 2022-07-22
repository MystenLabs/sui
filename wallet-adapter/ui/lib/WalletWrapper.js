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
/*
 * Wraps around all UI components for the Wallet Adapter.
 * Import this component where you desire your "Connect Wallet" button to be.
 */
import { createTheme, ThemeProvider } from "@mui/material";
import { ConnectWalletModal } from "./ConnectWalletModal";
import { ManageWalletModal } from "./ManageWalletModal";
var theme = createTheme({
    typography: {
        "fontFamily": "\"IBM Plex Sans\", sans-serif",
    }
});
export function WalletWrapper(_a) {
    return (_jsx(_Fragment, { children: _jsxs(ThemeProvider, __assign({ theme: theme }, { children: [_jsx(ConnectWalletModal, {}), _jsx(ManageWalletModal, {})] })) }));
}
