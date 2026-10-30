# sui-core

## Design docs

- Before modifying `src/consensus_adapter.rs`, `src/admission_queue.rs`, or the
  transaction submission path in `src/authority_server.rs`, read
  `src/consensus_submission_pipeline.md` for the end-to-end dataflow and behaviors.
  Update that doc in the same change when submission behavior changes.
- When touching `src/accumulators/` or
  `src/execution_scheduler/funds_withdraw_scheduler/`, start with
  `src/accumulators/design_docs/README.md`.
