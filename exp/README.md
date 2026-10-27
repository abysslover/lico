# Experiments

Each record lives under `exp/YYYYMM/YYYYMMDD/NNNNN_title/` and states its question, bounded
synthetic workload, tested versions, measurement scope and observed result. Raw generated
runs stay untracked under `scratch/`. A target or a passing mock is not a measured
resource result or a real provider acceptance.

- `202610/20261026/00001_real_program_canary` - opt-in check that the real `claude` and
  `codex` programs do not load a chosen folder's own configuration (tested: 2.1.310, 0.164.0).
- `202610/20261026/00002_s02_listener_memory` - memory of the tool listener of the host with sessions open (tested: S02 build).
- `202610/20261027/00001_config_file_after_connect` - opt-in check that the real `claude` does not read its
  `--mcp-config` file again after the first `initialize`, and where its token string is found (tested: 2.1.310).
- `202610/20261027/00002_s04_real_program_check` - opt-in check with the real `claude` that the tool connection answers `send` and `create` with `pending` and the decision comes later as an
  input, what the hand-over of a real conversation stores, and what the program does when a session's conversation is gone (tested: 2.1.310, 2.1.311).
- `202610/20261027/00003_record_layout_2_1_311` - opt-in check of the layout of the program's own conversation record across two versions, and the scrubber that makes the fixtures
  of the conversation reader (tested: 2.1.310, 2.1.311).
- `202610/20261027/00004_s07c_real_program_history_check` - opt-in check with the real `claude` that a model calls `history`, ends its turn on `pending`, names what the notice brings, follows the notice of a long answer on to
  `read_handoff` (one part of 32,445 bytes, stored whole by the program), and what it does with a planted line that tells a reader to call `send` (tested: 2.1.310, haiku, 7 turns).
- `202610/20261027/00005_s08_ten_minutes_of_traffic` - ten minutes of traffic on all five tools of the tool connection (simulator, 3 sessions, 150 cycles) with the host alone and with the window
  showing the list: growth of the host and the window, threads, descriptors, a search for credentials and sent texts in the run folder, and the peak of the host's `status` operation.
