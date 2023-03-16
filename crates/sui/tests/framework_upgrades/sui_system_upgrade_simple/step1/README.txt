The first step of the upgrade, comparing to the framework code in sui_system_upgrade_base, contains the following
changes:
1. Defined `SuiSystemStateInnerV2` in sui_system_state_inner.move.
2. Modified `upgrade_system_state` function in sui_system_state_inner.move to migrate system state object from V1 to V2.