// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
 * Wraps around all UI components for the Wallet Adapter.
 * Import this component where you desire your "Connect Wallet" button to be.
 */

import { createTheme, ThemeProvider } from "@mui/material"
import { ConnectWalletModal } from "./ConnectWalletModal"
import { ManageWalletModal } from "./ManageWalletModal"


const theme = createTheme({
    typography: {
        "fontFamily": `"IBM Plex Sans", sans-serif`,
    }
})

export function WalletWrapper({}) {
    return(
        <>
            <ThemeProvider theme={theme}>
                <ConnectWalletModal/>
                <ManageWalletModal/>
            </ThemeProvider>
        </>
    )
}