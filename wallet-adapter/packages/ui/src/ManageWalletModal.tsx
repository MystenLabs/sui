// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Box, Button, List, Modal, Typography, ListItemButton, ListItemText}  from "@mui/material";
import { useEffect, useState } from "react";
import { useWallet } from "@mysten/wallet-adapter-react";
import SettingsIcon from '@mui/icons-material/Settings';

export interface Before {}

export interface ManageWalletButtonProps {}

export function ManageWalletModal(props: ManageWalletButtonProps) {
    let { connected, disconnect, wallet, getAccounts} = useWallet();
    const [open, setOpen] = useState(false);
    const [account, setAccount] = useState("");
    const PK_DISPLAY_LENGTH = 10;

    useEffect(() => {
        getAccounts().then((accounts) => {
                if(accounts && accounts.length) {
                    setAccount(accounts[0])
                }
            }
        );
    }, [wallet, getAccounts]);

    const handleClickOpen = () => {
      setOpen(true);
    }

    const handleClickDisconnect = () => {
        disconnect();
        setOpen(false);
    }

    const handleClose = (value: string) => {
      setOpen(false);
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

    const ManageWalletButtonStyle = {
        borderRadius: 7,
        backgroundColor: '#6fbcf0',
        fontWeight: 600
    }

    const handleCopyAddress = () => {
        navigator.clipboard.writeText(account)
    }

    return (
        <>
            { (connected && wallet) &&
                <>
                <Button color="primary" variant="contained" style={ManageWalletButtonStyle} onClick={handleClickOpen}>
                    <SettingsIcon/> { account.slice(0, PK_DISPLAY_LENGTH)}...
                </Button>
                <Modal open={open} onClose={handleClose}>
                    <Box sx={style}>
                        <Typography id="modal-modal-title" variant="h6" component="h2">
                        </Typography>
                        <List>
                            <ListItemButton onClick={handleClickDisconnect}>
                                <ListItemText primary={"Disconnect"}/>
                            </ListItemButton>
                            <ListItemButton onClick={handleCopyAddress}>
                                <ListItemText primary={"Copy Address"}/>
                            </ListItemButton>
                        </List>
                    </Box>
                </Modal>
                </>
            }
        </>
    )
}
