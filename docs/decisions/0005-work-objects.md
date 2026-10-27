# Work objects and financial evidence

Status: accepted for the build package. Company deployment still needs the owner's and IT's approvals.

## Decision

A module owns its named calculations and record types. It uses the shared record, evidence, scope and transaction contracts in [Financial work](../reference/financial-work.md), rather than adding a second store or writer. The foundation is defined before the records port is built. Module screens and calculations can follow later.

An entity is the reporting boundary. A project belongs to one entity. A work object (bid, estimate, close or analysis) belongs to an entity and may name a project. Period identity is an entity's calendar id and period id, with exact start and end dates; it is not inferred from a display month. IDs and versions, not titles, identify each object.

Approvals name exact immutable targets, method versions and evidence hashes. Locks cover an explicit scope and period. Reopening records a new authorized act and reason; it does not erase the close or its approval. Issuing work creates a content-addressed snapshot of the exact versions, runs, methods and approvals used. A later source or estimate never changes an issued snapshot. A corrective issue supersedes it.

Progress and revenue have no universal default formula. A module must name a versioned method and its required inputs and approvals. Missing prerequisites, a missing method or an invalid denominator produce Unknown with a reason. A model may explain a result but never supplies a missing calculation or approval.

## Evidence before modules

The synthetic pricing, three-period estimate and close cases in the reference are foundation contract tests. They do not implement a pricing or close application. The records card supplies the storage, scoped lock seam, figure evidence and durable run lookup once. Each later module adds types and calculations by the existing recipes, with no change to those contracts.

## Consequences

Record history, period observations and calculation evidence consume storage. Search must retain that history through partitioned indexes. Copies and outputs still use effective labels and the same egress gate. Neither a snapshot nor an approval lowers a label. Recovery tests restore the exact issued snapshot before delivery is authorized.
