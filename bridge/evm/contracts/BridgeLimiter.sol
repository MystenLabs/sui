// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";
import "./interfaces/IBridgeLimiter.sol";
import "./interfaces/IBridgeConfig.sol";
import "./utils/CommitteeUpgradeable.sol";

/// @title BridgeLimiter
/// @notice A contract that limits the amount of tokens that can be bridged from a given chain within
/// a rolling 24-hour window. This is accomplished by storing the amount bridged from a given chain in USD
/// within a given hourly timestamp. It also provides functions to update the token prices and the total
/// limit of the given chainID measured in USD with a 4 decimal precision.
/// The contract is intended to be used and owned by the SuiBridge contract.
contract BridgeLimiter is IBridgeLimiter, CommitteeUpgradeable, OwnableUpgradeable {
    /* ========== STATE VARIABLES ========== */

    mapping(uint256 chainHourTimestamp => uint256 totalAmountBridged) public
        chainHourlyTransferAmount;
    // price in USD (4 decimal precision) (e.g. 1 ETH = 2000 USD => 20000000)
    mapping(uint8 tokenID => uint256 tokenPrice) public tokenPrices;
    // total limit in USD (4 decimal precision) (e.g. 10000000 => 1000 USD)
    mapping(uint8 chainID => uint64 totalLimit) public chainLimits;
    mapping(uint8 chainID => uint32 oldestHourTimestamp) public oldestChainTimestamp;

    /* ========== INITIALIZER ========== */

    /// @notice Initializes the BridgeLimiter contract with the provided parameters.
    /// @dev this function should be called directly after deployment (see OpenZeppelin upgradeable
    /// standards).
    /// @param _committee The address of the BridgeCommittee contract.
    /// @param _tokenPrices An array of token prices (with 4 decimal precision).
    /// @param chainIDs An array of chain IDs to limit.
    /// @param _totalLimits The total limit for the bridge (4 decimal precision).
    function initialize(
        address _committee,
        uint256[] memory _tokenPrices,
        uint8[] memory chainIDs,
        uint64[] memory _totalLimits
    ) external initializer {
        require(
            chainIDs.length == _totalLimits.length,
            "BridgeLimiter: invalid chainIDs and totalLimits length"
        );
        __CommitteeUpgradeable_init(_committee);
        __Ownable_init(msg.sender);
        for (uint8 i; i < _tokenPrices.length; i++) {
            tokenPrices[i] = _tokenPrices[i];
        }
        for (uint8 i; i < chainIDs.length; i++) {
            require(
                committee.config().isChainSupported(chainIDs[i]),
                "BridgeLimiter: Chain not supported"
            );
            chainLimits[chainIDs[i]] = _totalLimits[i];
            oldestChainTimestamp[chainIDs[i]] = currentHour();
        }
    }

    /* ========== VIEW FUNCTIONS ========== */

    /// @notice Returns whether the total amount, including the given token amount, will exceed the totalLimit.
    /// @dev The function will calculate the given token amount in USD.
    /// @param chainID The ID of the chain to check limit for.
    /// @param tokenID The ID of the token.
    /// @param amount The amount of the token.
    /// @return boolean indicating whether the total amount will exceed the limit.
    function willAmountExceedLimit(uint8 chainID, uint8 tokenID, uint256 amount)
        external
        view
        override
        returns (bool)
    {
        uint256 windowAmount = calculateWindowAmount(chainID);
        uint256 USDAmount = calculateAmountInUSD(tokenID, amount);
        return windowAmount + USDAmount > chainLimits[chainID];
    }

    /// @notice Returns whether the total amount, including the given USD amount, will exceed the totalLimit.
    /// @param amount The amount in USD.
    /// @return boolean indicating whether the total amount will exceed the limit.
    function willUSDAmountExceedLimit(uint8 chainID, uint256 amount) public view returns (bool) {
        uint256 windowAmount = calculateWindowAmount(chainID);
        return windowAmount + amount > chainLimits[chainID];
    }

    /// @dev Calculates the total transfer amount within the rolling 24-hour window.
    /// @return total transfer amount within the window.
    function calculateWindowAmount(uint8 chainID) public view returns (uint256 total) {
        uint32 _currentHour = currentHour();
        // aggregate the last 24 hours
        for (uint32 i; i < 24; i++) {
            uint256 key = getChainHourTimestampKey(chainID, _currentHour - i);
            total += chainHourlyTransferAmount[key];
        }
        return total;
    }

    /// @notice Calculates the given token amount in USD (4 decimal precision).
    /// @param tokenID The ID of the token.
    /// @param amount The amount of tokens.
    /// @return amount in USD (4 decimal precision).
    function calculateAmountInUSD(uint8 tokenID, uint256 amount) public view returns (uint256) {
        // get the token address
        address tokenAddress = committee.config().getTokenAddress(tokenID);
        // get the decimals
        uint8 decimals = IERC20Metadata(tokenAddress).decimals();

        return amount * tokenPrices[tokenID] / (10 ** decimals);
    }

    /// @notice Returns the current hour timestamp.
    /// @return current hour timestamp.
    function currentHour() public view returns (uint32) {
        return uint32(block.timestamp / 1 hours);
    }

    /// @notice Returns the key for the chain and hour timestamp.
    /// @param chainID The ID of the chain.
    /// @param hourTimestamp The hour timestamp.
    function getChainHourTimestampKey(uint8 chainID, uint32 hourTimestamp)
        public
        pure
        returns (uint256)
    {
        return (uint256(chainID) << 32) | uint256(hourTimestamp);
    }

    /* ========== EXTERNAL FUNCTIONS ========== */

    /// @notice Updates the bridge transfers for a specific token ID and amount. Only the contract
    /// owner can call this function (intended to be the SuiBridge contract).
    /// @dev The amount must be greater than 0 and must not exceed the rolling window limit.
    /// @param chainID The ID of the chain to record the transfer for.
    /// @param tokenID The ID of the token.
    /// @param amount The amount of tokens to be transferred.
    function recordBridgeTransfers(uint8 chainID, uint8 tokenID, uint256 amount)
        external
        override
        onlyOwner
    {
        require(amount > 0, "BridgeLimiter: amount must be greater than 0");
        uint256 usdAmount = calculateAmountInUSD(tokenID, amount);
        require(
            !willUSDAmountExceedLimit(chainID, usdAmount),
            "BridgeLimiter: amount exceeds rolling window limit"
        );

        uint32 _currentHour = currentHour();

        // garbage collect most recently expired hour if possible
        uint256 key = getChainHourTimestampKey(chainID, _currentHour - 25);
        if (chainHourlyTransferAmount[key] > 0) {
            delete chainHourlyTransferAmount[key];
        }

        // update key to current hour
        key = getChainHourTimestampKey(chainID, _currentHour);
        // update hourly transfers
        chainHourlyTransferAmount[key] += usdAmount;

        emit HourlyTransferAmountUpdated(_currentHour, usdAmount);
    }

    /// @notice Updates the token price with the provided message if the provided signatures are valid.
    /// @param signatures array of signatures to validate the message.
    /// @param message BridgeMessage containing the update token price payload.
    function updateTokenPriceWithSignatures(
        bytes[] memory signatures,
        BridgeMessage.Message memory message
    )
        external
        nonReentrant
        verifyMessageAndSignatures(message, signatures, BridgeMessage.UPDATE_TOKEN_PRICE)
    {
        // decode the update token payload
        (uint8 tokenID, uint64 price) = BridgeMessage.decodeUpdateTokenPricePayload(message.payload);

        // update the token price
        tokenPrices[tokenID] = price;

        emit AssetPriceUpdated(tokenID, price);
    }

    /// @notice Updates the total limit with the provided message if the provided signatures are valid.
    /// @param signatures array of signatures to validate the message.
    /// @param message The BridgeMessage containing the update limit payload.
    function updateLimitWithSignatures(
        bytes[] memory signatures,
        BridgeMessage.Message memory message
    )
        external
        nonReentrant
        verifyMessageAndSignatures(message, signatures, BridgeMessage.UPDATE_BRIDGE_LIMIT)
    {
        // decode the update limit payload
        (uint8 sourceChainID, uint64 newLimit) =
            BridgeMessage.decodeUpdateLimitPayload(message.payload);

        require(
            committee.config().isChainSupported(sourceChainID),
            "BridgeLimiter: Source chain not supported"
        );

        // update the chain limit
        chainLimits[sourceChainID] = newLimit;

        emit LimitUpdated(sourceChainID, newLimit);
    }
}
