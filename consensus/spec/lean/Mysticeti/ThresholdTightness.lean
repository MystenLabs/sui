/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.Thresholds

namespace Mysticeti

/-! Tightness of the weighted v3 threshold construction.

These results concern the three arithmetic guarantees used by the protocol:
quorum availability, quorum-certificate exclusion, and preservation of one
certificate across two quorums. They do not claim that every unsafe parameter
choice occurs in one reachable protocol execution.
-/

/-- Quorum availability and the two safety inequalities require the hybrid
committee lower bound. -/
theorem safe_live_thresholds_require_hybrid_total
    {total fault unavailable quorum certificate : Nat}
    (quorumAvailable : quorum + fault + unavailable <= total)
    (quorumCertificateSafe : total + fault < quorum + certificate)
    (quorumPreservesCertificate :
      total + fault + certificate <= quorum + quorum) :
    5 * fault + 3 * unavailable + 1 <= total := by
  omega

/-- At the minimum hybrid committee size, the three threshold guarantees force
the nominal v3 quorum and certificate thresholds. -/
theorem tight_hybrid_total_forces_nominal_thresholds
    {total fault unavailable quorum certificate : Nat}
    (totalTight : total = 5 * fault + 3 * unavailable + 1)
    (quorumAvailable : quorum + fault + unavailable <= total)
    (quorumCertificateSafe : total + fault < quorum + certificate)
    (quorumPreservesCertificate :
      total + fault + certificate <= quorum + quorum) :
    quorum = 4 * fault + 2 * unavailable + 1 /\
      certificate = 2 * fault + unavailable + 1 := by
  omega

namespace Thresholds

/-- The checked `Thresholds` fields and one unavailable-stake budget imply the
hybrid committee lower bound. -/
theorem hybrid_total_lower_bound
    {authorityCount : Nat} {stake : Nat -> Nat}
    (thresholds : Thresholds authorityCount stake)
    (unavailable : Nat)
    (quorumAvailable :
      thresholds.quorum + thresholds.fault + unavailable <=
        totalWeight authorityCount stake) :
    5 * thresholds.fault + 3 * unavailable + 1 <=
      totalWeight authorityCount stake := by
  exact safe_live_thresholds_require_hybrid_total quorumAvailable
    thresholds.quorum_certificate_intersection
    thresholds.quorum_preserves_certificate

/-- At the minimum hybrid committee size, a checked `Thresholds` value must use
the nominal v3 thresholds. -/
theorem tight_hybrid_total_has_nominal_thresholds
    {authorityCount : Nat} {stake : Nat -> Nat}
    (thresholds : Thresholds authorityCount stake)
    (unavailable : Nat)
    (totalTight :
      totalWeight authorityCount stake =
        5 * thresholds.fault + 3 * unavailable + 1)
    (quorumAvailable :
      thresholds.quorum + thresholds.fault + unavailable <=
        totalWeight authorityCount stake) :
    thresholds.quorum = 4 * thresholds.fault + 2 * unavailable + 1 /\
      thresholds.certificate = 2 * thresholds.fault + unavailable + 1 := by
  exact tight_hybrid_total_forces_nominal_thresholds totalTight quorumAvailable
    thresholds.quorum_certificate_intersection
    thresholds.quorum_preserves_certificate

end Thresholds

end Mysticeti
