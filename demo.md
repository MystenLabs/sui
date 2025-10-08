========================================
AUTHENTICATED EVENTS E2E TEST
========================================
Published event package: 0x5ad131d11425985a460951163d1b9fc620d59f9b723bc16806ccc2b248d56656

Step 1: Emitting first event in epoch 0...
First event emitted successfully

Step 2: Waiting for epoch change to demonstrate trust ratcheting...
Epoch changed to epoch 1

Step 3: Emitting remaining 9 events in epoch 1...
All 9 events emitted successfully in epoch 1

Step 4: Querying authenticated events via ListAuthenticatedEvents API...
Received 10 authenticated events

Step 5: Getting EventStreamHead inclusion proofs and verifying with committee...
First event checkpoint: 20, Last event checkpoint: 102

Step 5a: Requesting inclusion proof for first EventStreamHead...
  EventStreamHead object ID: 0x65e4d040377c230f64bf953c10fc8e9d8ee5729994b8b856995c53e981ce4faa
  Checkpoint: 20
Received inclusion proof for first EventStreamHead

Step 5b: Verifying first EventStreamHead with trust-ratcheted committee...
  EventStreamHead - checkpoint: 20, num_events: 1, mmr_len: 1
========================================
INCLUSION PROOF VERIFICATION
========================================
Verifying EventStreamHead inclusion proof:
  Checkpoint: 20
  Object ID: 0x65e4d040377c230f64bf953c10fc8e9d8ee5729994b8b856995c53e981ce4faa
  Version: 0x1c

========================================
LIGHT CLIENT TRUST RATCHETING DEMO
========================================
Target checkpoint: 20
Target epoch: 0

Step 1: Fetching genesis committee (epoch 0) via get_epoch API...
  Genesis committee loaded:
    Epoch: 0
    Validators: 4
    Total stake: 10000

Target epoch is genesis epoch, no trust ratcheting needed
Verifying inclusion proof with trust-ratcheted committee...
Inclusion proof verification PASSED
EventStreamHead authenticity cryptographically verified!
========================================


Step 5c: Requesting inclusion proof for last EventStreamHead...
  EventStreamHead object ID: 0x65e4d040377c230f64bf953c10fc8e9d8ee5729994b8b856995c53e981ce4faa
  Checkpoint: 102
Received inclusion proof for last EventStreamHead

Step 5d: Verifying last EventStreamHead with trust-ratcheted committee...
  EventStreamHead - checkpoint: 102, num_events: 10, mmr_len: 4
========================================
INCLUSION PROOF VERIFICATION
========================================
Verifying EventStreamHead inclusion proof:
  Checkpoint: 102
  Object ID: 0x65e4d040377c230f64bf953c10fc8e9d8ee5729994b8b856995c53e981ce4faa
  Version: 0x71

========================================
LIGHT CLIENT TRUST RATCHETING DEMO
========================================
Target checkpoint: 102
Target epoch: 9

Step 1: Fetching genesis committee (epoch 0) via get_epoch API...
  Genesis committee loaded:
    Epoch: 0
    Validators: 4
    Total stake: 10000

