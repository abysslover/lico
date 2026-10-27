# Contributing to Lico

Lico is built phase by phase on reviewed branches. The first phases are built
and reported, and none is accepted until the owner says so (see
[PLAN.md](PLAN.md)). Please discuss behavior and architecture changes before
implementation. No phase is approved by the existence of this repository or a
planning pull request.

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
Work is done on a branch per phase; how a finished phase reaches `main` is set
out in the next section. Merges are performed locally with the intended
pseudonymous author and committer, then pushed without rewriting published
history. Do not create merge commits in the hosting service UI.

## How finished phases reach `main`

The owner decided this rule on 2026-10-27. It applies to every phase after the
catch-up of `main` on that day.

1. `main` follows the finished phases one at a time. When a phase is finished and
   verified (its report and its independent review, with the fix rounds), the
   owner is asked for G4 for that phase alone. At most one finished phase waits
   for G4 while the next is built.
2. The move is a fast-forward of `main` to the tip of the phase branch while the
   phases form one chain, so no merge commit is made. The owner's G4 names the
   exact tip. If the phases ever stop forming one chain, a merge commit would be
   needed and that case is put to the owner on its own.
3. G4 does not assume G3. The request for G4 says in what state the G3 of the
   phase is (given, due at the next G3 point named in its milestone document, or
   still pending). A phase is moved with G3 pending only on the owner's G4 given
   with that statement in front of them, and the status documents then call it
   "inspection pending".
4. Phase branches are kept after the move; they are not deleted.
5. At the closing commit of a milestone, the commit on `main` that sets the
   milestone's status to done, a branch `milestone/Mxx-<name>` is created at that
   commit and listed in the milestone document. Each milestone is then a named
   point that can be opened while `main` stays current.
6. The status documents are updated in the same change as the phase: PLAN.md, the
   "Current status" paragraph of README.md, the phase table and the gate rows of
   the milestone document, and the state cells of the roadmap. A phase is not
   finished while they show an older state.

Moving `main` is not publication. A push needs the owner's separate, explicit yes
for the exact list of what is published.

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

The program is one executable, `lico`, written in Rust; its build is described in
`src/host/README.md` and its tests and their commands in `tests/README.md`.
Documentation checks are not evidence of application behavior. Record failures,
unsupported capabilities and unverified outcomes. Retain third-party license
notices for any code or dependencies introduced.
