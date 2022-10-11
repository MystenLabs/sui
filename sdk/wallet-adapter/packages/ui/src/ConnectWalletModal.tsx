// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Box, Button, List, Modal, Typography, ListItemButton, ListItemText, CircularProgress } from "@mui/material";
import { useEffect, useState } from "react";
import { useWallet } from "@mysten/wallet-adapter-react";

export interface ConnectWalletButtonProps {}

export function ConnectWalletModal(props: ConnectWalletButtonProps) {
    let { connected } = useWallet();

    const [open, setOpen] = useState(false);

    const handleClickOpen = () => {
      setOpen(true);
    }

    const handleClose = () => {
      setOpen(false);
    }

    const { supportedWallets, wallet, select, connecting } = useWallet();

    const handleConnect = (walletName: string) => {
        select(walletName);
        handleClose();
    }

    const style = {
        position: 'absolute' as 'absolute',
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

    const connectButtonStyle = {
        borderRadius: 7,
        backgroundColor: '#6fbcf0',
        fontWeight: 600
    }

    return (
        <>
            { (!connected) && <>
                <Button style={connectButtonStyle} variant="contained" onClick={handleClickOpen}>Connect To Wallet</Button>
                {console.log(open)}
                <Modal open={open} onClose={handleClose}>
                    <>
                        {!connecting && <Box sx={style}>
                            <Typography id="modal-modal-title" variant="h6" component="h2" align="center">
                                Select Wallet
                            </Typography>
                            <List>
                                {
                                    supportedWallets.map(w =>
                                        <ListItemButton onClick={() => handleConnect(w.adapter.name)}>
                                            <ListItemText primary={w.adapter.name}/>
                                        </ListItemButton>
                                    )
                                }
                            </List>
                        </Box>}
                        {connecting && <Box sx={style}>
                            <Typography id="modal-modal-title" variant="h6" component="h2">
                                Connecting to { wallet ? wallet.adapter.name : "Wallet"}
                            </Typography>
                            <CircularProgress/>
                        </Box>}
                    </>
                </Modal>
            </>}
        </>
    )
}
