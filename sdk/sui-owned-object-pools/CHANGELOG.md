# Change Log


## 2.3.2
* Add support for sponsored transactions

## 2.2.0
* Fix race condition when modifying mainpool (e6a5b6a)

## v2.1.3
* Improve Main Pool split and update tests (bf025a9)

## v2.1.2
* Enhance release script

## v2.1.1
* Fix initial_setup - call this when you need to publish (496154a)
* Improve EShandler log messages and increment NUMBER_OF_TRANSACTIONS_TO_EXECUTE in tests (929aa17)


## v2.1.0
Small change - Fix txb declaration (d86ee99)
* Bring back case 2 - multiple admin caps (6f35b0e)
* Include transactions.ts to index.ts (59444f8)
* Change 0 pool objects error to warning (d8725cc)
* Fix test: concurrent transactions with AdminCaps (d4bebf7)
* Fix IncludeAdminCapStrategy (b9a5c7a)
* Test concurrent transactions with multiple AdminCaps (c2c7bff)
* Replace AdminCapTransactionBlockFacade with TransactionBlockWithLambda (WIP) (fd80fc1)
* Create AdminCapTransactionBlockFacade (WIP) (8a6074f)
* Merge pull request #118 from MystenLabs/tzalex/116-move-admincap-handling-responsibility-from-user-to-library (7ed7847)
* Include transactions.ts to index.ts (30c5297)
* Change 0 pool objects error to warning (7755d23)
* Fix test: concurrent transactions with AdminCaps (fd59dae)
* Fix IncludeAdminCapStrategy (b2d605b)
* Test concurrent transactions with multiple AdminCaps (e991688)
* Replace AdminCapTransactionBlockFacade with TransactionBlockWithLambda (WIP) (b47d680)
* Remove case 2: multiple AdminCap transactions (b6a90cc)
* Create AdminCapTransactionBlockFacade (WIP) (b9c569e)
* Change assertions of test "uses only the pool s coins for gas" (b5ab2d0)
* Update splitCoinAndTransferToSelf inside README - Case 1 (97551cf)
* Bring back the double coin creation at test setup (6865198)
* Add case 2 (WIP) (f88eb27)
* Add code for case 1 - needs to be tested (76c1b51)
* Add use-case sections to README (WIP) (c007d09)
* Increase sleep after setupAdmin to 5 seconds (0a5242c)
* Merge pull request #115 from MystenLabs/tzalex/108-handle-transaction-block-execution-failure-due-to-low-balance (0e81a2c)
* Resolve review comments - micro fixes (f9f47b2)
* Fix setupTestsHelper.assureAdminHasMoreThanEnoughCoins (9cdc25d)
* Update balance too in Pool.updateCoins (0e48d4b)
* Update IncludeAdminCapStrategy (c5819a6)
* Update README part regarding DefaultSplitStrategy (7884244)
* Include balance in PoolObject(s) and update SplitStrategy (e4c4565)
* Fix lint warnings (bdadece)
* Merge pull request #111 from MystenLabs/tzalex/82-update-the-lint-config-to-align-with-that-of-sui-js (4d046aa)
* Update .eslintrc.js to match the rules of @mysten/sui.js (e840070)
* Merge pull request #110 from MystenLabs/tzalex/101-add-splitstrategy-to-readme (0d25efc)
* Ignore smash coins test since it breaks build (9b93008)
* Remove smashCoins from inside test (8468eb0)
* Merge pull request #107 from MystenLabs/tzalex/106-fix-ci-build-fails-on-tests (2b56b94)
* Update test - make it pass (43ed1d1)
* Minot change - README (cfb9e61)
* Update DefaultSplitStrategy (2b9f70b)
* Update README (cd2599e)
* Ignore smash coins test since it breaks build (5fc1b72)
* Remove smashCoins from inside test (b143dbd)


## v2.0.6 - 2.0.7
- Update coins after transaction completes
- Improve some log messages
- ESHandler: Add options and requestType

## v2.0.4 - 2.0.5
- Packaging configuration fixes 

## v2.0.3
- Update tsconfig.json to compile to js files too

## v2.0.2
- Merge pull request #96 from MystenLabs/tzalex/95-handle-gasbalancetoolow-edge-case (d194cdb)
- Improve tests - minor fixes (ae5113b)
- Revert to using two splitCoins instead of one (0837848)
- Only smash coins when needed (37f061e)
- Refactor: Use only one txb.splitCoins instead of two (ddbbb8a)
- Fix: Use await when calling smashCoins (8e8d969)
- SmashCoins: Add another try-catch block (50a6470)
- Include more logs to debug tests (652cd77)
- Move helper constructor to the top of tests (c1f8f8a)
- Create splitStrategies.ts (760c347)
- Create setupTestsHelper.smashCoins for gas coin reset (368571c)

## v2.0.1
- minor fixes

## v2.0.0
- Renamed library to `suioop` (Sui Owned Object Pools)
- Refactor test logic to be isolated from the main library
- Introduction of logging library to standardize logs 
- Refactor logic in `getAWorker` function to remove busy wait

## v1.0.5
- Fixed bug about Immutable object handling
- Fixed bug about worker status update

## v1.0.4
- Performance enhancement with Lazy load the Pool objects only when needed.
- Added flowchart to README.md

## v1.0.3
 - Fixes on packaging and distribution format.
 - Introduction of this file (CHANGELOG.md) to track changes.
 - Introduction of .npmignore file to strictly define files from distribution.

## v1.0.2
First version of library published.
