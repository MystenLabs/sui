//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.0;

import "@openzeppelin/contracts/utils/Address.sol";

contract Native_Sol {
    using Address for address;

    function getContractAddr() public view returns (address) {
        return address(this);
    }

    function getSenderAddr() public view returns (address) {
        return msg.sender;
    }

    function getIsContract(address addr) public view returns (bool) {
        return addr.isContract();
    }
}
