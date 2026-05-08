mod _accessor_impls {
    #![allow(clippy::useless_conversion)]
    impl super::RestoreState {
        pub const fn const_default() -> Self {
            Self { state: None }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::RestoreState = super::RestoreState::const_default();
            &DEFAULT
        }
        ///Returns the value of `in_progress`, or the default value if `in_progress` is unset.
        pub fn in_progress(&self) -> &super::restore_state::InProgress {
            if let Some(super::restore_state::State::InProgress(field)) = &self.state {
                field as _
            } else {
                super::restore_state::InProgress::default_instance() as _
            }
        }
        ///If `in_progress` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn in_progress_opt(&self) -> Option<&super::restore_state::InProgress> {
            if let Some(super::restore_state::State::InProgress(field)) = &self.state {
                Some(field as _)
            } else {
                None
            }
        }
        ///If `in_progress` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn in_progress_opt_mut(
            &mut self,
        ) -> Option<&mut super::restore_state::InProgress> {
            if let Some(super::restore_state::State::InProgress(field)) = &mut self.state
            {
                Some(field as _)
            } else {
                None
            }
        }
        ///Returns a mutable reference to `in_progress`.
        ///If the field is unset, it is first initialized with the default value.
        ///If any other oneof field in the same oneof is set, it will be cleared.
        pub fn in_progress_mut(&mut self) -> &mut super::restore_state::InProgress {
            if self.in_progress_opt_mut().is_none() {
                self.state = Some(
                    super::restore_state::State::InProgress(
                        super::restore_state::InProgress::default(),
                    ),
                );
            }
            self.in_progress_opt_mut().unwrap()
        }
        ///Sets `in_progress` with the provided value.
        ///If any other oneof field in the same oneof is set, it will be cleared.
        pub fn set_in_progress<T: Into<super::restore_state::InProgress>>(
            &mut self,
            field: T,
        ) {
            self.state = Some(
                super::restore_state::State::InProgress(field.into().into()),
            );
        }
        ///Sets `in_progress` with the provided value.
        ///If any other oneof field in the same oneof is set, it will be cleared.
        pub fn with_in_progress<T: Into<super::restore_state::InProgress>>(
            mut self,
            field: T,
        ) -> Self {
            self.set_in_progress(field.into());
            self
        }
        ///Returns the value of `complete`, or the default value if `complete` is unset.
        pub fn complete(&self) -> &super::restore_state::Complete {
            if let Some(super::restore_state::State::Complete(field)) = &self.state {
                field as _
            } else {
                super::restore_state::Complete::default_instance() as _
            }
        }
        ///If `complete` is set, returns [`Some`] with the value; otherwise returns [`None`].
        pub fn complete_opt(&self) -> Option<&super::restore_state::Complete> {
            if let Some(super::restore_state::State::Complete(field)) = &self.state {
                Some(field as _)
            } else {
                None
            }
        }
        ///If `complete` is set, returns [`Some`] with a mutable reference to the value; otherwise returns [`None`].
        pub fn complete_opt_mut(
            &mut self,
        ) -> Option<&mut super::restore_state::Complete> {
            if let Some(super::restore_state::State::Complete(field)) = &mut self.state {
                Some(field as _)
            } else {
                None
            }
        }
        ///Returns a mutable reference to `complete`.
        ///If the field is unset, it is first initialized with the default value.
        ///If any other oneof field in the same oneof is set, it will be cleared.
        pub fn complete_mut(&mut self) -> &mut super::restore_state::Complete {
            if self.complete_opt_mut().is_none() {
                self.state = Some(
                    super::restore_state::State::Complete(
                        super::restore_state::Complete::default(),
                    ),
                );
            }
            self.complete_opt_mut().unwrap()
        }
        ///Sets `complete` with the provided value.
        ///If any other oneof field in the same oneof is set, it will be cleared.
        pub fn set_complete<T: Into<super::restore_state::Complete>>(
            &mut self,
            field: T,
        ) {
            self.state = Some(
                super::restore_state::State::Complete(field.into().into()),
            );
        }
        ///Sets `complete` with the provided value.
        ///If any other oneof field in the same oneof is set, it will be cleared.
        pub fn with_complete<T: Into<super::restore_state::Complete>>(
            mut self,
            field: T,
        ) -> Self {
            self.set_complete(field.into());
            self
        }
    }
    impl super::restore_state::Complete {
        pub const fn const_default() -> Self {
            Self { restored_at: 0 }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::restore_state::Complete = super::restore_state::Complete::const_default();
            &DEFAULT
        }
        ///Returns a mutable reference to `restored_at`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn restored_at_mut(&mut self) -> &mut u64 {
            &mut self.restored_at
        }
        ///Sets `restored_at` with the provided value.
        pub fn set_restored_at(&mut self, field: u64) {
            self.restored_at = field;
        }
        ///Sets `restored_at` with the provided value.
        pub fn with_restored_at(mut self, field: u64) -> Self {
            self.set_restored_at(field);
            self
        }
    }
    impl super::restore_state::InProgress {
        pub const fn const_default() -> Self {
            Self {
                target_checkpoint: 0,
                partitions_complete: Vec::new(),
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::restore_state::InProgress = super::restore_state::InProgress::const_default();
            &DEFAULT
        }
        ///Returns a mutable reference to `target_checkpoint`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn target_checkpoint_mut(&mut self) -> &mut u64 {
            &mut self.target_checkpoint
        }
        ///Sets `target_checkpoint` with the provided value.
        pub fn set_target_checkpoint(&mut self, field: u64) {
            self.target_checkpoint = field;
        }
        ///Sets `target_checkpoint` with the provided value.
        pub fn with_target_checkpoint(mut self, field: u64) -> Self {
            self.set_target_checkpoint(field);
            self
        }
        ///Returns the value of `partitions_complete`, or the default value if `partitions_complete` is unset.
        pub fn partitions_complete(&self) -> &[::prost::bytes::Bytes] {
            &self.partitions_complete
        }
        ///Returns a mutable reference to `partitions_complete`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn partitions_complete_mut(&mut self) -> &mut Vec<::prost::bytes::Bytes> {
            &mut self.partitions_complete
        }
        ///Sets `partitions_complete` with the provided value.
        pub fn set_partitions_complete(&mut self, field: Vec<::prost::bytes::Bytes>) {
            self.partitions_complete = field;
        }
        ///Sets `partitions_complete` with the provided value.
        pub fn with_partitions_complete(
            mut self,
            field: Vec<::prost::bytes::Bytes>,
        ) -> Self {
            self.set_partitions_complete(field);
            self
        }
    }
    impl super::Watermark {
        pub const fn const_default() -> Self {
            Self {
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive: 0,
                tx_hi: 0,
                timestamp_ms_hi_inclusive: 0,
            }
        }
        #[doc(hidden)]
        pub fn default_instance() -> &'static Self {
            static DEFAULT: super::Watermark = super::Watermark::const_default();
            &DEFAULT
        }
        ///Returns a mutable reference to `epoch_hi_inclusive`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn epoch_hi_inclusive_mut(&mut self) -> &mut u64 {
            &mut self.epoch_hi_inclusive
        }
        ///Sets `epoch_hi_inclusive` with the provided value.
        pub fn set_epoch_hi_inclusive(&mut self, field: u64) {
            self.epoch_hi_inclusive = field;
        }
        ///Sets `epoch_hi_inclusive` with the provided value.
        pub fn with_epoch_hi_inclusive(mut self, field: u64) -> Self {
            self.set_epoch_hi_inclusive(field);
            self
        }
        ///Returns a mutable reference to `checkpoint_hi_inclusive`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn checkpoint_hi_inclusive_mut(&mut self) -> &mut u64 {
            &mut self.checkpoint_hi_inclusive
        }
        ///Sets `checkpoint_hi_inclusive` with the provided value.
        pub fn set_checkpoint_hi_inclusive(&mut self, field: u64) {
            self.checkpoint_hi_inclusive = field;
        }
        ///Sets `checkpoint_hi_inclusive` with the provided value.
        pub fn with_checkpoint_hi_inclusive(mut self, field: u64) -> Self {
            self.set_checkpoint_hi_inclusive(field);
            self
        }
        ///Returns a mutable reference to `tx_hi`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn tx_hi_mut(&mut self) -> &mut u64 {
            &mut self.tx_hi
        }
        ///Sets `tx_hi` with the provided value.
        pub fn set_tx_hi(&mut self, field: u64) {
            self.tx_hi = field;
        }
        ///Sets `tx_hi` with the provided value.
        pub fn with_tx_hi(mut self, field: u64) -> Self {
            self.set_tx_hi(field);
            self
        }
        ///Returns a mutable reference to `timestamp_ms_hi_inclusive`.
        ///If the field is unset, it is first initialized with the default value.
        pub fn timestamp_ms_hi_inclusive_mut(&mut self) -> &mut u64 {
            &mut self.timestamp_ms_hi_inclusive
        }
        ///Sets `timestamp_ms_hi_inclusive` with the provided value.
        pub fn set_timestamp_ms_hi_inclusive(&mut self, field: u64) {
            self.timestamp_ms_hi_inclusive = field;
        }
        ///Sets `timestamp_ms_hi_inclusive` with the provided value.
        pub fn with_timestamp_ms_hi_inclusive(mut self, field: u64) -> Self {
            self.set_timestamp_ms_hi_inclusive(field);
            self
        }
    }
}
