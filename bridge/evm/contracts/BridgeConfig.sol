// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";
import "./utils/CommitteeUpgradeable.sol";
import "./interfaces/IBridgeConfig.sol";

/// @title BridgeConfig
/// @notice This contract manages a registry of supported tokens and supported chain IDs for the SuiBridge.
/// It also provides functions to convert token amounts to Sui decimal adjusted amounts and vice versa.
contract BridgeConfig is IBridgeConfig, CommitteeUpgradeable {
    /* ========== STATE VARIABLES ========== */

    uint8 public chainID;
    mapping(uint8 tokenID => Token) public supportedTokens;
    // price in USD (8 decimal precision) (e.g. 1 ETH = 2000 USD => 2000_00000000)
    mapping(uint8 tokenID => uint64 tokenPrice) public tokenPrices;
    mapping(uint8 chainId => bool isSupported) public supportedChains;

    /* ========== INITIALIZER ========== */

    /// @notice Constructor function for the BridgeConfig contract.
    /// @dev the provided arrays must have the same length.
    /// @param _committee The address of the BridgeCommittee contract.
    /// @param _chainID The ID of the chain this contract is deployed on.
    /// @param _supportedTokens The addresses of the supported tokens.
    /// @param _tokenPrices An array of token prices (with 8 decimal precision).
    /// @param _supportedChains array of supported chain IDs.
    function initialize(
        address _committee,
        uint8 _chainID,
        address[] memory _supportedTokens,
        uint64[] memory _tokenPrices,
        uint8[] memory _tokenIds,
        uint8[] memory _suiDecimals,
        uint8[] memory _supportedChains
    ) external initializer {
        __CommitteeUpgradeable_init(_committee);
        require(
            _supportedTokens.length == _tokenPrices.length, "BridgeConfig: Invalid token prices"
        );
        require(
            _supportedTokens.length == _tokenIds.length, "BridgeConfig: Invalid token IDs"
        );
        require(
            _supportedTokens.length == _suiDecimals.length, "BridgeConfig: Invalid Sui decimals"
        );

        for (uint8 i; i < _tokenIds.length; i++) {
            // `is_native` is hardcoded to `true` because we only support Eth native tokens
            // at the moment. This needs to change when we support tokens native on other chains.
            supportedTokens[_tokenIds[i]] = Token(_supportedTokens[i], _suiDecimals[i], true);
        }

        for (uint8 i; i < _supportedChains.length; i++) {
            require(_supportedChains[i] != _chainID, "BridgeConfig: Cannot support self");
            supportedChains[_supportedChains[i]] = true;
        }

        for (uint8 i; i < _tokenPrices.length; i++) {
            tokenPrices[_tokenIds[i]] = _tokenPrices[i];
        }

        chainID = _chainID;
    }

    /* ========== VIEW FUNCTIONS ========== */

    /// @notice Returns the address of the token with the given ID.
    /// @param tokenID The ID of the token.
    /// @return address of the provided token.
    function tokenAddressOf(uint8 tokenID) public view override returns (address) {
        return supportedTokens[tokenID].tokenAddress;
    }

    /// @notice Returns the sui decimal places of the token with the given ID.
    /// @param tokenID The ID of the token.
    /// @return amount of sui decimal places of the provided token.
    function tokenSuiDecimalOf(uint8 tokenID) public view override returns (uint8) {
        return supportedTokens[tokenID].suiDecimal;
    }

    /// @notice Returns the price of the token with the given ID.
    /// @param tokenID The ID of the token.
    /// @return price of the provided token.
    function tokenPriceOf(uint8 tokenID) public view override returns (uint64) {
        return tokenPrices[tokenID];
    }

    /// @notice Returns whether a token is supported in SuiBridge with the given ID.
    /// @param tokenID The ID of the token.
    /// @return true if the token is supported, false otherwise.
    function isTokenSupported(uint8 tokenID) public view override returns (bool) {
        return supportedTokens[tokenID].tokenAddress != address(0);
    }

    /// @notice Returns whether a chain is supported in SuiBridge with the given ID.
    /// @param chainId The ID of the chain.
    /// @return true if the chain is supported, false otherwise.
    function isChainSupported(uint8 chainId) public view override returns (bool) {
        return supportedChains[chainId];
    }

    /* ========== MUTATIVE FUNCTIONS ========== */

    /// @notice Updates the token price with the provided message if the provided signatures are valid.
    /// @param signatures array of signatures to validate the message.
    /// @param message BridgeMessage containing the update token price payload.
    function updateTokenPriceWithSignatures(
        bytes[] memory signatures,
        BridgeUtils.Message memory message
    )
        external
        nonReentrant
        verifyMessageAndSignatures(message, signatures, BridgeUtils.UPDATE_TOKEN_PRICE)
    {
        // decode the update token payload
        (uint8 tokenID, uint64 price) = BridgeUtils.decodeUpdateTokenPricePayload(message.payload);

        _updateTokenPrice(tokenID, price);

        emit TokenPriceUpdatedV2(message.nonce, tokenID, price);
    }

    function addTokensWithSignatures(bytes[] memory signatures, BridgeUtils.Message memory message)
        external
        nonReentrant
        verifyMessageAndSignatures(message, signatures, BridgeUtils.ADD_EVM_TOKENS)
    {
        // decode the update token payload
        (
            bool native,
            uint8[] memory tokenIDs,
            address[] memory tokenAddresses,
            uint8[] memory suiDecimals,
            uint64[] memory _tokenPrices
        ) = BridgeUtils.decodeAddTokensPayload(message.payload);

        // update the token
        for (uint8 i; i < tokenIDs.length; i++) {
            _addToken(tokenIDs[i], tokenAddresses[i], suiDecimals[i], _tokenPrices[i], native);
        }

        emit TokensAddedV2(message.nonce, tokenIDs, tokenAddresses, suiDecimals, _tokenPrices);
    }

    /* ========== PRIVATE FUNCTIONS ========== */

    /// @notice Updates the price of the token with the provided ID.
    /// @param tokenID The ID of the token to update.
    /// @param tokenPrice The price of the token.
    function _updateTokenPrice(uint8 tokenID, uint64 tokenPrice) private {
        require(isTokenSupported(tokenID), "BridgeConfig: Unsupported token");
        require(tokenPrice > 0, "BridgeConfig: Invalid token price");

        tokenPrices[tokenID] = tokenPrice;
    }

    /// @notice Updates the token with the provided ID.
    /// @param tokenID The ID of the token to update.
    /// @param tokenAddress The address of the token.
    /// @param suiDecimal The decimal places of the token.
    /// @param tokenPrice The price of the token.
    /// @param native Whether the token is native to the chain.
    function _addToken(
        uint8 tokenID,
        address tokenAddress,
        uint8 suiDecimal,
        uint64 tokenPrice,
        bool native
    ) private {
        require(tokenAddress != address(0), "BridgeConfig: Invalid token address");
        require(suiDecimal > 0, "BridgeConfig: Invalid Sui decimal");
        require(tokenPrice > 0, "BridgeConfig: Invalid token price");

        uint8 erc20Decimals = IERC20Metadata(tokenAddress).decimals();
        require(erc20Decimals >= suiDecimal, "BridgeConfig: Invalid Sui decimal");

        supportedTokens[tokenID] = Token(tokenAddress, suiDecimal, native);
        tokenPrices[tokenID] = tokenPrice;
    }

    /* ========== MODIFIERS ========== */

    /// @notice Requires the given token to be supported.
    /// @param tokenID The ID of the token to check.
    modifier tokenSupported(uint8 tokenID) {
        require(isTokenSupported(tokenID), "BridgeConfig: Unsupported token");
        _;
    }
}
