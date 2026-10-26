# Experiments

Each record lives under `exp/YYYYMM/YYYYMMDD/NNNNN_title/` and states its question, bounded
synthetic workload, tested versions, measurement scope and observed result. Raw generated
runs stay untracked under `scratch/`. A target or a passing mock is not a measured
resource result or a real provider acceptance.

- `202610/20261026/00001_real_program_canary` - opt-in check that the real `claude` and
  `codex` programs do not load a chosen folder's own configuration (tested: 2.1.310, 0.164.0).
