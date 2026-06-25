mod _accessor_impls {
    #![allow(clippy::useless_conversion)]
    impl super::BalanceDelta {
        pub const fn const_default() -> Self {
            Self {
                coin: ::prost::bytes::Bytes::new(),
                address: ::prost::bytes::Bytes::new(),
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::BalanceDelta = super::BalanceDelta::const_default();
            &DEFAULT
        }
        ///Sets `coin` with the provided value.
        pub fn set_coin<T: Into<::prost::bytes::Bytes>>(&mut self, field: T) {
            self.coin = field.into().into();
        }
        ///Sets `coin` with the provided value.
        pub fn with_coin<T: Into<::prost::bytes::Bytes>>(mut self, field: T) -> Self {
            self.set_coin(field.into());
            self
        }
        ///Sets `address` with the provided value.
        pub fn set_address<T: Into<::prost::bytes::Bytes>>(&mut self, field: T) {
            self.address = field.into().into();
        }
        ///Sets `address` with the provided value.
        pub fn with_address<T: Into<::prost::bytes::Bytes>>(mut self, field: T) -> Self {
            self.set_address(field.into());
            self
        }
    }
    impl super::BitmapBlob {
        pub const fn const_default() -> Self {
            Self {
                data: ::prost::bytes::Bytes::new(),
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::BitmapBlob = super::BitmapBlob::const_default();
            &DEFAULT
        }
        ///Sets `data` with the provided value.
        pub fn set_data<T: Into<::prost::bytes::Bytes>>(&mut self, field: T) {
            self.data = field.into().into();
        }
        ///Sets `data` with the provided value.
        pub fn with_data<T: Into<::prost::bytes::Bytes>>(mut self, field: T) -> Self {
            self.set_data(field.into());
            self
        }
    }
    impl super::PackageVersionInfo {
        pub const fn const_default() -> Self {
            Self {
                storage_id: ::prost::bytes::Bytes::new(),
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::PackageVersionInfo = super::PackageVersionInfo::const_default();
            &DEFAULT
        }
        ///Sets `storage_id` with the provided value.
        pub fn set_storage_id<T: Into<::prost::bytes::Bytes>>(&mut self, field: T) {
            self.storage_id = field.into().into();
        }
        ///Sets `storage_id` with the provided value.
        pub fn with_storage_id<T: Into<::prost::bytes::Bytes>>(
            mut self,
            field: T,
        ) -> Self {
            self.set_storage_id(field.into());
            self
        }
    }
    impl super::PruningWatermarks {
        pub const fn const_default() -> Self {
            Self {
                tx_seq_lo: 0,
                checkpoint_lo: 0,
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::PruningWatermarks = super::PruningWatermarks::const_default();
            &DEFAULT
        }
        ///Returns a mutable reference to `tx_seq_lo`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn tx_seq_lo_mut(&mut self) -> &mut u64 {
            &mut self.tx_seq_lo
        }
        ///Sets `tx_seq_lo` with the provided value.
        pub fn set_tx_seq_lo(&mut self, field: u64) {
            self.tx_seq_lo = field;
        }
        ///Sets `tx_seq_lo` with the provided value.
        pub fn with_tx_seq_lo(mut self, field: u64) -> Self {
            self.set_tx_seq_lo(field);
            self
        }
        ///Returns a mutable reference to `checkpoint_lo`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn checkpoint_lo_mut(&mut self) -> &mut u64 {
            &mut self.checkpoint_lo
        }
        ///Sets `checkpoint_lo` with the provided value.
        pub fn set_checkpoint_lo(&mut self, field: u64) {
            self.checkpoint_lo = field;
        }
        ///Sets `checkpoint_lo` with the provided value.
        pub fn with_checkpoint_lo(mut self, field: u64) -> Self {
            self.set_checkpoint_lo(field);
            self
        }
    }
    impl super::StoredCheckpointContents {
        pub const fn const_default() -> Self {
            Self {
                bcs: ::prost::bytes::Bytes::new(),
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::StoredCheckpointContents = super::StoredCheckpointContents::const_default();
            &DEFAULT
        }
        ///Sets `bcs` with the provided value.
        pub fn set_bcs<T: Into<::prost::bytes::Bytes>>(&mut self, field: T) {
            self.bcs = field.into().into();
        }
        ///Sets `bcs` with the provided value.
        pub fn with_bcs<T: Into<::prost::bytes::Bytes>>(mut self, field: T) -> Self {
            self.set_bcs(field.into());
            self
        }
    }
    impl super::StoredCheckpointSummary {
        pub const fn const_default() -> Self {
            Self {
                summary_bcs: ::prost::bytes::Bytes::new(),
                signature_bcs: ::prost::bytes::Bytes::new(),
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::StoredCheckpointSummary = super::StoredCheckpointSummary::const_default();
            &DEFAULT
        }
        ///Sets `summary_bcs` with the provided value.
        pub fn set_summary_bcs<T: Into<::prost::bytes::Bytes>>(&mut self, field: T) {
            self.summary_bcs = field.into().into();
        }
        ///Sets `summary_bcs` with the provided value.
        pub fn with_summary_bcs<T: Into<::prost::bytes::Bytes>>(
            mut self,
            field: T,
        ) -> Self {
            self.set_summary_bcs(field.into());
            self
        }
        ///Sets `signature_bcs` with the provided value.
        pub fn set_signature_bcs<T: Into<::prost::bytes::Bytes>>(&mut self, field: T) {
            self.signature_bcs = field.into().into();
        }
        ///Sets `signature_bcs` with the provided value.
        pub fn with_signature_bcs<T: Into<::prost::bytes::Bytes>>(
            mut self,
            field: T,
        ) -> Self {
            self.set_signature_bcs(field.into());
            self
        }
    }
    impl super::StoredEffects {
        pub const fn const_default() -> Self {
            Self {
                bcs: ::prost::bytes::Bytes::new(),
                unchanged_loaded_bcs: ::prost::bytes::Bytes::new(),
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::StoredEffects = super::StoredEffects::const_default();
            &DEFAULT
        }
        ///Sets `bcs` with the provided value.
        pub fn set_bcs<T: Into<::prost::bytes::Bytes>>(&mut self, field: T) {
            self.bcs = field.into().into();
        }
        ///Sets `bcs` with the provided value.
        pub fn with_bcs<T: Into<::prost::bytes::Bytes>>(mut self, field: T) -> Self {
            self.set_bcs(field.into());
            self
        }
        ///Sets `unchanged_loaded_bcs` with the provided value.
        pub fn set_unchanged_loaded_bcs<T: Into<::prost::bytes::Bytes>>(
            &mut self,
            field: T,
        ) {
            self.unchanged_loaded_bcs = field.into().into();
        }
        ///Sets `unchanged_loaded_bcs` with the provided value.
        pub fn with_unchanged_loaded_bcs<T: Into<::prost::bytes::Bytes>>(
            mut self,
            field: T,
        ) -> Self {
            self.set_unchanged_loaded_bcs(field.into());
            self
        }
    }
    impl super::StoredEpoch {
        pub const fn const_default() -> Self {
            Self {
                protocol_version: None,
                reference_gas_price: None,
                start_timestamp_ms: None,
                end_timestamp_ms: None,
                start_checkpoint: None,
                end_checkpoint: None,
                system_state_bcs: None,
                tx_hi: None,
                safe_mode: None,
                total_stake: None,
                storage_fund_balance: None,
                storage_fund_reinvestment: None,
                storage_charge: None,
                storage_rebate: None,
                stake_subsidy_amount: None,
                total_gas_fees: None,
                total_stake_rewards_distributed: None,
                leftover_storage_fund_inflow: None,
                epoch_commitments: None,
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::StoredEpoch = super::StoredEpoch::const_default();
            &DEFAULT
        }
        ///If `protocol_version` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn protocol_version_opt_mut(&mut self) -> Option<&mut u64> {
            self.protocol_version.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `protocol_version`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn protocol_version_mut(&mut self) -> &mut u64 {
            self.protocol_version.get_or_insert_default()
        }
        ///If `protocol_version` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn protocol_version_opt(&self) -> Option<u64> {
            self.protocol_version.as_ref().map(|field| *field)
        }
        ///Sets `protocol_version` with the provided value.
        pub fn set_protocol_version(&mut self, field: u64) {
            self.protocol_version = Some(field);
        }
        ///Sets `protocol_version` with the provided value.
        pub fn with_protocol_version(mut self, field: u64) -> Self {
            self.set_protocol_version(field);
            self
        }
        ///If `reference_gas_price` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn reference_gas_price_opt_mut(&mut self) -> Option<&mut u64> {
            self.reference_gas_price.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `reference_gas_price`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn reference_gas_price_mut(&mut self) -> &mut u64 {
            self.reference_gas_price.get_or_insert_default()
        }
        ///If `reference_gas_price` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn reference_gas_price_opt(&self) -> Option<u64> {
            self.reference_gas_price.as_ref().map(|field| *field)
        }
        ///Sets `reference_gas_price` with the provided value.
        pub fn set_reference_gas_price(&mut self, field: u64) {
            self.reference_gas_price = Some(field);
        }
        ///Sets `reference_gas_price` with the provided value.
        pub fn with_reference_gas_price(mut self, field: u64) -> Self {
            self.set_reference_gas_price(field);
            self
        }
        ///If `start_timestamp_ms` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn start_timestamp_ms_opt_mut(&mut self) -> Option<&mut u64> {
            self.start_timestamp_ms.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `start_timestamp_ms`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn start_timestamp_ms_mut(&mut self) -> &mut u64 {
            self.start_timestamp_ms.get_or_insert_default()
        }
        ///If `start_timestamp_ms` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn start_timestamp_ms_opt(&self) -> Option<u64> {
            self.start_timestamp_ms.as_ref().map(|field| *field)
        }
        ///Sets `start_timestamp_ms` with the provided value.
        pub fn set_start_timestamp_ms(&mut self, field: u64) {
            self.start_timestamp_ms = Some(field);
        }
        ///Sets `start_timestamp_ms` with the provided value.
        pub fn with_start_timestamp_ms(mut self, field: u64) -> Self {
            self.set_start_timestamp_ms(field);
            self
        }
        ///If `end_timestamp_ms` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn end_timestamp_ms_opt_mut(&mut self) -> Option<&mut u64> {
            self.end_timestamp_ms.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `end_timestamp_ms`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn end_timestamp_ms_mut(&mut self) -> &mut u64 {
            self.end_timestamp_ms.get_or_insert_default()
        }
        ///If `end_timestamp_ms` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn end_timestamp_ms_opt(&self) -> Option<u64> {
            self.end_timestamp_ms.as_ref().map(|field| *field)
        }
        ///Sets `end_timestamp_ms` with the provided value.
        pub fn set_end_timestamp_ms(&mut self, field: u64) {
            self.end_timestamp_ms = Some(field);
        }
        ///Sets `end_timestamp_ms` with the provided value.
        pub fn with_end_timestamp_ms(mut self, field: u64) -> Self {
            self.set_end_timestamp_ms(field);
            self
        }
        ///If `start_checkpoint` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn start_checkpoint_opt_mut(&mut self) -> Option<&mut u64> {
            self.start_checkpoint.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `start_checkpoint`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn start_checkpoint_mut(&mut self) -> &mut u64 {
            self.start_checkpoint.get_or_insert_default()
        }
        ///If `start_checkpoint` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn start_checkpoint_opt(&self) -> Option<u64> {
            self.start_checkpoint.as_ref().map(|field| *field)
        }
        ///Sets `start_checkpoint` with the provided value.
        pub fn set_start_checkpoint(&mut self, field: u64) {
            self.start_checkpoint = Some(field);
        }
        ///Sets `start_checkpoint` with the provided value.
        pub fn with_start_checkpoint(mut self, field: u64) -> Self {
            self.set_start_checkpoint(field);
            self
        }
        ///If `end_checkpoint` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn end_checkpoint_opt_mut(&mut self) -> Option<&mut u64> {
            self.end_checkpoint.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `end_checkpoint`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn end_checkpoint_mut(&mut self) -> &mut u64 {
            self.end_checkpoint.get_or_insert_default()
        }
        ///If `end_checkpoint` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn end_checkpoint_opt(&self) -> Option<u64> {
            self.end_checkpoint.as_ref().map(|field| *field)
        }
        ///Sets `end_checkpoint` with the provided value.
        pub fn set_end_checkpoint(&mut self, field: u64) {
            self.end_checkpoint = Some(field);
        }
        ///Sets `end_checkpoint` with the provided value.
        pub fn with_end_checkpoint(mut self, field: u64) -> Self {
            self.set_end_checkpoint(field);
            self
        }
        ///If `system_state_bcs` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn system_state_bcs_opt(&self) -> Option<&[u8]> {
            self.system_state_bcs.as_ref().map(|field| field as _)
        }
        ///Sets `system_state_bcs` with the provided value.
        pub fn set_system_state_bcs<T: Into<::prost::bytes::Bytes>>(
            &mut self,
            field: T,
        ) {
            self.system_state_bcs = Some(field.into().into());
        }
        ///Sets `system_state_bcs` with the provided value.
        pub fn with_system_state_bcs<T: Into<::prost::bytes::Bytes>>(
            mut self,
            field: T,
        ) -> Self {
            self.set_system_state_bcs(field.into());
            self
        }
        ///If `tx_hi` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn tx_hi_opt_mut(&mut self) -> Option<&mut u64> {
            self.tx_hi.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `tx_hi`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn tx_hi_mut(&mut self) -> &mut u64 {
            self.tx_hi.get_or_insert_default()
        }
        ///If `tx_hi` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn tx_hi_opt(&self) -> Option<u64> {
            self.tx_hi.as_ref().map(|field| *field)
        }
        ///Sets `tx_hi` with the provided value.
        pub fn set_tx_hi(&mut self, field: u64) {
            self.tx_hi = Some(field);
        }
        ///Sets `tx_hi` with the provided value.
        pub fn with_tx_hi(mut self, field: u64) -> Self {
            self.set_tx_hi(field);
            self
        }
        ///If `safe_mode` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn safe_mode_opt_mut(&mut self) -> Option<&mut bool> {
            self.safe_mode.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `safe_mode`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn safe_mode_mut(&mut self) -> &mut bool {
            self.safe_mode.get_or_insert_default()
        }
        ///If `safe_mode` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn safe_mode_opt(&self) -> Option<bool> {
            self.safe_mode.as_ref().map(|field| *field)
        }
        ///Sets `safe_mode` with the provided value.
        pub fn set_safe_mode(&mut self, field: bool) {
            self.safe_mode = Some(field);
        }
        ///Sets `safe_mode` with the provided value.
        pub fn with_safe_mode(mut self, field: bool) -> Self {
            self.set_safe_mode(field);
            self
        }
        ///If `total_stake` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn total_stake_opt_mut(&mut self) -> Option<&mut u64> {
            self.total_stake.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `total_stake`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn total_stake_mut(&mut self) -> &mut u64 {
            self.total_stake.get_or_insert_default()
        }
        ///If `total_stake` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn total_stake_opt(&self) -> Option<u64> {
            self.total_stake.as_ref().map(|field| *field)
        }
        ///Sets `total_stake` with the provided value.
        pub fn set_total_stake(&mut self, field: u64) {
            self.total_stake = Some(field);
        }
        ///Sets `total_stake` with the provided value.
        pub fn with_total_stake(mut self, field: u64) -> Self {
            self.set_total_stake(field);
            self
        }
        ///If `storage_fund_balance` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn storage_fund_balance_opt_mut(&mut self) -> Option<&mut u64> {
            self.storage_fund_balance.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `storage_fund_balance`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn storage_fund_balance_mut(&mut self) -> &mut u64 {
            self.storage_fund_balance.get_or_insert_default()
        }
        ///If `storage_fund_balance` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn storage_fund_balance_opt(&self) -> Option<u64> {
            self.storage_fund_balance.as_ref().map(|field| *field)
        }
        ///Sets `storage_fund_balance` with the provided value.
        pub fn set_storage_fund_balance(&mut self, field: u64) {
            self.storage_fund_balance = Some(field);
        }
        ///Sets `storage_fund_balance` with the provided value.
        pub fn with_storage_fund_balance(mut self, field: u64) -> Self {
            self.set_storage_fund_balance(field);
            self
        }
        ///If `storage_fund_reinvestment` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn storage_fund_reinvestment_opt_mut(&mut self) -> Option<&mut u64> {
            self.storage_fund_reinvestment.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `storage_fund_reinvestment`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn storage_fund_reinvestment_mut(&mut self) -> &mut u64 {
            self.storage_fund_reinvestment.get_or_insert_default()
        }
        ///If `storage_fund_reinvestment` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn storage_fund_reinvestment_opt(&self) -> Option<u64> {
            self.storage_fund_reinvestment.as_ref().map(|field| *field)
        }
        ///Sets `storage_fund_reinvestment` with the provided value.
        pub fn set_storage_fund_reinvestment(&mut self, field: u64) {
            self.storage_fund_reinvestment = Some(field);
        }
        ///Sets `storage_fund_reinvestment` with the provided value.
        pub fn with_storage_fund_reinvestment(mut self, field: u64) -> Self {
            self.set_storage_fund_reinvestment(field);
            self
        }
        ///If `storage_charge` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn storage_charge_opt_mut(&mut self) -> Option<&mut u64> {
            self.storage_charge.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `storage_charge`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn storage_charge_mut(&mut self) -> &mut u64 {
            self.storage_charge.get_or_insert_default()
        }
        ///If `storage_charge` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn storage_charge_opt(&self) -> Option<u64> {
            self.storage_charge.as_ref().map(|field| *field)
        }
        ///Sets `storage_charge` with the provided value.
        pub fn set_storage_charge(&mut self, field: u64) {
            self.storage_charge = Some(field);
        }
        ///Sets `storage_charge` with the provided value.
        pub fn with_storage_charge(mut self, field: u64) -> Self {
            self.set_storage_charge(field);
            self
        }
        ///If `storage_rebate` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn storage_rebate_opt_mut(&mut self) -> Option<&mut u64> {
            self.storage_rebate.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `storage_rebate`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn storage_rebate_mut(&mut self) -> &mut u64 {
            self.storage_rebate.get_or_insert_default()
        }
        ///If `storage_rebate` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn storage_rebate_opt(&self) -> Option<u64> {
            self.storage_rebate.as_ref().map(|field| *field)
        }
        ///Sets `storage_rebate` with the provided value.
        pub fn set_storage_rebate(&mut self, field: u64) {
            self.storage_rebate = Some(field);
        }
        ///Sets `storage_rebate` with the provided value.
        pub fn with_storage_rebate(mut self, field: u64) -> Self {
            self.set_storage_rebate(field);
            self
        }
        ///If `stake_subsidy_amount` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn stake_subsidy_amount_opt_mut(&mut self) -> Option<&mut u64> {
            self.stake_subsidy_amount.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `stake_subsidy_amount`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn stake_subsidy_amount_mut(&mut self) -> &mut u64 {
            self.stake_subsidy_amount.get_or_insert_default()
        }
        ///If `stake_subsidy_amount` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn stake_subsidy_amount_opt(&self) -> Option<u64> {
            self.stake_subsidy_amount.as_ref().map(|field| *field)
        }
        ///Sets `stake_subsidy_amount` with the provided value.
        pub fn set_stake_subsidy_amount(&mut self, field: u64) {
            self.stake_subsidy_amount = Some(field);
        }
        ///Sets `stake_subsidy_amount` with the provided value.
        pub fn with_stake_subsidy_amount(mut self, field: u64) -> Self {
            self.set_stake_subsidy_amount(field);
            self
        }
        ///If `total_gas_fees` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn total_gas_fees_opt_mut(&mut self) -> Option<&mut u64> {
            self.total_gas_fees.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `total_gas_fees`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn total_gas_fees_mut(&mut self) -> &mut u64 {
            self.total_gas_fees.get_or_insert_default()
        }
        ///If `total_gas_fees` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn total_gas_fees_opt(&self) -> Option<u64> {
            self.total_gas_fees.as_ref().map(|field| *field)
        }
        ///Sets `total_gas_fees` with the provided value.
        pub fn set_total_gas_fees(&mut self, field: u64) {
            self.total_gas_fees = Some(field);
        }
        ///Sets `total_gas_fees` with the provided value.
        pub fn with_total_gas_fees(mut self, field: u64) -> Self {
            self.set_total_gas_fees(field);
            self
        }
        ///If `total_stake_rewards_distributed` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn total_stake_rewards_distributed_opt_mut(&mut self) -> Option<&mut u64> {
            self.total_stake_rewards_distributed.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `total_stake_rewards_distributed`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn total_stake_rewards_distributed_mut(&mut self) -> &mut u64 {
            self.total_stake_rewards_distributed.get_or_insert_default()
        }
        ///If `total_stake_rewards_distributed` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn total_stake_rewards_distributed_opt(&self) -> Option<u64> {
            self.total_stake_rewards_distributed.as_ref().map(|field| *field)
        }
        ///Sets `total_stake_rewards_distributed` with the provided value.
        pub fn set_total_stake_rewards_distributed(&mut self, field: u64) {
            self.total_stake_rewards_distributed = Some(field);
        }
        ///Sets `total_stake_rewards_distributed` with the provided value.
        pub fn with_total_stake_rewards_distributed(mut self, field: u64) -> Self {
            self.set_total_stake_rewards_distributed(field);
            self
        }
        ///If `leftover_storage_fund_inflow` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn leftover_storage_fund_inflow_opt_mut(&mut self) -> Option<&mut u64> {
            self.leftover_storage_fund_inflow.as_mut().map(|field| field as _)
        }
        ///Returns a mutable reference to `leftover_storage_fund_inflow`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn leftover_storage_fund_inflow_mut(&mut self) -> &mut u64 {
            self.leftover_storage_fund_inflow.get_or_insert_default()
        }
        ///If `leftover_storage_fund_inflow` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn leftover_storage_fund_inflow_opt(&self) -> Option<u64> {
            self.leftover_storage_fund_inflow.as_ref().map(|field| *field)
        }
        ///Sets `leftover_storage_fund_inflow` with the provided value.
        pub fn set_leftover_storage_fund_inflow(&mut self, field: u64) {
            self.leftover_storage_fund_inflow = Some(field);
        }
        ///Sets `leftover_storage_fund_inflow` with the provided value.
        pub fn with_leftover_storage_fund_inflow(mut self, field: u64) -> Self {
            self.set_leftover_storage_fund_inflow(field);
            self
        }
        ///If `epoch_commitments` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn epoch_commitments_opt(&self) -> Option<&[u8]> {
            self.epoch_commitments.as_ref().map(|field| field as _)
        }
        ///Sets `epoch_commitments` with the provided value.
        pub fn set_epoch_commitments<T: Into<::prost::bytes::Bytes>>(
            &mut self,
            field: T,
        ) {
            self.epoch_commitments = Some(field.into().into());
        }
        ///Sets `epoch_commitments` with the provided value.
        pub fn with_epoch_commitments<T: Into<::prost::bytes::Bytes>>(
            mut self,
            field: T,
        ) -> Self {
            self.set_epoch_commitments(field.into());
            self
        }
    }
    impl super::StoredEvents {
        pub const fn const_default() -> Self {
            Self {
                bcs: ::prost::bytes::Bytes::new(),
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::StoredEvents = super::StoredEvents::const_default();
            &DEFAULT
        }
        ///Sets `bcs` with the provided value.
        pub fn set_bcs<T: Into<::prost::bytes::Bytes>>(&mut self, field: T) {
            self.bcs = field.into().into();
        }
        ///Sets `bcs` with the provided value.
        pub fn with_bcs<T: Into<::prost::bytes::Bytes>>(mut self, field: T) -> Self {
            self.set_bcs(field.into());
            self
        }
    }
    impl super::StoredObject {
        pub const fn const_default() -> Self {
            Self { kind: None }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::StoredObject = super::StoredObject::const_default();
            &DEFAULT
        }
        ///Returns the value of `bcs`, or the default value if `bcs` is unset.
        pub fn bcs(&self) -> &[u8] {
            if let Some(super::stored_object::Kind::Bcs(field)) = &self.kind {
                field as _
            } else {
                &[]
            }
        }
        ///If `bcs` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn bcs_opt(&self) -> Option<&[u8]> {
            if let Some(super::stored_object::Kind::Bcs(field)) = &self.kind {
                Some(field as _)
            } else {
                None
            }
        }
        ///If `bcs` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn bcs_opt_mut(&mut self) -> Option<&mut ::prost::bytes::Bytes> {
            if let Some(super::stored_object::Kind::Bcs(field)) = &mut self.kind {
                Some(field as _)
            } else {
                None
            }
        }
        ///Returns a mutable reference to `bcs`.
        ///If the field is unset, it is first initialized with the default value.
        ///If any other oneof field in the same oneof is set, it will be cleared.
        pub fn bcs_mut(&mut self) -> &mut ::prost::bytes::Bytes {
            if self.bcs_opt_mut().is_none() {
                self.kind = Some(
                    super::stored_object::Kind::Bcs(::prost::bytes::Bytes::default()),
                );
            }
            self.bcs_opt_mut().unwrap()
        }
        ///Sets `bcs` with the provided value.
        ///If any other oneof field in the same oneof is set, it will be cleared.
        pub fn set_bcs<T: Into<::prost::bytes::Bytes>>(&mut self, field: T) {
            self.kind = Some(super::stored_object::Kind::Bcs(field.into().into()));
        }
        ///Sets `bcs` with the provided value.
        ///If any other oneof field in the same oneof is set, it will be cleared.
        pub fn with_bcs<T: Into<::prost::bytes::Bytes>>(mut self, field: T) -> Self {
            self.set_bcs(field.into());
            self
        }
        ///Returns the value of `tombstone`, or the default value if `tombstone` is unset.
        pub fn tombstone(&self) -> &super::StoredObjectTombstone {
            if let Some(super::stored_object::Kind::Tombstone(field)) = &self.kind {
                field as _
            } else {
                super::StoredObjectTombstone::default_instance() as _
            }
        }
        ///If `tombstone` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn tombstone_opt(&self) -> Option<&super::StoredObjectTombstone> {
            if let Some(super::stored_object::Kind::Tombstone(field)) = &self.kind {
                Some(field as _)
            } else {
                None
            }
        }
        ///If `tombstone` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn tombstone_opt_mut(
            &mut self,
        ) -> Option<&mut super::StoredObjectTombstone> {
            if let Some(super::stored_object::Kind::Tombstone(field)) = &mut self.kind {
                Some(field as _)
            } else {
                None
            }
        }
        ///Returns a mutable reference to `tombstone`.
        ///If the field is unset, it is first initialized with the default value.
        ///If any other oneof field in the same oneof is set, it will be cleared.
        pub fn tombstone_mut(&mut self) -> &mut super::StoredObjectTombstone {
            if self.tombstone_opt_mut().is_none() {
                self.kind = Some(
                    super::stored_object::Kind::Tombstone(
                        super::StoredObjectTombstone::default(),
                    ),
                );
            }
            self.tombstone_opt_mut().unwrap()
        }
        ///Sets `tombstone` with the provided value.
        ///If any other oneof field in the same oneof is set, it will be cleared.
        pub fn set_tombstone<T: Into<super::StoredObjectTombstone>>(
            &mut self,
            field: T,
        ) {
            self.kind = Some(super::stored_object::Kind::Tombstone(field.into().into()));
        }
        ///Sets `tombstone` with the provided value.
        ///If any other oneof field in the same oneof is set, it will be cleared.
        pub fn with_tombstone<T: Into<super::StoredObjectTombstone>>(
            mut self,
            field: T,
        ) -> Self {
            self.set_tombstone(field.into());
            self
        }
    }
    impl super::StoredObjectTombstone {
        pub const fn const_default() -> Self {
            Self { kind: 0 }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::StoredObjectTombstone = super::StoredObjectTombstone::const_default();
            &DEFAULT
        }
    }
    impl super::StoredTransaction {
        pub const fn const_default() -> Self {
            Self {
                transaction_bcs: ::prost::bytes::Bytes::new(),
                signatures_bcs: ::prost::bytes::Bytes::new(),
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::StoredTransaction = super::StoredTransaction::const_default();
            &DEFAULT
        }
        ///Sets `transaction_bcs` with the provided value.
        pub fn set_transaction_bcs<T: Into<::prost::bytes::Bytes>>(&mut self, field: T) {
            self.transaction_bcs = field.into().into();
        }
        ///Sets `transaction_bcs` with the provided value.
        pub fn with_transaction_bcs<T: Into<::prost::bytes::Bytes>>(
            mut self,
            field: T,
        ) -> Self {
            self.set_transaction_bcs(field.into());
            self
        }
        ///Sets `signatures_bcs` with the provided value.
        pub fn set_signatures_bcs<T: Into<::prost::bytes::Bytes>>(&mut self, field: T) {
            self.signatures_bcs = field.into().into();
        }
        ///Sets `signatures_bcs` with the provided value.
        pub fn with_signatures_bcs<T: Into<::prost::bytes::Bytes>>(
            mut self,
            field: T,
        ) -> Self {
            self.set_signatures_bcs(field.into());
            self
        }
    }
    impl super::TxMetadata {
        pub const fn const_default() -> Self {
            Self {
                digest: ::prost::bytes::Bytes::new(),
                checkpoint_seq: 0,
                ckpt_position: 0,
                event_count: 0,
                timestamp_ms: 0,
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::TxMetadata = super::TxMetadata::const_default();
            &DEFAULT
        }
        ///Sets `digest` with the provided value.
        pub fn set_digest<T: Into<::prost::bytes::Bytes>>(&mut self, field: T) {
            self.digest = field.into().into();
        }
        ///Sets `digest` with the provided value.
        pub fn with_digest<T: Into<::prost::bytes::Bytes>>(mut self, field: T) -> Self {
            self.set_digest(field.into());
            self
        }
        ///Returns a mutable reference to `checkpoint_seq`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn checkpoint_seq_mut(&mut self) -> &mut u64 {
            &mut self.checkpoint_seq
        }
        ///Sets `checkpoint_seq` with the provided value.
        pub fn set_checkpoint_seq(&mut self, field: u64) {
            self.checkpoint_seq = field;
        }
        ///Sets `checkpoint_seq` with the provided value.
        pub fn with_checkpoint_seq(mut self, field: u64) -> Self {
            self.set_checkpoint_seq(field);
            self
        }
        ///Returns a mutable reference to `ckpt_position`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn ckpt_position_mut(&mut self) -> &mut u32 {
            &mut self.ckpt_position
        }
        ///Sets `ckpt_position` with the provided value.
        pub fn set_ckpt_position(&mut self, field: u32) {
            self.ckpt_position = field;
        }
        ///Sets `ckpt_position` with the provided value.
        pub fn with_ckpt_position(mut self, field: u32) -> Self {
            self.set_ckpt_position(field);
            self
        }
        ///Returns a mutable reference to `event_count`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn event_count_mut(&mut self) -> &mut u32 {
            &mut self.event_count
        }
        ///Sets `event_count` with the provided value.
        pub fn set_event_count(&mut self, field: u32) {
            self.event_count = field;
        }
        ///Sets `event_count` with the provided value.
        pub fn with_event_count(mut self, field: u32) -> Self {
            self.set_event_count(field);
            self
        }
        ///Returns a mutable reference to `timestamp_ms`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn timestamp_ms_mut(&mut self) -> &mut u64 {
            &mut self.timestamp_ms
        }
        ///Sets `timestamp_ms` with the provided value.
        pub fn set_timestamp_ms(&mut self, field: u64) {
            self.timestamp_ms = field;
        }
        ///Sets `timestamp_ms` with the provided value.
        pub fn with_timestamp_ms(mut self, field: u64) -> Self {
            self.set_timestamp_ms(field);
            self
        }
    }
}
