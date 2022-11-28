import { describe, it, expect, beforeAll } from 'vitest';
import { setup, TestToolbox } from './utils/setup';


describe('Not empty object validation', () => {
  let toolbox: TestToolbox;

  beforeAll(async () => {
    toolbox = await setup();
  });
  
  it('Test that functions work properly with valid ids', async () => {
    const objba = await toolbox.provider.getObjectsOwnedByAddress('0x37e86ca3d95b95c0f8ecbe06c71d925b3b75470b');
    expect(objba.length).to.greaterThan(0);
    const obj = await toolbox.provider.getObject('0x0bee40ba4b2ae861a0cd1d58f6ff7732af9ea2d0');
    expect(obj.status).to.equal('Exists');
    const trn = await toolbox.provider.getTransactionWithEffects('y6EeM/iJOig8O8ElGPCH9iDGGTDNQgRfULWo32PQiMg=');
    expect(trn.certificate).toBeTruthy();
  })

  it('Test all functions with invalid Sui Address', async () => {
    expect(toolbox.provider.getObjectsOwnedByAddress('0xree86ca3d95b95c0f8ecbe06c71d925b3b75470b')).rejects.toThrowError(/Invalid Sui address/);
    expect(toolbox.provider.getTransactionsForAddress('QQQ')).rejects.toThrowError(/Invalid Sui address/);
  })

  it('Test all functions with invalid Object Id', async () => {
    expect(toolbox.provider.getObject('')).rejects.toThrowError(/Invalid Sui Object id/);
    expect(toolbox.provider.getObjectsOwnedByObject('0x4ce52ee7b659b610d59a1ced129291b3d0d421632')).rejects.toThrowError(/Invalid Sui Object id/);
    expect(toolbox.provider.getTransactionsForObject('4ce52ee7b659b610d59a1ced129291b3d0d421632')).rejects.toThrowError(/Invalid Sui Object id/);
  })

  it('Test all functions with invalid Transaction Digest', async () => {
    expect(toolbox.provider.getTransactionWithEffects('')).rejects.toThrowError(/Invalid Transaction digest/);
  })
});