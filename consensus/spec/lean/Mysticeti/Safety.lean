/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.CommitChain
import Mysticeti.Leader

namespace Mysticeti

/-! One aggregate safety theorem for the modeled Mysticeti v3 decision rules.

Applying this theorem to Rust requires the assumption ledger. Key obligations are
`ASM-SAFE-EVIDENCE-REFINEMENT`, `ASM-SAFE-DIGEST-IDENTITY`,
`ASM-SAFE-COMMIT-STORE`, `ASM-SAFE-INSTALL-PROVENANCE`,
`ASM-SAFE-FIRST-TRIGGER`, `ASM-SAFE-COMMITTED-PREFIX`, and `ASM-SAFE-GC`.
-/

/-- Valid modeled evidence cannot produce conflicting leader or transaction
decisions. Correct views with the same commit stream also choose one indirect
result at the first eligible trigger. -/
theorem mysticeti_v3_safety
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    (leader : LeaderEvidence authorityCount stake thresholds)
    (transaction : TransactionEvidence authorityCount stake thresholds)
    (stream : CommitStream) (hasCertificate : Nat → Bool)
    {start leftLength rightLength leftTrigger rightTrigger : Nat}
    (startLeader :
      (stream start).leaderRound = transaction.gcWindow.firstCommitLeaderRound)
    (leftVisible :
      VisibleFirst stream transaction.gcWindow.firstCommitLeaderRound
        start leftLength leftTrigger)
    (rightVisible :
      VisibleFirst stream transaction.gcWindow.firstCommitLeaderRound
        start rightLength rightTrigger) :
    (¬(leader.CanDecide .commit ∧ leader.CanDecide .skip)) ∧
    (¬(transaction.CanDecide .accept ∧ transaction.CanDecide .reject)) ∧
    indirectAt hasCertificate leftTrigger =
      indirectAt hasCertificate rightTrigger := by
  constructor
  · intro conflicting
    exact leader.safety conflicting.1 conflicting.2
  · constructor
    · intro conflicting
      have predecessor := firstEligible_predecessor stream startLeader leftVisible.2
      have ready : transaction.IndirectEvidenceReady
          (stream (leftTrigger - 1)).leaderRound := predecessor.2
      exact transaction.safety ready conflicting.1 conflicting.2
    · exact first_trigger_agreement stream hasCertificate leftVisible rightVisible

end Mysticeti
