// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

import "@openzeppelin/contracts/token/ERC721/IERC721Receiver.sol";
import "hardhat/console.sol";


contract ERC721ReceiverMock is IERC721Receiver {
    enum Error {
        None,
        RevertWithMessage,
        RevertWithoutMessage,
        Panic
    }

    bytes4 private immutable _retval;
    Error private immutable _error;

    event Received(address operator, address from, uint256 tokenId, bytes data, uint256 gas);

    constructor(bytes4 retval, Error error) {
        _retval = retval;
        _error = error;
    }

    function onERC721Received(
        address operator,
        address from,
        uint256 tokenId,
        bytes memory data
    ) public override returns (bytes4) {
        if (_error == Error.RevertWithMessage) {
            // console.log('RevertWithMessage');
            revert("ERC721ReceiverMock: reverting");
        } else if (_error == Error.RevertWithoutMessage) {
            // console.log('RevertWithoutMessage');
            revert();
        } else if (_error == Error.Panic) {
            // console.log('Panic');
            uint256 a = uint256(0) / uint256(0);
            a;
        }
        emit Received(operator, from, tokenId, data, gasleft());
        //console.log('Ok');
        return _retval;
    }
}
