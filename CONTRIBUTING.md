# Contributing to Lico

Lico is at the project-bootstrap stage. Please discuss behavior and architecture
changes before implementation. No phase is approved by the existence of this
repository or a planning pull request.

## Review and approval

Work follows a milestone, a draft phase plan, a branch, meaningful red-to-green
behavior tests, an inspection bundle, a result report, a pull request and an
independent review. Review findings are resolved in new commits.

- G1: the owner approves the milestone and its boundaries.
- G2: the owner approves the concrete phase plan before implementation.
- G3: an inspection verdict records what was actually observed. The owner retains
  final acceptance of the complete product.
- G4: the owner explicitly approves a merge after checks and review pass.

Each approval is separate. There is no inherited automatic-merge permission.
Documentation drafts may share a pull request so milestone and phase approvals
can be reviewed together, while both remain pending until explicitly approved.
The initial scaffold is the sole bootstrap commit on the default branch. Later
work uses branches and reviewed pull requests. Merges are performed locally with
the intended pseudonymous author and committer, then pushed without rewriting
published history. Do not create merge commits in the hosting service UI.

## Public source

Keep local assistant instructions, prompts, credentials, authentication state,
private inspection evidence and machine-specific runtime configuration outside
tracked source. Stage named files deliberately and inspect their staged content.
A first publication or release checks the exact tree, reachable history, archive,
author and committer. An ignore rule does not erase prior commits.

Use synthetic fixtures. Never put tokens in command arguments, URLs, logs or
screenshots. Provider-managed credentials remain provider-managed; accessing a
subscription in an independent client requires an officially supported flow.

## Layout and validation

The only non-hidden project directories are `doc`, `exp`, `private`, `src` and
`tests`; `private` is intentionally untracked. Reusable helpers and assets belong
under `src`. Keep authored directory groups to at most five at each node and use
indexed date archives for growing evidence. See the documentation index for the
current layout contract.

The first approved implementation phase will establish its stack and validation
commands. Until then, documentation checks are not evidence of application
behavior. Record failures, unsupported capabilities and unverified outcomes.
Retain third-party license notices for any code or dependencies introduced.
