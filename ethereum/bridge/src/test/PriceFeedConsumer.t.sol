// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

import "../PriceFeedConsumer.sol";
import "./mocks/MockV3Aggregator.sol";
import "forge-std/Test.sol";

contract PriceFeedConsumerTest is Test {
    uint8 public constant DECIMALS = 18;
    int256 public constant INITIAL_ANSWER = 1 * 10**18;
    PriceFeedConsumer public priceFeedConsumer;
    MockV3Aggregator public mockV3Aggregator;

    function setUp() public {
        mockV3Aggregator = new MockV3Aggregator(DECIMALS, INITIAL_ANSWER);
        priceFeedConsumer = new PriceFeedConsumer(address(mockV3Aggregator));
    }

    function testConsumerReturnsStartingValue() public {
        int256 price = priceFeedConsumer.getLatestPrice();
        assertTrue(price == INITIAL_ANSWER);
    }
}
