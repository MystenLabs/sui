// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./SuiBridge.sol";
import "./utils/BridgeUtilsV2.sol";

/// @title SuiBridge
/// @notice This contract implements a token bridge that enables users to deposit and withdraw
/// supported tokens to and from other chains. The bridge supports the transfer of Ethereum and ERC20
/// tokens. Bridge operations are managed by a committee of Sui validators that are responsible
/// for verifying and processing bridge messages. The bridge is designed to be upgradeable and
/// can be paused in case of an emergency. The bridge also enforces limits on the amount of
/// assets that can be withdrawn to prevent abuse.
contract SuiBridgeV2 is SuiBridge {
    /* ========== STATE VARIABLES ========== */

    // maximum nonce for which the limiter is skipped; nonces below this value are not limited
    uint64 public maxSkipLimiterNonce;

    /* ========== EXTERNAL FUNCTIONS ========== */

    /// @notice Allows the caller to provide signatures that enable the transfer of tokens to
    /// the recipient address indicated within the message payload.
    /// @dev `message.chainID` represents the sending chain ID. Receiving chain ID needs to match
    /// this bridge's chain ID (this chain).
    /// @param signatures The array of signatures.
    /// @param message The BridgeUtils containing the transfer details.
    function transferBridgedTokensWithSignatures(
        bytes[] memory signatures,
        BridgeUtils.Message memory message
    )
        external
        override
        nonReentrant
        verifyMessageAndSignatures(message, signatures, BridgeUtils.TOKEN_TRANSFER)
        onlySupportedChain(message.chainID)
    {
        // verify that message has not been processed
        require(!isTransferProcessed[message.nonce], "SuiBridge: Message already processed");

        IBridgeConfig config = committee.config();

        BridgeUtils.TokenTransferPayload memory tokenTransferPayload =
            BridgeUtils.decodeTokenTransferPayload(message.payload);

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

        _transferTokensFromVaultV2(
            message.chainID,
            message.nonce,
            tokenTransferPayload.tokenID,
            tokenTransferPayload.recipientAddress,
            erc20AdjustedAmount
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

    /// @notice Updates the total limit with the provided message if the provided signatures are valid.
    /// @param signatures array of signatures to validate the message.
    /// @param message The BridgeUtils containing the update limit payload.
    function updateMaxSkipLimiterNonceWithSignatures(
        bytes[] memory signatures,
        BridgeUtils.Message memory message
    )
        external
        nonReentrant
        verifyMessageAndSignatures(message, signatures, BridgeUtilsV2.UPDATE_MAX_SKIP_LIMITER_NONCE)
    {
        // decode the update limit payload
        uint64 newNonce = BridgeUtilsV2.decodeUpdateMaxSkipLimiterNoncePayload(message.payload);

        // update the chain limit
        maxSkipLimiterNonce = newNonce;

        emit MaxSkipLimiterNonceUpdated(newNonce);
    }

    /* ========== INTERNAL FUNCTIONS ========== */

    /// @dev Transfers tokens from the vault to a target address.
    /// @param sendingChainID The ID of the chain from which the tokens are being transferred.
    /// @param tokenID The ID of the token being transferred.
    /// @param recipientAddress The address to which the tokens are being transferred.
    /// @param amount The amount of tokens being transferred.
    function _transferTokensFromVaultV2(
        uint8 sendingChainID,
        uint64 nonce,
        uint8 tokenID,
        address recipientAddress,
        uint256 amount
    ) private whenNotPaused limitNotExceededV2(sendingChainID, nonce, tokenID, amount) {
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

        // update amount bridged
        limiter.recordBridgeTransfers(sendingChainID, tokenID, amount);
    }

    /* ========== MODIFIERS ========== */

    /// @dev Requires the amount being transferred does not exceed the bridge limit in
    /// the last 24 hours.
    /// @param tokenID The ID of the token being transferred.
    /// @param amount The amount of tokens being transferred.
    modifier limitNotExceededV2(uint8 chainID, uint64 nonce, uint8 tokenID, uint256 amount) {
        if (nonce > maxSkipLimiterNonce) {
            require(
                !limiter.willAmountExceedLimit(chainID, tokenID, amount),
                "SuiBridge: Amount exceeds bridge limit"
            );
        }
        _;
    }

    event MaxSkipLimiterNonceUpdated(uint64 newLimit);
}
