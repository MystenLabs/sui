/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.CommitChain
import Mysticeti.Leader

namespace Mysticeti

/-! One aggregate safety theorem for the modeled Mysticeti v3 decision rules. -/

theorem mysticeti_v3_safety
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    (leader : LeaderEvidence authorityCount stake thresholds)
    (transaction : TransactionEvidence authorityCount stake thresholds)
    (stream : CommitStream) (hasCertificate : Nat → Bool)
    {targetRound start leftLength rightLength leftTrigger rightTrigger : Nat}
    (leftVisible :
      VisibleFirst stream targetRound start leftLength leftTrigger)
    (rightVisible :
      VisibleFirst stream targetRound start rightLength rightTrigger) :
    (¬(leader.CanDecide .commit ∧ leader.CanDecide .skip)) ∧
    (¬(transaction.CanDecide .accept ∧ transaction.CanDecide .reject)) ∧
    indirectAt hasCertificate leftTrigger =
      indirectAt hasCertificate rightTrigger := by
  constructor
  · intro conflicting
    exact leader.safety conflicting.1 conflicting.2
  · constructor
    · intro conflicting
      exact transaction.safety conflicting.1 conflicting.2
    · exact first_trigger_agreement stream hasCertificate leftVisible rightVisible

end Mysticeti
