//SPDX-License-Identifier: GPL-3.0
pragma solidity ^0.8.20;

import "./interfaces/IProxy.sol";
import "./interfaces/IVault.sol";

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
// import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

contract Vault is IVault {
    using SafeERC20 for IERC20;

    IProxy private immutable proxy;

    constructor(IProxy _proxy) {
        proxy = IProxy(_proxy);
    }

    function batchTransferToErc20(
        Erc20Transfer[] calldata _transfers
    // ) external _onlyBridge nonReentrant {
    ) external _onlyBridge {
        for (uint256 i = 0; i < _transfers.length; ) {
            if (_transfers[i].amount > 0) {
                IERC20(_transfers[i].from).safeTransfer(
                    _transfers[i].to,
                    _transfers[i].amount
                );
            }
            unchecked {
                i++;
            }
        }
    }

    modifier _onlyBridge() {
        address bridgeAddress = proxy.getContract("bridge");
        require(msg.sender == bridgeAddress, "Invalid caller.");
        _;
    }
}
