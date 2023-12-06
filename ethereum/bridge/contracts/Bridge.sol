// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {ERC721Upgradeable} from "@openzeppelin/contracts-upgradeable/token/ERC721/ERC721Upgradeable.sol";

import "./ChainIDs.sol";
import "./TokenIDs.sol";

// import {BridgeMessage} from "./interfaces/ICommon.sol";

// Interface for ERC20 token
// interface IERC20 {
//     function transfer(
//         address recipient,
//         uint256 amount
//     ) external returns (bool);

//     function balanceOf(address account) external view returns (uint256);
// }

// Bridge contract
contract Bridge is Initializable, UUPSUpgradeable, ERC721Upgradeable, ChainIDs {
    using SafeERC20 for IERC20;
    using MessageHashUtils for bytes32;

    uint256[48] __gap;

    // require(recoverSigner(message, signature) == from, "wrong signature");
    // require(processedNonces[from][nonce] == false, "transfer already processed");
    // processedNonces[from][nonce] = true;
    mapping(address => mapping(uint => bool)) public processedNonces;

    // uint8 private immutable version;
    // uint8 private version;

    bool public paused;

    uint16 public constant MAX_TOTAL_WEIGHT = 10000;
    uint256 public constant MAX_SINGLE_VALIDATOR_WEIGHT = 1000;
    uint256 public constant APPROVAL_THRESHOLD = 3333;

    // A struct to represent a validator
    struct Validator {
        address addr; // The address of the validator
        uint256 weight; // The weight of the validator
    }

    struct ApprovedBridgeMessage {
        BridgeMessage message;
        uint64 approvedEpoch;
        bytes[] signatures;
    }

    struct BridgeMessageKey {
        uint8 sourceChain;
        uint64 bridgeSeqNum;
    }

    struct BridgeMessage {
        // 0: token , 1: object ? TBD
        uint8 messageType;
        uint8 version;
        ChainID sourceChain;
        uint64 bridgeSeqNum;
        address senderAddress;
        uint8 targetChain;
        address targetAddress;
        // bytes payload;
    }

    // A mapping from address to validator index
    mapping(address => uint256) public validatorIndex;

    // An array to store the validators
    Validator[] public validators;

    // Mapping of user address to nonce
    mapping(address => uint256) public nonces;

    // Event to emit when a transfer is initiated
    event TransferInitiated(
        address indexed sender,
        address indexed recipient,
        uint256 amount,
        uint256 nonce
    );

    // Event to emit when a transfer is completed
    event TransferCompleted(
        address indexed sender,
        address indexed recipient,
        uint256 amount,
        uint256 nonce
    );

    event BridgeEvent(BridgeMessage message, bytes message_bytes);

    // Function to pause the bridge
    function pauseBridge() private isRunning {
        paused = true;
    }

    // Function to pause the bridge
    function resumeBridge() private isPaused {
        paused = false;
    }

    // modifier to check if bridge is running
    modifier isRunning() {
        // If the first argument of 'require' evaluates to 'false', execution terminates and all
        // changes to the state and to Ether balances are reverted.
        // This used to consume all gas in old EVM versions, but not anymore.
        // It is often a good idea to use 'require' to check if functions are called correctly.
        // As a second argument, you can also provide an explanation about what went wrong.
        require(paused == false, "Bridge is not Running");
        _;
    }

    // modifier to check if bridge is paused
    modifier isPaused() {
        // If the first argument of 'require' evaluates to 'false', execution terminates and all
        // changes to the state and to Ether balances are reverted.
        // This used to consume all gas in old EVM versions, but not anymore.
        // It is often a good idea to use 'require' to check if functions are called correctly.
        // As a second argument, you can also provide an explanation about what went wrong.
        require(paused == true, "Bridge is Paused");
        _;
    }

    // Event to emit when a transfer is initiated
    event ValidatorAdded(
        address addr, // The address of the validator
        uint256 weight // The weight of the validator
    );

    function initialize(Validator[] calldata _validators) public initializer {
        // addValidator(firstPK, firstWeight);
        // __Ownable_init();
        __UUPSUpgradeable_init();
        paused = false;

        for (uint256 i = 0; i < _validators.length; i++) {
            addValidator(_validators[i].addr, _validators[i].weight);
            emit ValidatorAdded(_validators[i].addr, _validators[i].weight);
        }
    }

    // constructor() {
    //     _disableInitializers();
    // }

    // /// @custom:oz-upgrades-unsafe-allow constructor
    // constructor() initializer {}

    /**
    // Record bridge message approvels in Sui, call by the bridge client
    public fun approve_bridge_message(
        self: &mut Bridge,
        raw_message: vector<u8>,
        signatures: vector<vector<u8>>,
        ctx: &TxContext
    ) {
        // varify signatures
        bridge_committee::verify_signatures(&self.committee, raw_message, signatures);
        let message = deserialise_message(raw_message);
        // retrieve pending message if source chain is Sui
        if (message.source_chain == chain_ids::sui()) {
            let key = BridgeMessageKey { source_chain: chain_ids::sui(), bridge_seq_num: message.seq_num };
            let recorded_message = table::remove(&mut self.pending_messages,key);
            let message_bytes = serialise_message(recorded_message);
            assert!(message_bytes == raw_message, EMalformedMessageError);
        };
        assert!(message.source_chain != chain_ids::sui(), EUnexpectedChainID);
        let approved_message = ApprovedBridgeMessage {
            message,
            approved_epoch: tx_context::epoch(ctx),
            signatures,
        };
        let key = BridgeMessageKey { source_chain: message.source_chain, bridge_seq_num: message.seq_num };
        // Store approval
        table::add(&mut self.approved_messages, key, approved_message);
    }
 */

    function testBridgeMessage(
        BridgeMessage calldata bridgeMessage
    )
        public
        pure
        returns (uint8, uint8, ChainID, uint64, address, uint8, address)
    {
        return (
            bridgeMessage.messageType,
            bridgeMessage.version,
            bridgeMessage.sourceChain,
            bridgeMessage.bridgeSeqNum,
            bridgeMessage.senderAddress,
            bridgeMessage.targetChain,
            bridgeMessage.targetAddress
        );
    }

    function approveBridgeMessage(
        BridgeMessage calldata bridgeMessage,
        bytes[] calldata signatures
    ) public isRunning returns (bool, uint256) {
        uint256 totalWeight = 0;
        // verify signatures
        bytes32 hash = ethSignedMessageHash(bridgeMessage);
        for (uint256 i = 0; i < signatures.length; i++) {
            address recoveredPK = recoverSigner(hash, signatures[i]);
            // Check if the address is not zero
            require(recoveredPK != address(0), "Zero address.");
            uint256 index = validatorIndex[recoveredPK] - 1;
            require(index < validators.length, "Index out of bounds");

            Validator memory validator = validators[index];
            require(recoveredPK == validator.addr, "Invalid signature");
            totalWeight += validator.weight;
        }

        if (bridgeMessage.messageType == 1) pauseBridge();

        return (true, totalWeight);
    }

    function resumePausedBridge(
        BridgeMessage calldata bridgeMessage,
        bytes[] calldata signatures
    ) public isPaused {
        uint256 totalWeight = 0;
        // verify signatures
        bytes32 hash = ethSignedMessageHash(bridgeMessage);
        for (uint256 i = 0; i < signatures.length; i++) {
            address recoveredPK = recoverSigner(hash, signatures[i]);
            // Check if the address is not zero
            require(recoveredPK != address(0), "Zero address.");
            uint256 index = validatorIndex[recoveredPK] - 1;
            require(index < validators.length, "Index out of bounds");

            Validator memory validator = validators[index];
            require(recoveredPK == validator.addr, "Invalid signature");
            totalWeight += validator.weight;
        }

        // TODO
        require(totalWeight >= 999, "Not enough total signature weight");
        if (bridgeMessage.messageType == 1) resumeBridge();
    }

    // Check also weight. i.e. no more than 33% of the total weight
    // A function to add a validator
    function addValidator(address _pk, uint256 _weight) private {
        // Check if the address is not zero
        require(_pk != address(0), "Zero address.");
        // Check if the address is not already a validator
        require(validatorIndex[_pk] == 0, "Already a validator.");
        // Add the validator to the array
        validators.push(Validator(_pk, _weight));
        // Update the validator index
        validatorIndex[_pk] = validators.length;
    }

    function validatorsCount() public view returns (uint count) {
        return validators.length;
    }

    // The contract can be upgraded by the owner
    function _authorizeUpgrade(address newImplementation) internal override {}

    // function _verify(
    //     bytes32 data,
    //     bytes memory signature,
    //     address account
    // ) internal pure returns (bool) {
    //     return data.toEthSignedMessageHash().recover(signature) == account;
    // }

    // function wrapEther() external payable {
    //     uint256 balanceBefore = IWETH(WETH).balanceOf(msg.sender);
    //     uint256 ETHAmount = msg.value;

    //     //create WETH from ETH
    //     if (ETHAmount != 0) {
    //         IWETH(WETH).deposit{value: ETHAmount}();
    //         IWETH(WETH).transfer(msg.sender, ETHAmount);
    //     }
    //     require(
    //         IWETH(WETH).balanceOf(msg.sender) - balanceBefore == ETHAmount,
    //         "Ethereum not deposited"
    //     );
    // }

    // //Extremely important!!!!
    // receive() external payable {}

    // function unwrapEther(uint256 Amount) external {
    //     address payable sender = msg.sender;

    //     if (Amount != 0) {
    //         IWETH(WETH).transferFrom(msg.sender, address(this), Amount);
    //         IWETH(WETH).withdraw(Amount);
    //         sender.transfer(address(this).balance);
    //     }
    // }

    // Function to initiate a transfer from the source chain to the destination chain
    function initiateTransfer(address recipient, uint256 amount) external {
        // Transfer the tokens from the sender to this contract
        // require(
        //     IERC20(token).transferFrom(msg.sender, address(this), amount),
        //     "Transfer failed"
        // );
    }

    // Function to complete a transfer from the destination chain to the source chain
    function completeTransfer(
        address sender,
        address recipient,
        uint256 amount,
        uint256 nonce,
        bytes memory signature
    ) private {
        // Verify that the nonce is correct
        require(
            processedNonces[recipient][nonce] == false,
            "transfer already processed"
        );

        // Verify that the signature is valid
        // require(
        //     verifySignature(sender, recipient, amount, nonce, signature),
        //     "Invalid signature"
        // );

        // Transfer the tokens from this contract to the recipient
        // require(IERC20(token).transfer(recipient, amount), "Transfer failed");

        // Increment the nonce for the recipient
        processedNonces[recipient][nonce] = true;

        // Emit the transfer completed event
        emit TransferCompleted(sender, recipient, amount, nonce);
    }

    // returning the contract's balance in wei
    function getBalance() public view returns (uint256) {
        return address(this).balance;
    }

    function transfer(address payable transferAddress, uint256 amount) public {
        transferAddress.transfer(amount);
    }

    // "0x93f82d7903c6a37336c33d68a890b448665735b4f513003cb4ef0029da0372b9329e0f6fc0b9f9c0c77d66bbf7217da260803fcae345a72f7a7764c56f464b5c1b"
    // [1 ,2 ,3 ,4 ,"0x5567f54B29B973343d632f7BFCe9507343D41FCa" ,5 ,"0x5567f54B29B973343d632f7BFCe9507343D41FCa"]
    function ethSignedMessageHash(
        BridgeMessage calldata bridgeMessage
    ) public pure returns (bytes32) {
        bytes32 hash = keccak256(
            abi.encodePacked(
                bridgeMessage.messageType,
                bridgeMessage.version,
                bridgeMessage.sourceChain,
                bridgeMessage.bridgeSeqNum,
                bridgeMessage.senderAddress,
                bridgeMessage.targetChain,
                bridgeMessage.targetAddress
            )
        );
        return MessageHashUtils.toEthSignedMessageHash(hash);

        // address signer = ECDSA.recover(message, signature);
        // return signer;
    }

    function ethSignedMessageHash(
        string memory message
    ) public pure returns (bytes32) {
        bytes32 hash = keccak256(abi.encodePacked(message));
        return MessageHashUtils.toEthSignedMessageHash(hash);
    }

    function recoverSigner(
        bytes32 hash,
        bytes calldata signature
    ) public pure returns (address) {
        return ECDSA.recover(hash, signature);
    }

    // function burn(address to, uint amount) external {
    //     token.burn(msg.sender, amount);
    // }

    // function mint(address to, uint amount, uint otherChainNonce) external {
    //     token.mint(to, amount);
    // }

    // function verifySignature(
    //     address sender,
    //     uint8 messageType,
    //     uint8 version,
    //     uint8 sourceChain,
    //     uint64 bridgeSeqNum,
    //     address senderAddress,
    //     uint8 targetChain,
    //     address targetAddress,
    //     bytes memory payload,
    //     bytes memory signature
    // ) public pure returns (bool) {
    //     // Recover the signer from the hash and the signature
    //     address signer = recoverSigner(
    //         // Hash the parameters
    //         computeHash(
    //             messageType,
    //             version,
    //             sourceChain,
    //             bridgeSeqNum,
    //             senderAddress,
    //             targetChain,
    //             targetAddress,
    //             payload
    //         ),
    //         signature
    //     );
    //     // Return true if the signer is the sender
    //     return (signer == sender);
    // }

    // https://github.com/Gravity-Bridge/Gravity-Bridge/blob/main/solidity/contracts/Gravity.sol#L153
    // Utility function to verify geth style signatures
    // function verifySignature(
    //     address _signer,
    //     bytes32 _theHash,
    //     bytes memory signature
    // ) public pure returns (bool) {
    //     bytes32 messageDigest = keccak256(
    //         abi.encodePacked("\x19Ethereum Signed Message:\n32", _theHash)
    //     );
    //     // Signature calldata _sig
    //     // return _signer == ECDSA.recover(messageDigest, _sig.v, _sig.r, _sig.s);
    //     return _signer == ECDSA.recover(messageDigest, signature);
    // }

    // function verifySignature(
    //     string memory message,
    //     bytes memory signature
    // )
    //     public
    //     pure
    //     returns (
    //         // ) public pure returns (address, ECDSA.RecoverError) {
    //         address,
    //         ECDSA.RecoverError,
    //         bytes32
    //     )
    // {
    //     // https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/utils/cryptography/MessageHashUtils.sol#L49
    //     bytes32 signedMessageHash = MessageHashUtils.toEthSignedMessageHash(
    //         bytes(message)
    //     );
    //     // https://docs.openzeppelin.com/contracts/4.x/api/utils#ECDSA-tryRecover-bytes32-bytes-
    //     return ECDSA.tryRecover(signedMessageHash, signature);
    // }

    // Function to verify the signature of the transfer
    // function verifySignature(
    //     address sender,
    //     address recipient,
    //     uint256 amount,
    //     uint256 nonce,
    //     bytes memory signature
    // ) public view returns (bool) {
    //     // Hash the parameters with the chain ID
    //     bytes32 hash = keccak256(
    //         abi.encodePacked(sender, recipient, amount, nonce, block.chainid)
    //     );
    //     // Recover the signer from the hash and the signature
    //     address signer = recoverSigner(hash, signature);
    //     // Return true if the signer is the sender
    //     return (signer == sender);
    // }

    // Function to recover the signer from the hash and the signature
    // function recoverSigner(
    //     bytes32 hash,
    //     bytes memory signature
    // ) public pure returns (address) {
    //     // Check the signature length
    //     require(signature.length == 65, "Invalid signature length");
    //     // Divide the signature into r, s and v variables
    //     bytes32 r;
    //     bytes32 s;
    //     uint8 v;
    //     assembly {
    //         r := mload(add(signature, 0x20))
    //         s := mload(add(signature, 0x40))
    //         v := byte(0, mload(add(signature, 0x60)))
    //     }
    //     // Return the address that signed the hash
    //     return ecrecover(hash, v, r, s);
    // }

    // https://github.com/anoma/ethereum-bridge/blob/main/src/Bridge.sol#L279
    // function _isValidSignature(
    //     address _signer,
    //     bytes32 _messageHash,
    //     // Signature calldata _signature
    //     bytes memory signature
    // ) internal pure returns (bool) {
    //     bytes32 messageDigest;
    //     assembly ("memory-safe") {
    //         let scratch := mload(0x40)

    //         mstore(scratch, "\x19Ethereum Signed Message:\n32\x00\x00\x00\x00")
    //         mstore(add(scratch, 28), _messageHash)

    //         messageDigest := keccak256(scratch, 60)
    //     }

    //     // (address recovered, ECDSA.RecoverError error) = ECDSA.tryRecover(
    //     (address recovered, ECDSA.RecoverError error, ) = ECDSA.tryRecover(
    //         messageDigest,
    //         signature
    //     );
    //     return error == ECDSA.RecoverError.NoError && recovered == _signer;
    // }

    // https://github.com/Gravity-Bridge/Gravity-Bridge/blob/main/solidity/contracts/Gravity.sol#L153

    // This represents a validator signature
    // struct Signature {
    //     uint8 v;
    //     bytes32 r;
    //     bytes32 s;
    // }

    // Utility function to verify geth style signatures
    // function verifyGethStyleSignature(
    //     address _signer,
    //     bytes32 _theHash,
    //     Signature calldata _sig
    // ) private pure returns (bool) {
    //     bytes32 messageDigest = keccak256(
    //         abi.encodePacked("\x19Ethereum Signed Message:\n32", _theHash)
    //     );
    //     return _signer == ECDSA.recover(messageDigest, _sig.v, _sig.r, _sig.s);
    // }

    // https://medium.com/coinmonks/how-to-build-a-decentralized-token-bridge-between-ethereum-and-binance-smart-chain-58de17441259

    // function prefixed(bytes32 hash) internal pure returns (bytes32) {
    //     return
    //         keccak256(
    //             abi.encodePacked("\x19Ethereum Signed Message:\n32", hash)
    //         );
    // }

    // function recoverSignerMedium(
    //     bytes32 message,
    //     bytes memory sig
    // ) internal pure returns (address) {
    //     uint8 v;
    //     bytes32 r;
    //     bytes32 s;
    //     (v, r, s) = splitSignature(sig);
    //     return ecrecover(message, v, r, s);
    // }

    // function splitSignature(
    //     bytes memory sig
    // ) internal pure returns (uint8, bytes32, bytes32) {
    //     require(sig.length == 65);
    //     bytes32 r;
    //     bytes32 s;
    //     uint8 v;
    //     assembly {
    //         // first 32 bytes, after the length prefix
    //         r := mload(add(sig, 32))
    //         // second 32 bytes
    //         s := mload(add(sig, 64))
    //         // final byte (first byte of the next 32 bytes)
    //         v := byte(0, mload(add(sig, 96)))
    //     }
    //     return (v, r, s);
    // }
}
