// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";
import "./interfaces/IBridgeLimiter.sol";
import "./interfaces/IBridgeTokens.sol";
import "./utils/CommitteeUpgradeable.sol";

/// @title BridgeLimiter
/// @notice A contract that limits the amount of tokens that can be bridged within a rolling 24-hour
/// window. This is accomplished by storing the amount bridged in USD within a given hourly timestamp.
/// It also provides functions to update the token prices and the total limit of the bridge
/// measured in USD with a 4 decimal precision. The contract is intended to be used and owned by the
/// SuiBridge contract.
contract BridgeLimiter is IBridgeLimiter, CommitteeUpgradeable, OwnableUpgradeable {
    /* ========== STATE VARIABLES ========== */

    mapping(uint32 hourTimestamp => uint256 totalAmountBridged) public hourlyTransferAmount;
    // price in USD (4 decimal precision) (e.g. 1 ETH = 2000 USD => 20000000)
    mapping(uint8 tokenID => uint256 tokenPrice) public tokenPrices;
    // total limit in USD (4 decimal precision) (e.g. 10000000 => 1000 USD)
    uint64 public totalLimit;
    uint32 public oldestHourTimestamp;
    IBridgeTokens public tokens;

    /* ========== INITIALIZER ========== */

    /// @notice Initializes the BridgeLimiter contract with the provided parameters.
    /// @dev this function should be called directly after deployment (see OpenZeppelin upgradeable
    /// standards).
    /// @param _committee The address of the BridggeCommittee contract.
    /// @param _tokens The address of the BridgeTokens contract.
    /// @param _tokenPrices An array of token prices (with 4 decimal precision).
    /// @param _totalLimit The total limit for the bridge (4 decimal precision).
    function initialize(
        address _committee,
        address _tokens,
        uint256[] memory _tokenPrices,
        uint64 _totalLimit
    ) external initializer {
        __CommitteeUpgradeable_init(_committee);
        __Ownable_init(msg.sender);
        tokens = IBridgeTokens(_tokens);
        for (uint8 i; i < _tokenPrices.length; i++) {
            tokenPrices[i] = _tokenPrices[i];
        }
        totalLimit = _totalLimit;
        oldestHourTimestamp = currentHour();
    }

    /* ========== VIEW FUNCTIONS ========== */

    /// @notice Returns whether the total amount, including the given token amount, will exceed the totalLimit.
    /// @dev The function will calculate the given token amount in USD.
    /// @param tokenID The ID of the token.
    /// @param amount The amount of the token.
    /// @return boolean indicating whether the total amount will exceed the limit.
    function willAmountExceedLimit(uint8 tokenID, uint256 amount)
        public
        view
        override
        returns (bool)
    {
        uint256 windowAmount = calculateWindowAmount();
        uint256 USDAmount = calculateAmountInUSD(tokenID, amount);
        return windowAmount + USDAmount > totalLimit;
    }

    /// @notice Returns whether the total amount, including the given USD amount, will exceed the totalLimit.
    /// @param amount The amount in USD.
    /// @return boolean indicating whether the total amount will exceed the limit.
    function willUSDAmountExceedLimit(uint256 amount) public view returns (bool) {
        uint256 windowAmount = calculateWindowAmount();
        return windowAmount + amount > totalLimit;
    }

    /// @dev Calculates the total transfer amount within the rolling 24-hour window.
    /// @return total transfer amount within the window.
    function calculateWindowAmount() public view returns (uint256 total) {
        uint32 _currentHour = currentHour();
        // aggregate the last 24 hours
        for (uint32 i; i < 24; i++) {
            total += hourlyTransferAmount[_currentHour - i];
        }
        return total;
    }

    /// @notice Calculates the given token amount in USD (4 decimal precision).
    /// @param tokenID The ID of the token.
    /// @param amount The amount of tokens.
    /// @return amount in USD (4 decimal precision).
    function calculateAmountInUSD(uint8 tokenID, uint256 amount) public view returns (uint256) {
        // get the token address
        address tokenAddress = tokens.getAddress(tokenID);
        // get the decimals
        uint8 decimals = IERC20Metadata(tokenAddress).decimals();

        return amount * tokenPrices[tokenID] / (10 ** decimals);
    }

    /// @notice Returns the current hour timestamp.
    /// @return current hour timestamp.
    function currentHour() public view returns (uint32) {
        return uint32(block.timestamp / 1 hours);
    }

    /* ========== EXTERNAL FUNCTIONS ========== */

    /// @notice Updates the bridge transfers for a specific token ID and amount. Only the contract
    /// owner can call this function (intended to be the SuiBridge contract).
    /// @dev The amount must be greater than 0 and must not exceed the rolling window limit.
    /// @param tokenID The ID of the token.
    /// @param amount The amount of tokens to be transferred.
    function recordBridgeTransfers(uint8 tokenID, uint256 amount) external override onlyOwner {
        require(amount > 0, "BridgeLimiter: amount must be greater than 0");
        uint256 usdAmount = calculateAmountInUSD(tokenID, amount);
        require(
            !willUSDAmountExceedLimit(usdAmount),
            "BridgeLimiter: amount exceeds rolling window limit"
        );

        uint32 _currentHour = currentHour();

        // garbage collect most recently expired hour if possible
        if (hourlyTransferAmount[_currentHour - 25] > 0) {
            delete hourlyTransferAmount[_currentHour - 25];
        }

        // update hourly transfers
        hourlyTransferAmount[_currentHour] += usdAmount;

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

        // TODO: expose supportedChainIDs from SuiBridge contract and check that the sourceChainID is supported

        // update the limit
        totalLimit = newLimit;

        emit LimitUpdated(sourceChainID, newLimit);
    }
}
