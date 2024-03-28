// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./BridgeBaseTest.t.sol";

contract BridgeGasTest is BridgeBaseTest {
    // This function is called before each unit test
    function setUp() public {
        setUpBridgeTest();
    }

    // Uncomment to run these tests (must run tests with --via-ir flag)

    // function testTransferBridgedTokensWith7Signatures() public {
    //     // define committee with 50 members
    //     address[] memory _committee = new address[](56);
    //     uint256[] memory pks = new uint256[](56);
    //     uint16[] memory _stake = new uint16[](56);
    //     for (uint256 i = 0; i < 56; i++) {
    //         string memory name = string(abi.encodePacked("committeeMember", i));
    //         (address member, uint256 pk) = makeAddrAndKey(name);
    //         _committee[i] = member;
    //         pks[i] = pk;
    //         // 1 member with 2500 stake
    //         if (i == 55) {
    //             _stake[i] = 2500;
    //             // 50 members with 100 stake (total: 5000)
    //         } else if (i < 50) {
    //             _stake[i] = 100;
    //             // 5 members with 500 stake (total: 2500)
    //         } else {
    //             _stake[i] = 500;
    //         }
    //     }
    //     committee = new BridgeCommittee();
    //     committee.initialize(_committee, _stake, minStakeRequired);
    //     committee.initializeConfig(address(config));
    //     uint256[] memory tokenPrices = new uint256[](4);
    //     tokenPrices[0] = 10000; // SUI PRICE
    //     tokenPrices[1] = 10000; // BTC PRICE
    //     tokenPrices[2] = 10000; // ETH PRICE
    //     tokenPrices[3] = 10000; // USDC PRICE
    //     uint64[] memory totalLimits = new uint64[](1);
    //     totalLimits[0] = 1000000;
    //     skip(2 days);
    //     SuiBridge _bridge = new SuiBridge();
    //     _bridge.initialize(address(committee), address(vault), address(limiter), wETH);
    //     changePrank(address(bridge));
    //     limiter.transferOwnership(address(_bridge));
    //     vault.transferOwnership(address(_bridge));
    //     bridge = _bridge;

    //     // Fill vault with WETH
    //     changePrank(deployer);
    //     IWETH9(wETH).deposit{value: 10 ether}();
    //     IERC20(wETH).transfer(address(vault), 10 ether);

    //     // transfer bridged tokens with 7 signatures
    //     // Create transfer payload
    //     uint8 senderAddressLength = 32;
    //     bytes memory senderAddress = abi.encode(0);
    //     uint8 targetChain = chainID;
    //     uint8 recipientAddressLength = 20;
    //     address recipientAddress = bridgerA;
    //     uint8 tokenID = BridgeUtils.ETH;
    //     uint64 amount = 100000000; // 1 ether in sui decimals
    //     bytes memory payload = abi.encodePacked(
    //         senderAddressLength,
    //         senderAddress,
    //         targetChain,
    //         recipientAddressLength,
    //         recipientAddress,
    //         tokenID,
    //         amount
    //     );

    //     // Create transfer message
    //     BridgeUtils.Message memory message = BridgeUtils.Message({
    //         messageType: BridgeUtils.TOKEN_TRANSFER,
    //         version: 1,
    //         nonce: 1,
    //         chainID: 0,
    //         payload: payload
    //     });

    //     bytes memory encodedMessage = BridgeUtils.encodeMessage(message);

    //     bytes32 messageHash = keccak256(encodedMessage);

    //     bytes[] memory signatures = new bytes[](7);

    //     uint8 index;
    //     for (uint256 i = 50; i < 55; i++) {
    //         signatures[index++] = getSignature(messageHash, pks[i]);
    //     }
    //     signatures[5] = getSignature(messageHash, pks[55]);
    //     signatures[6] = getSignature(messageHash, pks[0]);

    //     bridge.transferBridgedTokensWithSignatures(signatures, message);
    // }

    // function testTransferBridgedTokensWith26Signatures() public {
    //     // define committee with 50 members
    //     address[] memory _committee = new address[](56);
    //     uint256[] memory pks = new uint256[](56);
    //     uint16[] memory _stake = new uint16[](56);
    //     for (uint256 i = 0; i < 56; i++) {
    //         string memory name = string(abi.encodePacked("committeeMember", i));
    //         (address member, uint256 pk) = makeAddrAndKey(name);
    //         _committee[i] = member;
    //         pks[i] = pk;
    //         // 1 member with 2500 stake
    //         if (i == 55) {
    //             _stake[i] = 2500;
    //             // 50 members with 100 stake (total: 5000)
    //         } else if (i < 50) {
    //             _stake[i] = 100;
    //             // 5 members with 500 stake (total: 2500)
    //         } else {
    //             _stake[i] = 500;
    //         }
    //     }
    //     committee = new BridgeCommittee();
    //     committee.initialize(_committee, _stake, minStakeRequired);
    //     committee.initializeConfig(address(config));
    //     uint256[] memory tokenPrices = new uint256[](4);
    //     tokenPrices[0] = 10000; // SUI PRICE
    //     tokenPrices[1] = 10000; // BTC PRICE
    //     tokenPrices[2] = 10000; // ETH PRICE
    //     tokenPrices[3] = 10000; // USDC PRICE
    //     uint64[] memory totalLimits = new uint64[](1);
    //     totalLimits[0] = 1000000;
    //     skip(2 days);
    //     SuiBridge _bridge = new SuiBridge();
    //     _bridge.initialize(address(committee), address(vault), address(limiter), wETH);
    //     changePrank(address(bridge));
    //     limiter.transferOwnership(address(_bridge));
    //     vault.transferOwnership(address(_bridge));
    //     bridge = _bridge;

    //     // Fill vault with WETH
    //     changePrank(deployer);
    //     IWETH9(wETH).deposit{value: 10 ether}();
    //     IERC20(wETH).transfer(address(vault), 10 ether);

    //     // transfer bridged tokens with 26 signatures

    //     // Create transfer payload
    //     uint8 senderAddressLength = 32;
    //     bytes memory senderAddress = abi.encode(0);
    //     uint8 targetChain = chainID;
    //     uint8 recipientAddressLength = 20;
    //     address recipientAddress = bridgerA;
    //     uint8 tokenID = BridgeUtils.ETH;
    //     uint64 amount = 100000000; // 1 ether in sui decimals
    //     bytes memory payload = abi.encodePacked(
    //         senderAddressLength,
    //         senderAddress,
    //         targetChain,
    //         recipientAddressLength,
    //         recipientAddress,
    //         tokenID,
    //         amount
    //     );

    //     // Create transfer message
    //     BridgeUtils.Message memory message = BridgeUtils.Message({
    //         messageType: BridgeUtils.TOKEN_TRANSFER,
    //         version: 1,
    //         nonce: 2,
    //         chainID: 0,
    //         payload: payload
    //     });

    //     bytes memory encodedMessage = BridgeUtils.encodeMessage(message);

    //     bytes32 messageHash = keccak256(encodedMessage);

    //     bytes[] memory signatures = new bytes[](25);

    //     uint256 index = 0;
    //     // add 5 committee members with 100 stake
    //     for (uint256 i = 50; i < 55; i++) {
    //         signatures[index++] = getSignature(messageHash, pks[i]);
    //     }
    //     // add last committee member with 2500 stake
    //     signatures[5] = getSignature(messageHash, pks[55]);

    //     // add 20 committee members with 100 stake
    //     for (uint256 i = 0; i < 20; i++) {
    //         signatures[index++] = getSignature(messageHash, pks[i]);
    //     }

    //     bridge.transferBridgedTokensWithSignatures(signatures, message);
    // }

    // function testTransferBridgedTokensWith56Signatures() public {
    //     // define committee with 50 members
    //     address[] memory _committee = new address[](56);
    //     uint256[] memory pks = new uint256[](56);
    //     uint16[] memory _stake = new uint16[](56);
    //     for (uint256 i = 0; i < 56; i++) {
    //         string memory name = string(abi.encodePacked("committeeMember", i));
    //         (address member, uint256 pk) = makeAddrAndKey(name);
    //         _committee[i] = member;
    //         pks[i] = pk;
    //         // 1 member with 2500 stake
    //         if (i == 55) {
    //             _stake[i] = 2500;
    //             // 50 members with 100 stake (total: 5000)
    //         } else if (i < 50) {
    //             _stake[i] = 100;
    //             // 5 members with 500 stake (total: 2500)
    //         } else {
    //             _stake[i] = 500;
    //         }
    //     }
    //     committee = new BridgeCommittee();
    //     committee.initialize(_committee, _stake, minStakeRequired);
    //     committee.initializeConfig(address(config));
    //     uint256[] memory tokenPrices = new uint256[](4);
    //     tokenPrices[0] = 10000; // SUI PRICE
    //     tokenPrices[1] = 10000; // BTC PRICE
    //     tokenPrices[2] = 10000; // ETH PRICE
    //     tokenPrices[3] = 10000; // USDC PRICE
    //     uint64[] memory totalLimits = new uint64[](1);
    //     totalLimits[0] = 1000000;
    //     skip(2 days);
    //     SuiBridge _bridge = new SuiBridge();
    //     _bridge.initialize(address(committee), address(vault), address(limiter), wETH);
    //     changePrank(address(bridge));
    //     limiter.transferOwnership(address(_bridge));
    //     vault.transferOwnership(address(_bridge));
    //     bridge = _bridge;

    //     // Fill vault with WETH
    //     changePrank(deployer);
    //     IWETH9(wETH).deposit{value: 10 ether}();
    //     IERC20(wETH).transfer(address(vault), 10 ether);

    //     // transfer bridged tokens with 56 signatures

    //     // Create transfer payload
    //     uint8 senderAddressLength = 32;
    //     bytes memory senderAddress = abi.encode(0);
    //     uint8 targetChain = chainID;
    //     uint8 recipientAddressLength = 20;
    //     address recipientAddress = bridgerA;
    //     uint8 tokenID = BridgeUtils.ETH;
    //     uint64 amount = 100000000; // 1 ether in sui decimals
    //     bytes memory payload = abi.encodePacked(
    //         senderAddressLength,
    //         senderAddress,
    //         targetChain,
    //         recipientAddressLength,
    //         recipientAddress,
    //         tokenID,
    //         amount
    //     );

    //     // Create transfer message
    //     BridgeUtils.Message memory message = BridgeUtils.Message({
    //         messageType: BridgeUtils.TOKEN_TRANSFER,
    //         version: 1,
    //         nonce: 3,
    //         chainID: 0,
    //         payload: payload
    //     });

    //     bytes memory encodedMessage = BridgeUtils.encodeMessage(message);

    //     bytes32 messageHash = keccak256(encodedMessage);

    //     bytes[] memory signatures = new bytes[](56);

    //     // get all signatures
    //     for (uint256 i = 0; i < 56; i++) {
    //         signatures[i] = getSignature(messageHash, pks[i]);
    //     }

    //     bridge.transferBridgedTokensWithSignatures(signatures, message);
    // }
}
