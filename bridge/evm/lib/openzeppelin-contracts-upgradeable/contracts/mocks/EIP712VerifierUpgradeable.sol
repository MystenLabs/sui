// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {EIP712Upgradeable} from "../utils/cryptography/EIP712Upgradeable.sol";
import {Initializable} from "../proxy/utils/Initializable.sol";

abstract contract EIP712VerifierUpgradeable is Initializable, EIP712Upgradeable {
    function __EIP712Verifier_init() internal onlyInitializing {
    }

    function __EIP712Verifier_init_unchained() internal onlyInitializing {
    }
    function verify(bytes memory signature, address signer, address mailTo, string memory mailContents) external view {
        bytes32 digest = _hashTypedDataV4(
            keccak256(abi.encode(keccak256("Mail(address to,string contents)"), mailTo, keccak256(bytes(mailContents))))
        );
        address recoveredSigner = ECDSA.recover(digest, signature);
        require(recoveredSigner == signer);
    }
}
