const { BN } = require('@openzeppelin/test-helpers');
const { expect } = require('chai');
const { range } = require('../../../scripts/helpers');
const { expectRevertCustomError } = require('../../helpers/customError');

const SafeCast = artifacts.require('$SafeCast');

contract('SafeCast', async function () {
  beforeEach(async function () {
    this.safeCast = await SafeCast.new();
  });

  function testToUint(bits) {
    describe(`toUint${bits}`, () => {
      const maxValue = new BN('2').pow(new BN(bits)).subn(1);

      it('downcasts 0', async function () {
        expect(await this.safeCast[`$toUint${bits}`](0)).to.be.bignumber.equal('0');
      });

      it('downcasts 1', async function () {
        expect(await this.safeCast[`$toUint${bits}`](1)).to.be.bignumber.equal('1');
      });

      it(`downcasts 2^${bits} - 1 (${maxValue})`, async function () {
        expect(await this.safeCast[`$toUint${bits}`](maxValue)).to.be.bignumber.equal(maxValue);
      });

      it(`reverts when downcasting 2^${bits} (${maxValue.addn(1)})`, async function () {
        await expectRevertCustomError(
          this.safeCast[`$toUint${bits}`](maxValue.addn(1)),
          `SafeCastOverflowedUintDowncast`,
          [bits, maxValue.addn(1)],
        );
      });

      it(`reverts when downcasting 2^${bits} + 1 (${maxValue.addn(2)})`, async function () {
        await expectRevertCustomError(
          this.safeCast[`$toUint${bits}`](maxValue.addn(2)),
          `SafeCastOverflowedUintDowncast`,
          [bits, maxValue.addn(2)],
        );
      });
    });
  }

  range(8, 256, 8).forEach(bits => testToUint(bits));

  describe('toUint256', () => {
    const maxInt256 = new BN('2').pow(new BN(255)).subn(1);
    const minInt256 = new BN('2').pow(new BN(255)).neg();

    it('casts 0', async function () {
      expect(await this.safeCast.$toUint256(0)).to.be.bignumber.equal('0');
    });

    it('casts 1', async function () {
      expect(await this.safeCast.$toUint256(1)).to.be.bignumber.equal('1');
    });

    it(`casts INT256_MAX (${maxInt256})`, async function () {
      expect(await this.safeCast.$toUint256(maxInt256)).to.be.bignumber.equal(maxInt256);
    });

    it('reverts when casting -1', async function () {
      await expectRevertCustomError(this.safeCast.$toUint256(-1), `SafeCastOverflowedIntToUint`, [-1]);
    });

    it(`reverts when casting INT256_MIN (${minInt256})`, async function () {
      await expectRevertCustomError(this.safeCast.$toUint256(minInt256), `SafeCastOverflowedIntToUint`, [minInt256]);
    });
  });

  function testToInt(bits) {
    describe(`toInt${bits}`, () => {
      const minValue = new BN('-2').pow(new BN(bits - 1));
      const maxValue = new BN('2').pow(new BN(bits - 1)).subn(1);

      it('downcasts 0', async function () {
        expect(await this.safeCast[`$toInt${bits}`](0)).to.be.bignumber.equal('0');
      });

      it('downcasts 1', async function () {
        expect(await this.safeCast[`$toInt${bits}`](1)).to.be.bignumber.equal('1');
      });

      it('downcasts -1', async function () {
        expect(await this.safeCast[`$toInt${bits}`](-1)).to.be.bignumber.equal('-1');
      });

      it(`downcasts -2^${bits - 1} (${minValue})`, async function () {
        expect(await this.safeCast[`$toInt${bits}`](minValue)).to.be.bignumber.equal(minValue);
      });

      it(`downcasts 2^${bits - 1} - 1 (${maxValue})`, async function () {
        expect(await this.safeCast[`$toInt${bits}`](maxValue)).to.be.bignumber.equal(maxValue);
      });

      it(`reverts when downcasting -2^${bits - 1} - 1 (${minValue.subn(1)})`, async function () {
        await expectRevertCustomError(
          this.safeCast[`$toInt${bits}`](minValue.subn(1)),
          `SafeCastOverflowedIntDowncast`,
          [bits, minValue.subn(1)],
        );
      });

      it(`reverts when downcasting -2^${bits - 1} - 2 (${minValue.subn(2)})`, async function () {
        await expectRevertCustomError(
          this.safeCast[`$toInt${bits}`](minValue.subn(2)),
          `SafeCastOverflowedIntDowncast`,
          [bits, minValue.subn(2)],
        );
      });

      it(`reverts when downcasting 2^${bits - 1} (${maxValue.addn(1)})`, async function () {
        await expectRevertCustomError(
          this.safeCast[`$toInt${bits}`](maxValue.addn(1)),
          `SafeCastOverflowedIntDowncast`,
          [bits, maxValue.addn(1)],
        );
      });

      it(`reverts when downcasting 2^${bits - 1} + 1 (${maxValue.addn(2)})`, async function () {
        await expectRevertCustomError(
          this.safeCast[`$toInt${bits}`](maxValue.addn(2)),
          `SafeCastOverflowedIntDowncast`,
          [bits, maxValue.addn(2)],
        );
      });
    });
  }

  range(8, 256, 8).forEach(bits => testToInt(bits));

  describe('toInt256', () => {
    const maxUint256 = new BN('2').pow(new BN(256)).subn(1);
    const maxInt256 = new BN('2').pow(new BN(255)).subn(1);

    it('casts 0', async function () {
      expect(await this.safeCast.$toInt256(0)).to.be.bignumber.equal('0');
    });

    it('casts 1', async function () {
      expect(await this.safeCast.$toInt256(1)).to.be.bignumber.equal('1');
    });

    it(`casts INT256_MAX (${maxInt256})`, async function () {
      expect(await this.safeCast.$toInt256(maxInt256)).to.be.bignumber.equal(maxInt256);
    });

    it(`reverts when casting INT256_MAX + 1 (${maxInt256.addn(1)})`, async function () {
      await expectRevertCustomError(this.safeCast.$toInt256(maxInt256.addn(1)), 'SafeCastOverflowedUintToInt', [
        maxInt256.addn(1),
      ]);
    });

    it(`reverts when casting UINT256_MAX (${maxUint256})`, async function () {
      await expectRevertCustomError(this.safeCast.$toInt256(maxUint256), 'SafeCastOverflowedUintToInt', [maxUint256]);
    });
  });
});
