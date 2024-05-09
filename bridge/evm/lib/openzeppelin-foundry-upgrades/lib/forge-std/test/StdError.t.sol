// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0 <0.9.0;

import "../src/StdError.sol";
import "../src/Test.sol";

contract StdErrorsTest is Test {
    ErrorsTest test;

    function setUp() public {
        test = new ErrorsTest();
    }

    function test_ExpectAssertion() public {
        vm.expectRevert(stdError.assertionError);
        test.assertionError();
    }

    function test_ExpectArithmetic() public {
        vm.expectRevert(stdError.arithmeticError);
        test.arithmeticError(10);
    }

    function test_ExpectDiv() public {
        vm.expectRevert(stdError.divisionError);
        test.divError(0);
    }

    function test_ExpectMod() public {
        vm.expectRevert(stdError.divisionError);
        test.modError(0);
    }

    function test_ExpectEnum() public {
        vm.expectRevert(stdError.enumConversionError);
        test.enumConversion(1);
    }

    function test_ExpectEncodeStg() public {
        vm.expectRevert(stdError.encodeStorageError);
        test.encodeStgError();
    }

    function test_ExpectPop() public {
        vm.expectRevert(stdError.popError);
        test.pop();
    }

    function test_ExpectOOB() public {
        vm.expectRevert(stdError.indexOOBError);
        test.indexOOBError(1);
    }

    function test_ExpectMem() public {
        vm.expectRevert(stdError.memOverflowError);
        test.mem();
    }

    function test_ExpectIntern() public {
        vm.expectRevert(stdError.zeroVarError);
        test.intern();
    }
}

contract ErrorsTest {
    enum T {
        T1
    }

    uint256[] public someArr;
    bytes someBytes;

    function assertionError() public pure {
        assert(false);
    }

    function arithmeticError(uint256 a) public pure {
        a -= 100;
    }

    function divError(uint256 a) public pure {
        100 / a;
    }

    function modError(uint256 a) public pure {
        100 % a;
    }

    function enumConversion(uint256 a) public pure {
        T(a);
    }

    function encodeStgError() public {
        /// @solidity memory-safe-assembly
        assembly {
            sstore(someBytes.slot, 1)
        }
        keccak256(someBytes);
    }

    function pop() public {
        someArr.pop();
    }

    function indexOOBError(uint256 a) public pure {
        uint256[] memory t = new uint256[](0);
        t[a];
    }

    function mem() public pure {
        uint256 l = 2 ** 256 / 32;
        new uint256[](l);
    }

    function intern() public returns (uint256) {
        function(uint256) internal returns (uint256) x;
        x(2);
        return 7;
    }
}
