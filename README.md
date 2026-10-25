# Lico

a lightweight workbench for agentic workflows

Lico is a proposed desktop workbench for persistent command-line agents. The
session host owns execution and output independently of the interface, so
closing or hiding a view should let work continue. Hidden views are intended to
release presentation resources and return from a snapshot plus later events.

Linux comes first; Windows follows through explicit platform interfaces. The
renderer, implementation language and integration strategy are selected from
measurements recorded in the first milestone's reports; the owner's inspection
of those reports is pending.

## Current status

This repository holds the [product requirements](doc/product/202610/20261023/00001_product_spec/product_spec.md), the milestones,
and the phase plans and reports. Implementation is written on reviewed phase branches and reaches `main` only after the owner's
inspection (G3) and merge approval (G4); until then the application, provider adapters and measurements described in the
documents are not part of `main`, and no release exists. Resource budgets are targets unless a report marks a number as measured.

The first milestone is the [measured Linux foundation](doc/milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md).
The current work is the second, [M02 Agentic-coding basics](doc/milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md):
sessions that run in a real folder, sending between sessions, Lico tools that an agent can call, screen capture and clipboard, computer use,
and the memory of every session shown in the window, followed by a memory pass against the limit of an idle window under 50 MiB.
Its [planning set](doc/product/202610/20261026/00001_agentic_basics_plan/planning.md) gives the scope and the order, and the
[roadmap](doc/product/202610/20261025/00001_roadmap/roadmap.md) shows where each milestone stands.

There is no application build or run command on `main` yet. The build must
work from a public clone with documented, publicly available dependencies.
Provider tools are independently installed unless redistribution is permitted.

## Project map

- [Documentation](doc/README.md)
- [Current status and approval gates](PLAN.md)
- [Contributing](CONTRIBUTING.md)
- [Experiments](exp/README.md)
- [Source](src/README.md) and [tests](tests/README.md)

## Authentication and accounts

The intended design uses official provider authentication only. Application-owned
secrets belong in the operating system credential store, with no plaintext
fallback. Accounts for the same provider must have separate credentials,
refresh state and session bindings. A default-account change must not retarget
running work, and an error must not silently switch accounts or billing modes.
These are requirements, not implemented capabilities.

## License

[MIT](LICENSE). Provider names identify integrations and do not imply endorsement.
