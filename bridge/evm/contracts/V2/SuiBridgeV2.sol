// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../SuiBridge.sol";
import "./utils/BridgeUtilsV2.sol";
import "./utils/CommitteeUpgradeableV2.sol";

contract SuiBridgeV2 is SuiBridge, CommitteeUpgradeableV2 {
    /// @notice Allows the caller to provide signatures that enable the transfer of tokens to
    /// the recipient address indicated within the message payload.
    /// @dev `message.chainID` represents the sending chain ID. Receiving chain ID needs to match
    /// this bridge's chain ID (this chain).
    /// @param signatures The array of signatures.
    /// @param message The BridgeUtils containing the transfer details.
    function transferBridgedTokensWithSignaturesV2(
        bytes[] memory signatures,
        uint8 epoch,
        BridgeUtils.Message memory message
    )
        external
        nonReentrant
        verifyMessageAndSignaturesV2(message, signatures, epoch, BridgeUtils.UPDATE_BRIDGE_LIMIT)
        onlySupportedChain(message.chainID)
    {
        // verify that message has not been processed
        require(!isTransferProcessed[message.nonce], "SuiBridge: Message already processed");

        IBridgeConfig config = committee.config();

        BridgeUtilsV2.TokenTransferPayloadV2 memory tokenTransferPayload =
            BridgeUtilsV2.decodeTokenTransferPayloadV2(message.payload);

        // verify target chain ID is this chain ID
        require(
            tokenTransferPayload.targetChain == config.chainID(), "SuiBridge: Invalid target chain"
        );

        // convert amount to ERC20 token decimals
        uint256 erc20AdjustedAmount = BridgeUtils.convertSuiToERC20Decimal(
            IERC20Metadata(config.tokenAddressOf(tokenTransferPayload.tokenID)).decimals(),
            config.tokenSuiDecimalOf(tokenTransferPayload.tokenID),
            tokenTransferPayload.amount
        );

        _transferTokensFromVault(
            message.chainID,
            tokenTransferPayload.tokenID,
            tokenTransferPayload.recipientAddress,
            erc20AdjustedAmount,
            tokenTransferPayload.timestamp
        );

        // mark message as processed
        isTransferProcessed[message.nonce] = true;

        emit TokensClaimed(
            message.chainID,
            message.nonce,
            config.chainID(),
            tokenTransferPayload.tokenID,
            erc20AdjustedAmount,
            tokenTransferPayload.senderAddress,
            tokenTransferPayload.recipientAddress
        );
    }

    /// @dev Transfers tokens from the vault to a target address.
    /// @param sendingChainID The ID of the chain from which the tokens are being transferred.
    /// @param tokenID The ID of the token being transferred.
    /// @param recipientAddress The address to which the tokens are being transferred.
    /// @param amount The amount of tokens being transferred.
    function _transferTokensFromVault(
        uint8 sendingChainID,
        uint8 tokenID,
        address recipientAddress,
        uint256 amount,
        uint256 transferTimeStamp
    )
        private
        whenNotPaused
        limitNotExceededV2(sendingChainID, tokenID, amount, transferTimeStamp)
    {
        address tokenAddress = committee.config().tokenAddressOf(tokenID);

        // Check that the token address is supported
        require(tokenAddress != address(0), "SuiBridge: Unsupported token");

        // transfer eth if token type is eth
        if (tokenID == BridgeUtils.ETH) {
            vault.transferETH(payable(recipientAddress), amount);
        } else {
            // transfer tokens from vault to target address
            vault.transferERC20(tokenAddress, recipientAddress, amount);
        }
    }

    /// @notice Enables the upgrade of the inheriting contract by verifying the provided signatures.
    /// @dev The function will revert if the provided signatures or message is invalid.
    /// @param signatures The array of signatures to be verified.
    /// @param message The BridgeUtils to be verified.
    function upgradeWithSignatures(bytes[] memory signatures, BridgeUtils.Message memory message)
        public
        override(CommitteeUpgradeableV2, CommitteeUpgradeable)
        verifyMessageAndSignaturesV2(
            message,
            signatures,
            committeeV2.committeeEpoch(),
            BridgeUtils.ADD_EVM_TOKENS
        )
    {
        super.upgradeWithSignatures(signatures, message);
    }

    /* ========== MODIFIERS ========== */

    /// @dev Requires the amount being transferred does not exceed the bridge limit in
    /// the last 24 hours.
    /// @param tokenID The ID of the token being transferred.
    /// @param amount The amount of tokens being transferred.
    modifier limitNotExceededV2(
        uint8 chainID,
        uint8 tokenID,
        uint256 amount,
        uint256 transferTimeStamp
    ) {
        if (!BridgeUtilsV2.isMatureMessage(transferTimeStamp, block.timestamp)) {
            require(
                !limiter.willAmountExceedLimit(chainID, tokenID, amount),
                "SuiBridge: Amount exceeds bridge limit"
            );
            // record the transfer in the limiter
            limiter.recordBridgeTransfers(chainID, tokenID, amount);
        }
        _;
    }
}
