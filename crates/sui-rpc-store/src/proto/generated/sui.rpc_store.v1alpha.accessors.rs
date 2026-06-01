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
                object_lo: 0,
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
        ///Returns a mutable reference to `object_lo`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn object_lo_mut(&mut self) -> &mut u64 {
            &mut self.object_lo
        }
        ///Sets `object_lo` with the provided value.
        pub fn set_object_lo(&mut self, field: u64) {
            self.object_lo = field;
        }
        ///Sets `object_lo` with the provided value.
        pub fn with_object_lo(mut self, field: u64) -> Self {
            self.set_object_lo(field);
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
    impl super::StoredCommittee {
        pub const fn const_default() -> Self {
            Self {
                bcs: ::prost::bytes::Bytes::new(),
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::StoredCommittee = super::StoredCommittee::const_default();
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
                protocol_version: 0,
                reference_gas_price: 0,
                start_timestamp_ms: 0,
                end_timestamp_ms: 0,
                end_checkpoint: 0,
                system_state_bcs: ::prost::bytes::Bytes::new(),
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::StoredEpoch = super::StoredEpoch::const_default();
            &DEFAULT
        }
        ///Returns a mutable reference to `protocol_version`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn protocol_version_mut(&mut self) -> &mut u64 {
            &mut self.protocol_version
        }
        ///Sets `protocol_version` with the provided value.
        pub fn set_protocol_version(&mut self, field: u64) {
            self.protocol_version = field;
        }
        ///Sets `protocol_version` with the provided value.
        pub fn with_protocol_version(mut self, field: u64) -> Self {
            self.set_protocol_version(field);
            self
        }
        ///Returns a mutable reference to `reference_gas_price`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn reference_gas_price_mut(&mut self) -> &mut u64 {
            &mut self.reference_gas_price
        }
        ///Sets `reference_gas_price` with the provided value.
        pub fn set_reference_gas_price(&mut self, field: u64) {
            self.reference_gas_price = field;
        }
        ///Sets `reference_gas_price` with the provided value.
        pub fn with_reference_gas_price(mut self, field: u64) -> Self {
            self.set_reference_gas_price(field);
            self
        }
        ///Returns a mutable reference to `start_timestamp_ms`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn start_timestamp_ms_mut(&mut self) -> &mut u64 {
            &mut self.start_timestamp_ms
        }
        ///Sets `start_timestamp_ms` with the provided value.
        pub fn set_start_timestamp_ms(&mut self, field: u64) {
            self.start_timestamp_ms = field;
        }
        ///Sets `start_timestamp_ms` with the provided value.
        pub fn with_start_timestamp_ms(mut self, field: u64) -> Self {
            self.set_start_timestamp_ms(field);
            self
        }
        ///Returns a mutable reference to `end_timestamp_ms`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn end_timestamp_ms_mut(&mut self) -> &mut u64 {
            &mut self.end_timestamp_ms
        }
        ///Sets `end_timestamp_ms` with the provided value.
        pub fn set_end_timestamp_ms(&mut self, field: u64) {
            self.end_timestamp_ms = field;
        }
        ///Sets `end_timestamp_ms` with the provided value.
        pub fn with_end_timestamp_ms(mut self, field: u64) -> Self {
            self.set_end_timestamp_ms(field);
            self
        }
        ///Returns a mutable reference to `end_checkpoint`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn end_checkpoint_mut(&mut self) -> &mut u64 {
            &mut self.end_checkpoint
        }
        ///Sets `end_checkpoint` with the provided value.
        pub fn set_end_checkpoint(&mut self, field: u64) {
            self.end_checkpoint = field;
        }
        ///Sets `end_checkpoint` with the provided value.
        pub fn with_end_checkpoint(mut self, field: u64) -> Self {
            self.set_end_checkpoint(field);
            self
        }
        ///Sets `system_state_bcs` with the provided value.
        pub fn set_system_state_bcs<T: Into<::prost::bytes::Bytes>>(
            &mut self,
            field: T,
        ) {
            self.system_state_bcs = field.into().into();
        }
        ///Sets `system_state_bcs` with the provided value.
        pub fn with_system_state_bcs<T: Into<::prost::bytes::Bytes>>(
            mut self,
            field: T,
        ) -> Self {
            self.set_system_state_bcs(field.into());
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
            Self {
                bcs: ::prost::bytes::Bytes::new(),
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::StoredObject = super::StoredObject::const_default();
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
    impl super::StoredTransaction {
        pub const fn const_default() -> Self {
            Self {
                bcs: ::prost::bytes::Bytes::new(),
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::StoredTransaction = super::StoredTransaction::const_default();
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
