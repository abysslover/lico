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
the phase plans and reports, and the source of the finished phases. Implementation is written on reviewed phase branches and reaches
`main` only after the owner's inspection (G3) and merge approval (G4). The owner made two exceptions, each a fast-forward of `main` on
the merge approval (G4) alone: on 2026-10-25 to the first stack (M01-P01 to M01-P05) and on 2026-10-27 to the finished work up to
M02-P02. The inspection acceptance (G3) of those phases is still pending, and later phases stay on phase branches. No release exists.
Resource budgets are targets unless a report marks a number as measured.

The first milestone is the [measured Linux foundation](doc/milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md).
The current work is the second, [M02 Agentic-coding basics](doc/milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md):
sessions that run in a real folder, sending between sessions, Lico tools that an agent can call, screen capture and clipboard, computer use,
and the memory of every session shown in the window, followed by a memory pass against the limit of an idle window under 50 MiB.
Its [planning set](doc/product/202610/20261026/00001_agentic_basics_plan/planning.md) gives the scope and the order, and the
[roadmap](doc/product/202610/20261025/00001_roadmap/roadmap.md) shows where each milestone stands. Reports exist for M01-P01 to M01-P07, M01-P07a,
M02-P01 and M02-P02, and each waits for the owner's inspection (G3); M02-P03 is in progress and M01-P08 is not started.

The program is one executable, `lico`; its build is described in [src/host/README.md](src/host/README.md). No release exists, and the
acceptance pack that shows a clean public clone rebuilding it (M01-P08) is not started. The build must work from a public clone with
documented, publicly available dependencies.
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
These are requirements. The phases that implement them (M01-P04 and M01-P06) have reports that wait for the owner's inspection, so they
are not accepted capabilities.

## License

[MIT](LICENSE). Provider names identify integrations and do not imply endorsement.
