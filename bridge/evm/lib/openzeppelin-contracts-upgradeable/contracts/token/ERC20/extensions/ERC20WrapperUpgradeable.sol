// SPDX-License-Identifier: MIT
// OpenZeppelin Contracts (last updated v5.0.0) (token/ERC20/extensions/ERC20Wrapper.sol)

pragma solidity ^0.8.20;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {IERC20Metadata} from "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";
import {ERC20Upgradeable} from "../ERC20Upgradeable.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {Initializable} from "../../../proxy/utils/Initializable.sol";

/**
 * @dev Extension of the ERC20 token contract to support token wrapping.
 *
 * Users can deposit and withdraw "underlying tokens" and receive a matching number of "wrapped tokens". This is useful
 * in conjunction with other modules. For example, combining this wrapping mechanism with {ERC20Votes} will allow the
 * wrapping of an existing "basic" ERC20 into a governance token.
 */
abstract contract ERC20WrapperUpgradeable is Initializable, ERC20Upgradeable {
    /// @custom:storage-location erc7201:openzeppelin.storage.ERC20Wrapper
    struct ERC20WrapperStorage {
        IERC20 _underlying;
    }

    // keccak256(abi.encode(uint256(keccak256("openzeppelin.storage.ERC20Wrapper")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant ERC20WrapperStorageLocation = 0x3b5a617e0d4c238430871a64fe18212794b0c8d05a4eac064a8c9039fb5e0700;

    function _getERC20WrapperStorage() private pure returns (ERC20WrapperStorage storage $) {
        assembly {
            $.slot := ERC20WrapperStorageLocation
        }
    }

    /**
     * @dev The underlying token couldn't be wrapped.
     */
    error ERC20InvalidUnderlying(address token);

    function __ERC20Wrapper_init(IERC20 underlyingToken) internal onlyInitializing {
        __ERC20Wrapper_init_unchained(underlyingToken);
    }

    function __ERC20Wrapper_init_unchained(IERC20 underlyingToken) internal onlyInitializing {
        ERC20WrapperStorage storage $ = _getERC20WrapperStorage();
        if (underlyingToken == this) {
            revert ERC20InvalidUnderlying(address(this));
        }
        $._underlying = underlyingToken;
    }

    /**
     * @dev See {ERC20-decimals}.
     */
    function decimals() public view virtual override returns (uint8) {
        ERC20WrapperStorage storage $ = _getERC20WrapperStorage();
        try IERC20Metadata(address($._underlying)).decimals() returns (uint8 value) {
            return value;
        } catch {
            return super.decimals();
        }
    }

    /**
     * @dev Returns the address of the underlying ERC-20 token that is being wrapped.
     */
    function underlying() public view returns (IERC20) {
        ERC20WrapperStorage storage $ = _getERC20WrapperStorage();
        return $._underlying;
    }

    /**
     * @dev Allow a user to deposit underlying tokens and mint the corresponding number of wrapped tokens.
     */
    function depositFor(address account, uint256 value) public virtual returns (bool) {
        ERC20WrapperStorage storage $ = _getERC20WrapperStorage();
        address sender = _msgSender();
        if (sender == address(this)) {
            revert ERC20InvalidSender(address(this));
        }
        if (account == address(this)) {
            revert ERC20InvalidReceiver(account);
        }
        SafeERC20.safeTransferFrom($._underlying, sender, address(this), value);
        _mint(account, value);
        return true;
    }

    /**
     * @dev Allow a user to burn a number of wrapped tokens and withdraw the corresponding number of underlying tokens.
     */
    function withdrawTo(address account, uint256 value) public virtual returns (bool) {
        ERC20WrapperStorage storage $ = _getERC20WrapperStorage();
        if (account == address(this)) {
            revert ERC20InvalidReceiver(account);
        }
        _burn(_msgSender(), value);
        SafeERC20.safeTransfer($._underlying, account, value);
        return true;
    }

    /**
     * @dev Mint wrapped token to cover any underlyingTokens that would have been transferred by mistake. Internal
     * function that can be exposed with access control if desired.
     */
    function _recover(address account) internal virtual returns (uint256) {
        ERC20WrapperStorage storage $ = _getERC20WrapperStorage();
        uint256 value = $._underlying.balanceOf(address(this)) - totalSupply();
        _mint(account, value);
        return value;
    }
}
