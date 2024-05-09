// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0 <0.9.0;

import {Test} from "../src/Test.sol";
import {Vm, VmSafe} from "../src/Vm.sol";

contract VmTest is Test {
    // This test ensures that functions are never accidentally removed from a Vm interface, or
    // inadvertently moved between Vm and VmSafe. This test must be updated each time a function is
    // added to or removed from Vm or VmSafe.
    function test_interfaceId() public pure {
        assertEq(type(VmSafe).interfaceId, bytes4(0xce9c7617), "VmSafe");
        assertEq(type(Vm).interfaceId, bytes4(0xaf68a970), "Vm");
    }
}
