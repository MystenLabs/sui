// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use sui_types::{gas_allowance::StorageRebate, id::UID, object::MoveObject};

pub(super) struct StorageRebateUpdate {
    parent: ObjectID,
    child_id: ObjectID,
    unprotected: u64,
    old_child: Option<Object>,
}

pub(super) struct StorageRebatePlan {
    pub(super) updates: Vec<StorageRebateUpdate>,
    /// Protected deposits retained unchanged during failed-execution recovery.
    pub(super) retained_rebates: BTreeMap<ObjectID, u64>,
}

impl TemporaryStore<'_> {
    /// Prepay metadata lookups for mutable inputs, including the input-only OOG recovery path.
    pub(crate) fn charge_storage_rebate_input_reads(
        &self,
        gas_charger: &mut GasCharger,
    ) -> Result<(), ExecutionError> {
        if self.protocol_config.enable_storage_rebate_provenance() {
            let bytes = StorageRebate::input_read_bytes(
                self.mutable_input_refs.len(),
                self.protocol_config,
            );
            gas_charger.charge_storage_read(bytes)?;
        }
        Ok(())
    }

    /// Load attribution once per settlement attempt. Marker writes are deferred until the
    /// ordinary objects' new deposits are known, so recycled deposits are not re-attributed.
    pub(super) fn prepare_storage_rebates(
        &mut self,
        gas_charger: &mut GasCharger,
        include_writes: bool,
        preserve_protected_inputs: bool,
    ) -> Result<StorageRebatePlan, ExecutionError> {
        debug_assert!(self.storage_rebate_recipients.is_empty());
        if !self.protocol_config.enable_storage_rebate_provenance() {
            return Ok(StorageRebatePlan {
                updates: vec![],
                retained_rebates: BTreeMap::new(),
            });
        }

        let mut plan = StorageRebatePlan {
            updates: Vec::new(),
            retained_rebates: BTreeMap::new(),
        };
        let mut ids = self.execution_results.modified_objects.clone();
        if include_writes {
            ids.extend(self.execution_results.written_objects.keys());
        }
        let tag = StorageRebate::type_();
        for parent in ids {
            let written = self.execution_results.written_objects.get(&parent);
            if !include_writes && written.is_some() {
                continue;
            }
            let may_attribute = gas_charger.allowance_funder().is_some()
                && written.is_some_and(|object| !object.is_immutable());
            let old_metadata = self.get_object_modified_at(&parent);
            if old_metadata.is_none() && !may_attribute {
                continue;
            }
            let child_id = StorageRebate::child_id(parent);
            let old_deposit = old_metadata.as_ref().map_or(0, |m| m.storage_rebate);
            let old_child = match old_metadata {
                Some(metadata) => {
                    // Declared mutable inputs were prepaid before execution. Dynamically loaded
                    // children are charged here, before the lookup, and discarded on OOG rollback.
                    if !self.mutable_input_refs.contains_key(&parent) {
                        gas_charger.charge_storage_read(StorageRebate::input_read_bytes(
                            1,
                            self.protocol_config,
                        ))?;
                    }
                    self.store
                        .read_child_object(&parent, &child_id, metadata.version)
                        .map_err(|e| ExecutionError::invariant_violation(e.to_string()))?
                }
                None => None,
            };
            let mut unprotected = old_deposit;
            if let Some(child) = &old_child {
                let data = child.data.try_as_move().ok_or_else(|| {
                    ExecutionError::invariant_violation("Storage rebate child is not a Move object")
                })?;
                if !data.type_().is(&tag) {
                    return Err(ExecutionError::invariant_violation(
                        "Invalid storage rebate child type",
                    ));
                }
                let rebate: StorageRebate = bcs::from_bytes(data.contents())
                    .map_err(|e| ExecutionError::invariant_violation(e.to_string()))?;
                unprotected = old_deposit.checked_sub(rebate.amount).ok_or_else(|| {
                    ExecutionError::invariant_violation("Attribution exceeds the storage deposit")
                })?;
                // A matching funder can recycle protected storage through the paying allowance.
                // Other payments return the old protected deposit to its original funder.
                if !gas_charger.recycles_allowance_storage(rebate.recipient) {
                    if preserve_protected_inputs
                        && written.is_some()
                        && self.mutable_input_refs.contains_key(&parent)
                    {
                        plan.retained_rebates.insert(parent, old_deposit);
                        continue;
                    }
                    self.storage_rebate_recipients
                        .insert(parent, (rebate.recipient, rebate.amount));
                    self.storage_rebate_recipients
                        .insert(child_id, (rebate.recipient, child.storage_rebate));
                }
            }
            if may_attribute || old_child.is_some() {
                plan.updates.push(StorageRebateUpdate {
                    parent,
                    child_id,
                    unprotected,
                    old_child,
                });
            }
        }
        Ok(plan)
    }

    /// Attribute only newly funded storage. A pre-existing, unprotected deposit continues to
    /// recycle for subsequent payers instead of becoming a windfall for the allowance funder.
    pub(super) fn finish_storage_rebates(
        &mut self,
        gas_charger: &mut GasCharger,
        updates: Vec<StorageRebateUpdate>,
    ) -> Result<(), ExecutionError> {
        let funder = gas_charger.allowance_funder();
        let mut generated = BTreeSet::new();
        for update in updates {
            let amount = self
                .execution_results
                .written_objects
                .get(&update.parent)
                .filter(|object| !object.is_immutable())
                .map_or(0, |object| {
                    object.storage_rebate.saturating_sub(update.unprotected)
                });
            let Some(recipient) = funder.filter(|_| amount > 0) else {
                if let Some(child) = update.old_child {
                    self.delete_child_object(child);
                }
                continue;
            };
            let rebate = StorageRebate {
                id: UID::new(update.child_id),
                recipient,
                amount,
            };
            let mut child = if let Some(old_child) = &update.old_child {
                let mut child = old_child.clone();
                let contents = bcs::to_bytes(&rebate)
                    .map_err(|e| ExecutionError::invariant_violation(e.to_string()))?;
                child
                    .data
                    .try_as_move_mut()
                    .unwrap()
                    .set_contents_unsafe(contents);
                child.previous_transaction = self.tx_digest;
                child
            } else {
                self.new_storage_rebate_object(&rebate, Owner::ObjectOwner(update.parent.into()))?
            };
            let old_deposit = update
                .old_child
                .as_ref()
                .map_or(0, |child| child.storage_rebate);
            let usable_rebate = if let Some((recipient, amount)) =
                self.storage_rebate_recipients.get(&update.child_id)
            {
                self.route_storage_refund(gas_charger, update.child_id, *recipient, *amount)?;
                0
            } else {
                old_deposit
            };
            child.storage_rebate = gas_charger
                .track_storage_mutation(
                    update.child_id,
                    child.object_size_for_gas_metering(),
                    usable_rebate,
                )
                .ok_or_else(|| ExecutionError::from_kind(ExecutionErrorKind::InvariantViolation))?;
            if let Some(old_child) = update.old_child {
                self.mutate_child_object(old_child, child);
            } else {
                generated.insert(update.child_id);
                self.create_object(child);
            }
        }
        self.save_generated_object_ids(generated);
        Ok(())
    }

    fn new_storage_rebate_object(
        &self,
        rebate: &StorageRebate,
        owner: Owner,
    ) -> Result<Object, ExecutionError> {
        let contents = bcs::to_bytes(rebate)
            .map_err(|e| ExecutionError::invariant_violation(e.to_string()))?;
        // This framework key-only type has no Move constructor or public receive/transfer path.
        let data = unsafe {
            MoveObject::new_from_execution(
                StorageRebate::type_().into(),
                false,
                SequenceNumber::MIN,
                contents,
                self.protocol_config,
                false,
            )?
        };
        Ok(Object::new_move(data, owner, self.tx_digest))
    }

    pub(super) fn route_storage_refund(
        &mut self,
        gas_charger: &mut GasCharger,
        source: ObjectID,
        recipient: SuiAddress,
        amount: u64,
    ) -> Result<(), ExecutionError> {
        if amount == 0 {
            return Ok(());
        }
        if self.carried_rebate_inputs.contains_key(&source) {
            return Err(ExecutionError::invariant_violation(
                "Storage refund source was routed more than once",
            ));
        }
        self.carried_rebate_inputs.insert(source, amount);
        gas_charger.route_storage_refund(recipient, amount)
    }

    pub(crate) fn defer_storage_refund(
        &mut self,
        recipient: SuiAddress,
        amount: u64,
    ) -> Result<(), ExecutionError> {
        if amount == 0 {
            return Ok(());
        }
        let id = StorageRebate::claim_id(recipient, &self.tx_digest);
        let mut claim = self.new_storage_rebate_object(
            &StorageRebate {
                id: UID::new(id),
                recipient,
                amount: 0,
            },
            Owner::AddressOwner(recipient),
        )?;
        // Keep diverted raw deposits in a funder-owned claim. Deleting the claim later applies
        // the rebate rate and records the storage-fund withdrawal.
        claim.storage_rebate = amount;
        self.create_object(claim);
        self.save_generated_object_ids(BTreeSet::from([id]));
        if self.carried_rebate_outputs.insert(id, amount).is_some() {
            return Err(ExecutionError::invariant_violation(
                "Storage refund claim was created more than once",
            ));
        }
        Ok(())
    }

    pub(crate) fn add_storage_refund(&mut self, recipient: SuiAddress, amount: u64) {
        if amount == 0 {
            return;
        }
        let total = self.storage_refunds.entry(recipient).or_default();
        *total = total
            .checked_add(amount)
            .expect("bounded transaction storage refunds");
        let event = AccumulatorEvent::from_balance_change(
            recipient,
            sui_types::balance::Balance::type_tag(sui_types::gas_coin::GAS::type_tag()),
            i64::try_from(amount).expect("bounded transaction storage refund"),
        )
        .expect("SUI storage refund accumulator event");
        self.add_accumulator_event(event);
    }
}
