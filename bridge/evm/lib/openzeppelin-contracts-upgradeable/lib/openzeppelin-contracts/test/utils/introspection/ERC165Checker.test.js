require('@openzeppelin/test-helpers');

const { expect } = require('chai');

const ERC165Checker = artifacts.require('$ERC165Checker');
const ERC165MissingData = artifacts.require('ERC165MissingData');
const ERC165MaliciousData = artifacts.require('ERC165MaliciousData');
const ERC165NotSupported = artifacts.require('ERC165NotSupported');
const ERC165InterfacesSupported = artifacts.require('ERC165InterfacesSupported');
const ERC165ReturnBombMock = artifacts.require('ERC165ReturnBombMock');

const DUMMY_ID = '0xdeadbeef';
const DUMMY_ID_2 = '0xcafebabe';
const DUMMY_ID_3 = '0xdecafbad';
const DUMMY_UNSUPPORTED_ID = '0xbaddcafe';
const DUMMY_UNSUPPORTED_ID_2 = '0xbaadcafe';
const DUMMY_ACCOUNT = '0x1111111111111111111111111111111111111111';

contract('ERC165Checker', function () {
  beforeEach(async function () {
    this.mock = await ERC165Checker.new();
  });

  context('ERC165 missing return data', function () {
    beforeEach(async function () {
      this.target = await ERC165MissingData.new();
    });

    it('does not support ERC165', async function () {
      const supported = await this.mock.$supportsERC165(this.target.address);
      expect(supported).to.equal(false);
    });

    it('does not support mock interface via supportsInterface', async function () {
      const supported = await this.mock.$supportsInterface(this.target.address, DUMMY_ID);
      expect(supported).to.equal(false);
    });

    it('does not support mock interface via supportsAllInterfaces', async function () {
      const supported = await this.mock.$supportsAllInterfaces(this.target.address, [DUMMY_ID]);
      expect(supported).to.equal(false);
    });

    it('does not support mock interface via getSupportedInterfaces', async function () {
      const supported = await this.mock.$getSupportedInterfaces(this.target.address, [DUMMY_ID]);
      expect(supported.length).to.equal(1);
      expect(supported[0]).to.equal(false);
    });

    it('does not support mock interface via supportsERC165InterfaceUnchecked', async function () {
      const supported = await this.mock.$supportsERC165InterfaceUnchecked(this.target.address, DUMMY_ID);
      expect(supported).to.equal(false);
    });
  });

  context('ERC165 malicious return data', function () {
    beforeEach(async function () {
      this.target = await ERC165MaliciousData.new();
    });

    it('does not support ERC165', async function () {
      const supported = await this.mock.$supportsERC165(this.target.address);
      expect(supported).to.equal(false);
    });

    it('does not support mock interface via supportsInterface', async function () {
      const supported = await this.mock.$supportsInterface(this.target.address, DUMMY_ID);
      expect(supported).to.equal(false);
    });

    it('does not support mock interface via supportsAllInterfaces', async function () {
      const supported = await this.mock.$supportsAllInterfaces(this.target.address, [DUMMY_ID]);
      expect(supported).to.equal(false);
    });

    it('does not support mock interface via getSupportedInterfaces', async function () {
      const supported = await this.mock.$getSupportedInterfaces(this.target.address, [DUMMY_ID]);
      expect(supported.length).to.equal(1);
      expect(supported[0]).to.equal(false);
    });

    it('does not support mock interface via supportsERC165InterfaceUnchecked', async function () {
      const supported = await this.mock.$supportsERC165InterfaceUnchecked(this.target.address, DUMMY_ID);
      expect(supported).to.equal(true);
    });
  });

  context('ERC165 not supported', function () {
    beforeEach(async function () {
      this.target = await ERC165NotSupported.new();
    });

    it('does not support ERC165', async function () {
      const supported = await this.mock.$supportsERC165(this.target.address);
      expect(supported).to.equal(false);
    });

    it('does not support mock interface via supportsInterface', async function () {
      const supported = await this.mock.$supportsInterface(this.target.address, DUMMY_ID);
      expect(supported).to.equal(false);
    });

    it('does not support mock interface via supportsAllInterfaces', async function () {
      const supported = await this.mock.$supportsAllInterfaces(this.target.address, [DUMMY_ID]);
      expect(supported).to.equal(false);
    });

    it('does not support mock interface via getSupportedInterfaces', async function () {
      const supported = await this.mock.$getSupportedInterfaces(this.target.address, [DUMMY_ID]);
      expect(supported.length).to.equal(1);
      expect(supported[0]).to.equal(false);
    });

    it('does not support mock interface via supportsERC165InterfaceUnchecked', async function () {
      const supported = await this.mock.$supportsERC165InterfaceUnchecked(this.target.address, DUMMY_ID);
      expect(supported).to.equal(false);
    });
  });

  context('ERC165 supported', function () {
    beforeEach(async function () {
      this.target = await ERC165InterfacesSupported.new([]);
    });

    it('supports ERC165', async function () {
      const supported = await this.mock.$supportsERC165(this.target.address);
      expect(supported).to.equal(true);
    });

    it('does not support mock interface via supportsInterface', async function () {
      const supported = await this.mock.$supportsInterface(this.target.address, DUMMY_ID);
      expect(supported).to.equal(false);
    });

    it('does not support mock interface via supportsAllInterfaces', async function () {
      const supported = await this.mock.$supportsAllInterfaces(this.target.address, [DUMMY_ID]);
      expect(supported).to.equal(false);
    });

    it('does not support mock interface via getSupportedInterfaces', async function () {
      const supported = await this.mock.$getSupportedInterfaces(this.target.address, [DUMMY_ID]);
      expect(supported.length).to.equal(1);
      expect(supported[0]).to.equal(false);
    });

    it('does not support mock interface via supportsERC165InterfaceUnchecked', async function () {
      const supported = await this.mock.$supportsERC165InterfaceUnchecked(this.target.address, DUMMY_ID);
      expect(supported).to.equal(false);
    });
  });

  context('ERC165 and single interface supported', function () {
    beforeEach(async function () {
      this.target = await ERC165InterfacesSupported.new([DUMMY_ID]);
    });

    it('supports ERC165', async function () {
      const supported = await this.mock.$supportsERC165(this.target.address);
      expect(supported).to.equal(true);
    });

    it('supports mock interface via supportsInterface', async function () {
      const supported = await this.mock.$supportsInterface(this.target.address, DUMMY_ID);
      expect(supported).to.equal(true);
    });

    it('supports mock interface via supportsAllInterfaces', async function () {
      const supported = await this.mock.$supportsAllInterfaces(this.target.address, [DUMMY_ID]);
      expect(supported).to.equal(true);
    });

    it('supports mock interface via getSupportedInterfaces', async function () {
      const supported = await this.mock.$getSupportedInterfaces(this.target.address, [DUMMY_ID]);
      expect(supported.length).to.equal(1);
      expect(supported[0]).to.equal(true);
    });

    it('supports mock interface via supportsERC165InterfaceUnchecked', async function () {
      const supported = await this.mock.$supportsERC165InterfaceUnchecked(this.target.address, DUMMY_ID);
      expect(supported).to.equal(true);
    });
  });

  context('ERC165 and many interfaces supported', function () {
    beforeEach(async function () {
      this.supportedInterfaces = [DUMMY_ID, DUMMY_ID_2, DUMMY_ID_3];
      this.target = await ERC165InterfacesSupported.new(this.supportedInterfaces);
    });

    it('supports ERC165', async function () {
      const supported = await this.mock.$supportsERC165(this.target.address);
      expect(supported).to.equal(true);
    });

    it('supports each interfaceId via supportsInterface', async function () {
      for (const interfaceId of this.supportedInterfaces) {
        const supported = await this.mock.$supportsInterface(this.target.address, interfaceId);
        expect(supported).to.equal(true);
      }
    });

    it('supports all interfaceIds via supportsAllInterfaces', async function () {
      const supported = await this.mock.$supportsAllInterfaces(this.target.address, this.supportedInterfaces);
      expect(supported).to.equal(true);
    });

    it('supports none of the interfaces queried via supportsAllInterfaces', async function () {
      const interfaceIdsToTest = [DUMMY_UNSUPPORTED_ID, DUMMY_UNSUPPORTED_ID_2];

      const supported = await this.mock.$supportsAllInterfaces(this.target.address, interfaceIdsToTest);
      expect(supported).to.equal(false);
    });

    it('supports not all of the interfaces queried via supportsAllInterfaces', async function () {
      const interfaceIdsToTest = [...this.supportedInterfaces, DUMMY_UNSUPPORTED_ID];

      const supported = await this.mock.$supportsAllInterfaces(this.target.address, interfaceIdsToTest);
      expect(supported).to.equal(false);
    });

    it('supports all interfaceIds via getSupportedInterfaces', async function () {
      const supported = await this.mock.$getSupportedInterfaces(this.target.address, this.supportedInterfaces);
      expect(supported.length).to.equal(3);
      expect(supported[0]).to.equal(true);
      expect(supported[1]).to.equal(true);
      expect(supported[2]).to.equal(true);
    });

    it('supports none of the interfaces queried via getSupportedInterfaces', async function () {
      const interfaceIdsToTest = [DUMMY_UNSUPPORTED_ID, DUMMY_UNSUPPORTED_ID_2];

      const supported = await this.mock.$getSupportedInterfaces(this.target.address, interfaceIdsToTest);
      expect(supported.length).to.equal(2);
      expect(supported[0]).to.equal(false);
      expect(supported[1]).to.equal(false);
    });

    it('supports not all of the interfaces queried via getSupportedInterfaces', async function () {
      const interfaceIdsToTest = [...this.supportedInterfaces, DUMMY_UNSUPPORTED_ID];

      const supported = await this.mock.$getSupportedInterfaces(this.target.address, interfaceIdsToTest);
      expect(supported.length).to.equal(4);
      expect(supported[0]).to.equal(true);
      expect(supported[1]).to.equal(true);
      expect(supported[2]).to.equal(true);
      expect(supported[3]).to.equal(false);
    });

    it('supports each interfaceId via supportsERC165InterfaceUnchecked', async function () {
      for (const interfaceId of this.supportedInterfaces) {
        const supported = await this.mock.$supportsERC165InterfaceUnchecked(this.target.address, interfaceId);
        expect(supported).to.equal(true);
      }
    });
  });

  context('account address does not support ERC165', function () {
    it('does not support ERC165', async function () {
      const supported = await this.mock.$supportsERC165(DUMMY_ACCOUNT);
      expect(supported).to.equal(false);
    });

    it('does not support mock interface via supportsInterface', async function () {
      const supported = await this.mock.$supportsInterface(DUMMY_ACCOUNT, DUMMY_ID);
      expect(supported).to.equal(false);
    });

    it('does not support mock interface via supportsAllInterfaces', async function () {
      const supported = await this.mock.$supportsAllInterfaces(DUMMY_ACCOUNT, [DUMMY_ID]);
      expect(supported).to.equal(false);
    });

    it('does not support mock interface via getSupportedInterfaces', async function () {
      const supported = await this.mock.$getSupportedInterfaces(DUMMY_ACCOUNT, [DUMMY_ID]);
      expect(supported.length).to.equal(1);
      expect(supported[0]).to.equal(false);
    });

    it('does not support mock interface via supportsERC165InterfaceUnchecked', async function () {
      const supported = await this.mock.$supportsERC165InterfaceUnchecked(DUMMY_ACCOUNT, DUMMY_ID);
      expect(supported).to.equal(false);
    });
  });

  it('Return bomb resistance', async function () {
    this.target = await ERC165ReturnBombMock.new();

    const tx1 = await this.mock.$supportsInterface.sendTransaction(this.target.address, DUMMY_ID);
    expect(tx1.receipt.gasUsed).to.be.lessThan(120000); // 3*30k + 21k + some margin

    const tx2 = await this.mock.$getSupportedInterfaces.sendTransaction(this.target.address, [
      DUMMY_ID,
      DUMMY_ID_2,
      DUMMY_ID_3,
      DUMMY_UNSUPPORTED_ID,
      DUMMY_UNSUPPORTED_ID_2,
    ]);
    expect(tx2.receipt.gasUsed).to.be.lessThan(250000); // (2+5)*30k + 21k + some margin
  });
});
