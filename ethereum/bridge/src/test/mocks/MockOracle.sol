// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "@chainlink/contracts/src/v0.8/interfaces/ChainlinkRequestInterface.sol";
import "@chainlink/contracts/src/v0.8/interfaces/LinkTokenInterface.sol";

/**
 * @title The LinkTokenReceiver contract - used for the MockOracle below
 */
abstract contract LinkTokenReceiver {
    bytes4 private constant ORACLE_REQUEST_SELECTOR = 0x40429946;
    uint256 private constant SELECTOR_LENGTH = 4;
    uint256 private constant EXPECTED_REQUEST_WORDS = 2;
    uint256 private constant MINIMUM_REQUEST_LENGTH =
        SELECTOR_LENGTH + (32 * EXPECTED_REQUEST_WORDS);

    /**
     * @notice Called when LINK is sent to the contract via `transferAndCall`
     * @dev The data payload's first 2 words will be overwritten by the `_sender` and `_amount`
     * values to ensure correctness. Calls oracleRequest.
     * @param _sender Address of the sender
     * @param _amount Amount of LINK sent (specified in wei)
     * @param _data Payload of the transaction
     */
    function onTokenTransfer(
        address _sender,
        uint256 _amount,
        bytes memory _data
    )
        public
        onlyLINK
        validRequestLength(_data)
        permittedFunctionsForLINK(_data)
    {
        assembly {
            // solhint-disable-next-line avoid-low-level-calls
            mstore(add(_data, 36), _sender) // ensure correct sender is passed
            // solhint-disable-next-line avoid-low-level-calls
            mstore(add(_data, 68), _amount) // ensure correct amount is passed
        }
        // solhint-disable-next-line avoid-low-level-calls
        (bool success, ) = address(this).delegatecall(_data); // calls oracleRequest
        require(success, "Unable to create request");
    }

    function getChainlinkToken() public view virtual returns (address);

    /**
     * @dev Reverts if not sent from the LINK token
     */
    modifier onlyLINK() {
        require(msg.sender == getChainlinkToken(), "Must use LINK token");
        _;
    }

    /**
     * @dev Reverts if the given data does not begin with the `oracleRequest` function selector
     * @param _data The data payload of the request
     */
    modifier permittedFunctionsForLINK(bytes memory _data) {
        bytes4 funcSelector;
        assembly {
            // solhint-disable-next-line avoid-low-level-calls
            funcSelector := mload(add(_data, 32))
        }
        require(
            funcSelector == ORACLE_REQUEST_SELECTOR,
            "Must use whitelisted functions"
        );
        _;
    }

    /**
     * @dev Reverts if the given payload is less than needed to create a request
     * @param _data The request payload
     */
    modifier validRequestLength(bytes memory _data) {
        require(
            _data.length >= MINIMUM_REQUEST_LENGTH,
            "Invalid request length"
        );
        _;
    }
}

/**
 * @title The Chainlink Mock Oracle contract
 * @notice Chainlink smart contract developers can use this to test their contracts
 */
