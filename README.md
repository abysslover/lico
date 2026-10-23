# Lico

a lightweight workbench for agentic workflows

Lico is a proposed desktop workbench for persistent command-line agents. The
session host owns execution and output independently of the interface, so
closing or hiding a view should let work continue. Hidden views are intended to
release presentation resources and return from a snapshot plus later events.

Linux comes first; Windows follows through explicit platform interfaces. The
renderer, implementation language and integration strategy remain undecided.

## Current status

This repository contains the initial scaffold, [product requirements](doc/product/202610/20261023/00001_product_spec/product_spec.md),
a [proposed first milestone](doc/milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md) and a
[draft architecture experiment plan](doc/plans/202610/20261023/20261023_01_M01-P01_architecture_measurement_plan.md) for review. No application, provider adapter, benchmark result or release is
available yet. Resource budgets are targets, and no target is claimed as met.

There is no application build or run command at this stage. The future build
must work from a public clone with documented, publicly available dependencies.
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
