// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../VRFConsumerV2.sol";
import "./mocks/MockVRFCoordinatorV2.sol";
import "./mocks/LinkToken.sol";
import "./utils/Cheats.sol";
import "forge-std/Test.sol";

contract VRFConsumerV2Test is Test {
    LinkToken public linkToken;
    MockVRFCoordinatorV2 public vrfCoordinator;
    VRFConsumerV2 public vrfConsumer;
    Cheats internal constant cheats = Cheats(HEVM_ADDRESS);

    uint96 constant FUND_AMOUNT = 1 * 10**18;

    // Initialized as blank, fine for testing
    uint64 subId;
    bytes32 keyHash; // gasLane

    event ReturnedRandomness(uint256[] randomWords);

    function setUp() public {
        linkToken = new LinkToken();
        vrfCoordinator = new MockVRFCoordinatorV2();
        subId = vrfCoordinator.createSubscription();
        vrfCoordinator.fundSubscription(subId, FUND_AMOUNT);
        vrfConsumer = new VRFConsumerV2(
            subId,
            address(vrfCoordinator),
            address(linkToken),
            keyHash
        );
        vrfCoordinator.addConsumer(subId, address(vrfConsumer));
    }

    function testCanRequestRandomness() public {
        uint256 startingRequestId = vrfConsumer.s_requestId();
        vrfConsumer.requestRandomWords();
        assertTrue(vrfConsumer.s_requestId() != startingRequestId);
    }

    function testCanGetRandomResponse() public {
        vrfConsumer.requestRandomWords();
        uint256 requestId = vrfConsumer.s_requestId();

        uint256[] memory words = getWords(requestId);

        // When testing locally you MUST call fulfillRandomness youself to get the
        // randomness to the consumer contract, since there isn't a chainlink node on your local network
        vrfCoordinator.fulfillRandomWords(requestId, address(vrfConsumer));
        assertTrue(vrfConsumer.s_randomWords(0) == words[0]);
        assertTrue(vrfConsumer.s_randomWords(1) == words[1]);
    }

    function testEmitsEventOnFulfillment() public {
        vrfConsumer.requestRandomWords();
        uint256 requestId = vrfConsumer.s_requestId();
        uint256[] memory words = getWords(requestId);

        cheats.expectEmit(false, false, false, true);
        emit ReturnedRandomness(words);
        // When testing locally you MUST call fulfillRandomness youself to get the
         // randomness to the consumer contract, since there isn't a chainlink node on your local network
        vrfCoordinator.fulfillRandomWords(requestId, address(vrfConsumer));
    }

    function getWords(uint256 requestId)
        public
        view
        returns (uint256[] memory)
    {
        uint256[] memory words = new uint256[](vrfConsumer.s_numWords());
        for (uint256 i = 0; i < vrfConsumer.s_numWords(); i++) {
            words[i] = uint256(keccak256(abi.encode(requestId, i)));
        }
        return words;
    }
}
