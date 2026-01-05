// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./SuiBridge.sol";
import "./utils/BridgeUtilsV2.sol";

contract SuiBridgeV2 is SuiBridge {
    /* ========== EXTERNAL FUNCTIONS ========== */

    /// @notice Allows the caller to provide signatures that enable the transfer of tokens to
    /// the recipient address indicated within the message payload.
    /// @dev `message.chainID` represents the sending chain ID. Receiving chain ID needs to match
    /// this bridge's chain ID (this chain).
    /// @param signatures The array of signatures.
    /// @param message The BridgeUtils containing the transfer details.
    function transferBridgedTokensWithSignaturesV2(
        bytes[] memory signatures,
        BridgeUtils.Message memory message
    )
        external
        nonReentrant
        verifyMessageAndSignatures(message, signatures, BridgeUtils.TOKEN_TRANSFER)
        onlySupportedChain(message.chainID)
    {
        // verify that message has not been processed
        require(!isTransferProcessed[message.nonce], "SuiBridge: Message already processed");
        require(message.version == 2, "SuiBridge: Invalid message version");

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
            tokenTransferPayload.timestampMs / 1000
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

    /// @notice Enables the caller to deposit supported tokens to be bridged to a given
    /// destination chain.
    /// @dev The provided tokenID and destinationChainID must be supported. The caller must
    /// have approved this contract to transfer the given token.
    /// @param tokenID The ID of the token to be bridged.
    /// @param amount The amount of tokens to be bridged.
    /// @param recipientAddress The address on the Sui chain where the tokens will be sent.
    /// @param destinationChainID The ID of the destination chain.
    function bridgeERC20V2(
        uint8 tokenID,
        uint256 amount,
        bytes memory recipientAddress,
        uint8 destinationChainID
    ) external whenNotPaused nonReentrant onlySupportedChain(destinationChainID) {
        require(
            recipientAddress.length == SUI_ADDRESS_LENGTH,
            "SuiBridge: Invalid recipient address length"
        );

        IBridgeConfig config = committee.config();

        require(config.isTokenSupported(tokenID), "SuiBridge: Unsupported token");

        address tokenAddress = config.tokenAddressOf(tokenID);

        // check that the bridge contract has allowance to transfer the tokens
        require(
            IERC20(tokenAddress).allowance(msg.sender, address(this)) >= amount,
            "SuiBridge: Insufficient allowance"
        );

        // calculate old vault balance
        uint256 oldBalance = IERC20(tokenAddress).balanceOf(address(vault));

        // Transfer the tokens from the contract to the vault
        SafeERC20.safeTransferFrom(IERC20(tokenAddress), msg.sender, address(vault), amount);

        // calculate new vault balance
        uint256 newBalance = IERC20(tokenAddress).balanceOf(address(vault));

        // calculate the amount transferred
        uint256 amountTransfered = newBalance - oldBalance;

        // Adjust the amount
        uint64 suiAdjustedAmount = BridgeUtils.convertERC20ToSuiDecimal(
            IERC20Metadata(tokenAddress).decimals(),
            config.tokenSuiDecimalOf(tokenID),
            amountTransfered
        );

        emit TokensDepositedV2(
            config.chainID(),
            nonces[BridgeUtils.TOKEN_TRANSFER],
            destinationChainID,
            tokenID,
            suiAdjustedAmount,
            msg.sender,
            recipientAddress,
            block.timestamp
        );

        // increment token transfer nonce
        nonces[BridgeUtils.TOKEN_TRANSFER]++;
    }

    /// @notice Enables the caller to deposit Eth to be bridged to a given destination chain.
    /// @dev The provided destinationChainID must be supported.
    /// @param recipientAddress The address on the destination chain where Eth will be sent.
    /// @param destinationChainID The ID of the destination chain.
    function bridgeETHV2(bytes memory recipientAddress, uint8 destinationChainID)
        external
        payable
        whenNotPaused
        nonReentrant
        onlySupportedChain(destinationChainID)
    {
        require(
            recipientAddress.length == SUI_ADDRESS_LENGTH,
            "SuiBridge: Invalid recipient address length"
        );

        uint256 amount = msg.value;

        // Transfer the unwrapped ETH to the target address
        (bool success,) = payable(address(vault)).call{value: amount}("");
        require(success, "SuiBridge: Failed to transfer ETH to vault");

        // Adjust the amount to emit.
        IBridgeConfig config = committee.config();

        // Adjust the amount
        uint64 suiAdjustedAmount = BridgeUtils.convertERC20ToSuiDecimal(
            IERC20Metadata(config.tokenAddressOf(BridgeUtils.ETH)).decimals(),
            config.tokenSuiDecimalOf(BridgeUtils.ETH),
            amount
        );

        emit TokensDepositedV2(
            config.chainID(),
            nonces[BridgeUtils.TOKEN_TRANSFER],
            destinationChainID,
            BridgeUtils.ETH,
            suiAdjustedAmount,
            msg.sender,
            recipientAddress,
            block.timestamp
        );

        // increment token transfer nonce
        nonces[BridgeUtils.TOKEN_TRANSFER]++;
    }

    /* ========== INTERNAL FUNCTIONS ========== */

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
        uint256 timestampSeconds
    ) private whenNotPaused limitNotExceededV2(sendingChainID, tokenID, amount, timestampSeconds) {
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

    /* ========== MODIFIERS ========== */

    /// @dev Requires the amount being transferred does not exceed the bridge limit in
    /// the last 48 hours.
    /// @param tokenID The ID of the token being transferred.
    /// @param amount The amount of tokens being transferred.
    modifier limitNotExceededV2(
        uint8 chainID,
        uint8 tokenID,
        uint256 amount,
        uint256 timestampSeconds
    ) {
        if (!BridgeUtilsV2.isMatureMessage(timestampSeconds, block.timestamp)) {
            require(
                !limiter.willAmountExceedLimit(chainID, tokenID, amount),
                "SuiBridge: Amount exceeds bridge limit"
            );
            // record the transfer in the limiter
            limiter.recordBridgeTransfers(chainID, tokenID, amount);
        }
        _;
    }

    /* ========== EVENTS ========== */

    /// @notice Emitted when tokens are deposited to be bridged.
    /// @param sourceChainID The ID of the source chain (this chain).
    /// @param nonce The nonce of the transaction on source chain.
    /// @param destinationChainID The ID of the destination chain.
    /// @param tokenID The code of the token.
    /// @param suiAdjustedAmount The amount of tokens to transfer, adjusted for Sui decimals.
    /// @param senderAddress The address of the sender.
    /// @param recipientAddress The address of the sender.
    event TokensDepositedV2(
        uint8 indexed sourceChainID,
        uint64 indexed nonce,
        uint8 indexed destinationChainID,
        uint8 tokenID,
        uint64 suiAdjustedAmount,
        address senderAddress,
        bytes recipientAddress,
        uint256 timestampSeconds
    );
}
