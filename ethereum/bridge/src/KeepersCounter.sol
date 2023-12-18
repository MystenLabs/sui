// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

import "@chainlink/contracts/src/v0.8/interfaces/KeeperCompatibleInterface.sol";

contract KeepersCounter is KeeperCompatibleInterface {
    uint256 public counter;
    uint256 public immutable interval;
    uint256 public lastTimeStamp;

    constructor(uint256 updateInterval) {
        interval = updateInterval;
        lastTimeStamp = block.timestamp;
        counter = 0;
    }

    function checkUpkeep(
        bytes memory /* checkData */
    )
        public
        view
        override
        returns (bool upkeepNeeded, bytes memory performData)
    {
        upkeepNeeded = (block.timestamp - lastTimeStamp) > interval;
        performData = bytes("");
    }

    function performUpkeep(
        bytes calldata /* performData */
    ) external override {
        // add some verification
        (bool upkeepNeeded, ) = checkUpkeep("");
        require(upkeepNeeded, "Time interval not met");

        lastTimeStamp = block.timestamp;
        counter = counter + 1;
    }
}
