# M16-P06 Plan — The Driver Port, the Adapter, CLI and HTTP Parity, System Tests (Sketch)

- Status: sketch; its plan and the SPEC PROPOSAL for the protocol kind `local` are shown to the owner at their own stop (G2 of P06 and the owner's yes to the spec text) before anything of P06 starts
- Version: 1.0.0
- Branch: `phase/M16-P06-driver-port-and-adapter` (stacked on the P05 tip)
- Milestone: [M16](../../../milestones/202611/20261117/20261117_01_M16_retire_prototypes_and_local_llm_session_milestone.md)

## Objective (sketch)

The small driver port that keeps the new kind out of the 48-module cycle (new edges counted against the graph: none upward into the hubs); the thin host adapter that wires `llm-http`, `llm-wire`, `convo` and `toolloop` to `session`, `tools::run`, `pending` and the keyring; `session.create` with `kind: "local"`, CLI and HTTP parity; the proposed text for `protocol_v1.md` and the LLM-01..03 requirements (a proposal first, then a `doc(M16-P06)` commit, then code); the four named system tests on the short list (512 MiB and 60 s) with their own measured peak and time; idle RSS after the adapter; the affected host tests; the 5,120-byte tool list.

## Steps

- [ ] To be written with the plan that the owner sees first.