Step 2: Trust ratcheting through epochs 0 to 8...

  Epoch 0 -> 1 transition:
    Fetching end-of-epoch checkpoint via get_epoch API...
    End-of-epoch checkpoint: 30
    Fetching checkpoint summary and signature via get_checkpoint API...
    Verifying checkpoint signatures with epoch 0 committee...
    Signature verification PASSED
    Extracting epoch 1 committee from verified checkpoint...
    New committee extracted:
      Epoch: 1
      Validators: 4
      Total stake: 10000
    Trust ratchet COMPLETE for epoch 0 -> 1

  Epoch 1 -> 2 transition:
    Fetching end-of-epoch checkpoint via get_epoch API...
    End-of-epoch checkpoint: 39
    Fetching checkpoint summary and signature via get_checkpoint API...
    Verifying checkpoint signatures with epoch 1 committee...
    Signature verification PASSED
    Extracting epoch 2 committee from verified checkpoint...
    New committee extracted:
      Epoch: 2
      Validators: 4
      Total stake: 10000
    Trust ratchet COMPLETE for epoch 1 -> 2

  Epoch 2 -> 3 transition:
    Fetching end-of-epoch checkpoint via get_epoch API...
    End-of-epoch checkpoint: 48
    Fetching checkpoint summary and signature via get_checkpoint API...
    Verifying checkpoint signatures with epoch 2 committee...
    Signature verification PASSED
    Extracting epoch 3 committee from verified checkpoint...
    New committee extracted:
      Epoch: 3
      Validators: 4
      Total stake: 10000
    Trust ratchet COMPLETE for epoch 2 -> 3

  Epoch 3 -> 4 transition:
    Fetching end-of-epoch checkpoint via get_epoch API...
    End-of-epoch checkpoint: 56
    Fetching checkpoint summary and signature via get_checkpoint API...
    Verifying checkpoint signatures with epoch 3 committee...
    Signature verification PASSED
    Extracting epoch 4 committee from verified checkpoint...
    New committee extracted:
      Epoch: 4
      Validators: 4
      Total stake: 10000
    Trust ratchet COMPLETE for epoch 3 -> 4

  Epoch 4 -> 5 transition:
    Fetching end-of-epoch checkpoint via get_epoch API...
    End-of-epoch checkpoint: 64
    Fetching checkpoint summary and signature via get_checkpoint API...
    Verifying checkpoint signatures with epoch 4 committee...
    Signature verification PASSED
    Extracting epoch 5 committee from verified checkpoint...
    New committee extracted:
      Epoch: 5
      Validators: 4
      Total stake: 10000
    Trust ratchet COMPLETE for epoch 4 -> 5

  Epoch 5 -> 6 transition:
    Fetching end-of-epoch checkpoint via get_epoch API...
    End-of-epoch checkpoint: 73
    Fetching checkpoint summary and signature via get_checkpoint API...
    Verifying checkpoint signatures with epoch 5 committee...
    Signature verification PASSED
    Extracting epoch 6 committee from verified checkpoint...
    New committee extracted:
      Epoch: 6
      Validators: 4
      Total stake: 10000
    Trust ratchet COMPLETE for epoch 5 -> 6

  Epoch 6 -> 7 transition:
    Fetching end-of-epoch checkpoint via get_epoch API...
    End-of-epoch checkpoint: 82
    Fetching checkpoint summary and signature via get_checkpoint API...
    Verifying checkpoint signatures with epoch 6 committee...
    Signature verification PASSED
    Extracting epoch 7 committee from verified checkpoint...
    New committee extracted:
      Epoch: 7
      Validators: 4
      Total stake: 10000
    Trust ratchet COMPLETE for epoch 6 -> 7

  Epoch 7 -> 8 transition:
    Fetching end-of-epoch checkpoint via get_epoch API...
    End-of-epoch checkpoint: 91
    Fetching checkpoint summary and signature via get_checkpoint API...
    Verifying checkpoint signatures with epoch 7 committee...
    Signature verification PASSED
    Extracting epoch 8 committee from verified checkpoint...
    New committee extracted:
      Epoch: 8
      Validators: 4
      Total stake: 10000
    Trust ratchet COMPLETE for epoch 7 -> 8

  Epoch 8 -> 9 transition:
    Fetching end-of-epoch checkpoint via get_epoch API...
    End-of-epoch checkpoint: 99
    Fetching checkpoint summary and signature via get_checkpoint API...
    Verifying checkpoint signatures with epoch 8 committee...
    Signature verification PASSED
    Extracting epoch 9 committee from verified checkpoint...
    New committee extracted:
      Epoch: 9
      Validators: 4
      Total stake: 10000
    Trust ratchet COMPLETE for epoch 8 -> 9

Trust ratcheting complete! Final committee:
  Epoch: 9
  Validators: 4
========================================

Verifying inclusion proof with trust-ratcheted committee...
Inclusion proof verification PASSED
EventStreamHead authenticity cryptographically verified!
========================================


Step 6: Validating MMR computation from first to last checkpoint...
  Starting from first EventStreamHead state:
    Checkpoint: 20
    Events: 1
    MMR length: 1

  Converting 10 events to commitments...
  Grouping events by checkpoint...
    Checkpoint 33: 1 events
    Checkpoint 42: 1 events
    Checkpoint 51: 1 events
    Checkpoint 59: 1 events
    Checkpoint 68: 1 events
    Checkpoint 77: 1 events
    Checkpoint 86: 1 events
    Checkpoint 93: 1 events
    Checkpoint 102: 1 events

  Applying 9 checkpoint updates to MMR...

  Calculated EventStreamHead:
    Checkpoint: 20
    Events: 10
    MMR length: 4

  Actual EventStreamHead from chain:
    Checkpoint: 102
    Events: 10
    MMR length: 4

  Comparing calculated vs actual...
    ✓ Event count matches: 10
    ✓ MMR matches!

MMR validation successful!

========================================
ALL TESTS PASSED!
========================================