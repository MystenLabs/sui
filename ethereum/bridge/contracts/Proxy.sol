//SPDX-License-Identifier: GPL-3.0
pragma solidity ^0.8.20;

import "./interfaces/IProxy.sol";

contract Proxy is IProxy {
    mapping(bytes32 => address) private contractStorage;
    mapping(address => bool) private existContractStorage;
    address private owner;
    bool private initialiazed = false;

    constructor() {
        owner = msg.sender;
    }

    function completeContractInit() external {
        require(owner == msg.sender, "Must be called by owner.");

        string memory contractName = "bridge";
        require(
            __getContract(contractName) != address(0),
            "Bridge contract must exist."
        );

        owner = address(0);
        initialiazed = true;
    }

    function upgradeContract(
        string calldata _name,
        address _address
    ) external override bridgeOrOwner {
        require(owner == address(0), "Proxy must be initialized.");
        require(_address != address(0), "Invalid address.");

        address oldAddress = _getContract(_name);
        require(oldAddress != address(0), "Invalid contract.");
        require(oldAddress != _address, "Address must be different.");

        _deleteExistContract(oldAddress);
        _setExistContract(_address);
        _setContract(_name, _address);
    }

    function addContract(
        string calldata _name,
        address _address
    ) external override bridgeOrOwner {
        require(
            _getContract(_name) == address(0),
            "Contract name already exist."
        );
        require(_address != address(0), "Invalid contract address.");
        require(!_getExistContract(_address), "Invalid duplicate address.");

        _setContract(_name, _address);
        _setExistContract(_address);
    }

    function getContract(
        string calldata _name
    ) external view override returns (address) {
        return contractStorage[keccak256(abi.encode(_name))];
    }

    function _getContract(
        string calldata _name
    ) private view returns (address) {
        return contractStorage[keccak256(abi.encode(_name))];
    }

    // by duplicating the function we can save some gas on later invocations
    function __getContract(string memory _name) private view returns (address) {
        return contractStorage[keccak256(abi.encode(_name))];
    }

    function _setContract(string calldata _name, address _address) private {
        contractStorage[keccak256(abi.encode(_name))] = _address;
    }

    function _setExistContract(address _address) private {
        existContractStorage[_address] = true;
    }

    function _deleteExistContract(address _address) private {
        delete existContractStorage[_address];
    }

    function _getExistContract(address _address) private view returns (bool) {
        return existContractStorage[_address];
    }

    modifier bridgeOrOwner() {
        if (initialiazed) {
            string memory contractName = "bridge";
            require(
                __getContract(contractName) == msg.sender,
                "Invalid caller address."
            );
        } else {
            require(owner == msg.sender, "Caller is not owner.");
        }
        _;
    }
}
