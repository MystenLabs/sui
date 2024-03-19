import { generateFilters } from '../events/parameterize';

describe('generateFilters', () => {
  it('should generate correct filters', () => {
    // Mock data
    const data = {
      eventPackages: ['package1', 'package2'],
      eventModules: ['module1', 'module2'],
      eventTypes: ['type1', 'type2'],
      senders: ['sender1', 'sender2'],
      emittingPackages: ['emittingPackage1', 'emittingPackage2'],
      emittingModules: ['emittingModule1', 'emittingModule2'],
    };

    // Expected result
    const expected = [
      { eventType: 'type1', sender: 'sender1', emittingModule: 'emittingModule1' },
      // ... other expected filters ...
    ];

    // Generate filters
    const filters = Array.from(generateFilters(data));

    // Check that the generated filters match the expected result
    expect(filters).toEqual(expected);
  });
});
