// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

interface ICommon {
    struct Erc20Transfer {
        bytes32 dataDigest;
        uint256 amount;
        address from;
        address to;
    }

    struct BridgeMessage {
        // 0: token , 1: object ? TBD
        uint8 messageType;
        uint8 version;
        uint8 sourceChain;
        uint64 bridgeSeqNum;
        address senderAddress;
        uint8 targetChain;
        address targetAddress;
        bytes payload;
    }
}