contract MockOracle is ChainlinkRequestInterface, LinkTokenReceiver {
    uint256 public constant EXPIRY_TIME = 5 minutes;
    uint256 private constant MINIMUM_CONSUMER_GAS_LIMIT = 400000;

    struct Request {
        address callbackAddr;
        bytes4 callbackFunctionId;
    }

    LinkTokenInterface internal LinkToken;
    mapping(bytes32 => Request) private commitments;

    event OracleRequest(
        bytes32 indexed specId,
        address requester,
        bytes32 requestId,
        uint256 payment,
        address callbackAddr,
        bytes4 callbackFunctionId,
        uint256 cancelExpiration,
        uint256 dataVersion,
        bytes data
    );

    event CancelOracleRequest(bytes32 indexed requestId);

    /**
     * @notice Deploy with the address of the LINK token
     * @dev Sets the LinkToken address for the imported LinkTokenInterface
     * @param _link The address of the LINK token
     */
    constructor(address _link) {
        LinkToken = LinkTokenInterface(_link); // external but already deployed and unalterable
    }

    /**
     * @notice Creates the Chainlink request
     * @dev Stores the hash of the params as the on-chain commitment for the request.
     * Emits OracleRequest event for the Chainlink node to detect.
     * @param _sender The sender of the request
     * @param _payment The amount of payment given (specified in wei)
     * @param _specId The Job Specification ID
     * @param _callbackAddress The callback address for the response
     * @param _callbackFunctionId The callback function ID for the response
     * @param _nonce The nonce sent by the requester
     * @param _dataVersion The specified data version
     * @param _data The CBOR payload of the request
     */
    function oracleRequest(
        address _sender,
        uint256 _payment,
        bytes32 _specId,
        address _callbackAddress,
        bytes4 _callbackFunctionId,
        uint256 _nonce,
        uint256 _dataVersion,
        bytes calldata _data
    ) external override onlyLINK checkCallbackAddress(_callbackAddress) {
        bytes32 requestId = keccak256(abi.encodePacked(_sender, _nonce));
        require(
            commitments[requestId].callbackAddr == address(0),
            "Must use a unique ID"
        );
        // solhint-disable-next-line not-rely-on-time
        uint256 expiration = block.timestamp + EXPIRY_TIME;

        commitments[requestId] = Request(_callbackAddress, _callbackFunctionId);

        emit OracleRequest(
            _specId,
            _sender,
            requestId,
            _payment,
            _callbackAddress,
            _callbackFunctionId,
            expiration,
            _dataVersion,
            _data
        );
    }

    /**
     * @notice Called by the Chainlink node to fulfill requests
     * @dev Given params must hash back to the commitment stored from `oracleRequest`.
     * Will call the callback address' callback function without bubbling up error
     * checking in a `require` so that the node can get paid.
     * @param _requestId The fulfillment request ID that must match the requester's
     * @param _data The data to return to the consuming contract
     * @return Status if the external call was successful
     */
    function fulfillOracleRequest(bytes32 _requestId, bytes32 _data)
        external
        isValidRequest(_requestId)
        returns (bool)
    {
        Request memory req = commitments[_requestId];
        delete commitments[_requestId];
        require(
            gasleft() >= MINIMUM_CONSUMER_GAS_LIMIT,
            "Must provide consumer enough gas"
        );
        // All updates to the oracle's fulfillment should come before calling the
        // callback(addr+functionId) as it is untrusted.
        // See: https://solidity.readthedocs.io/en/develop/security-considerations.html#use-the-checks-effects-interactions-pattern
        (bool success, ) = req.callbackAddr.call(
            abi.encodeWithSelector(req.callbackFunctionId, _requestId, _data)
        ); // solhint-disable-line avoid-low-level-calls
        return success;
    }

    /**
     * @notice Allows requesters to cancel requests sent to this oracle contract. Will transfer the LINK
     * sent for the request back to the requester's address.
     * @dev Given params must hash to a commitment stored on the contract in order for the request to be valid
     * Emits CancelOracleRequest event.
     * @param _requestId The request ID
     * @param _payment The amount of payment given (specified in wei)
     * @param _expiration The time of the expiration for the request
     */
    function cancelOracleRequest(
        bytes32 _requestId,
        uint256 _payment,
        bytes4,
        uint256 _expiration
    ) external override {
        require(
            commitments[_requestId].callbackAddr != address(0),
            "Must use a unique ID"
        );
        // solhint-disable-next-line not-rely-on-time
        require(_expiration <= block.timestamp, "Request is not expired");

        delete commitments[_requestId];
        emit CancelOracleRequest(_requestId);

        assert(LinkToken.transfer(msg.sender, _payment));
    }

    /**
     * @notice Returns the address of the LINK token
     * @dev This is the public implementation for chainlinkTokenAddress, which is
     * an internal method of the ChainlinkClient contract
     */
    function getChainlinkToken() public view override returns (address) {
        return address(LinkToken);
    }

    // MODIFIERS

    /**
     * @dev Reverts if request ID does not exist
     * @param _requestId The given request ID to check in stored `commitments`
     */
    modifier isValidRequest(bytes32 _requestId) {
        require(
            commitments[_requestId].callbackAddr != address(0),
            "Must have a valid requestId"
        );
        _;
    }

    /**
     * @dev Reverts if the callback address is the LINK token
     * @param _to The callback address
     */
    modifier checkCallbackAddress(address _to) {
        require(_to != address(LinkToken), "Cannot callback to LINK");
        _;
    }
}
