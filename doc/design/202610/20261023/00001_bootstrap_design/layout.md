# Repository layout and navigation

Updated: 2026-10-23. This is the public product layout contract, not a runtime setup.

The project has exactly five non-hidden root directories: `doc`, `exp`, `private`,
`src` and `tests`. `private` exists locally and is never tracked, so a public
checkout creates it only when local evidence is needed. Root files may hold the
README, license, contributor guide, status and a future stack manifest. Generated
build/dependency directories and Git metadata are separate from authored modules.

| Root | Content |
|---|---|
| `doc` | Public product, milestone, plan, report and design documentation |
| `exp` | Sanitized experiment records; raw runs in ignored `scratch/` |
| `private` | Untracked local evidence, operational instructions and scratch |
| `src` | All implementation, assets, command-line tools and reusable utilities |
| `tests` | Synthetic unit, integration, acceptance and fixture material |

The five maximum documentation categories are `product`, `milestones`, `plans`,
`reports` and `design`. Design contains architecture, source research, decisions
and reusable procedures if they become necessary; these do not create extra doc
root categories. An authored source or document node has at most five immediate
directory groups. A sixth module is grouped semantically before creation.

Milestone/phase/report archives retain `YYYYMM/YYYYMMDD/` and phase-ID filenames.
Product/design artifacts may add a numbered descriptive artifact directory.
Archive dates and runs are storage keys, not authored modules: navigation indexes
show at most five visible links/items per page, using continuation pages as they
grow. This explicitly keeps date archives compatible with bounded navigation;
it does not hide a sixth module in a dot-directory or rewrite existing links.
No other repository is reorganized by this contract.

Local assistant instructions, prompts, account state and machine-specific
bindings are ignored from first staging. Public contributor documentation is
separately written and usable without local operational files. Lockfiles and
normal public dotfiles remain trackable; there is no blanket dotfile or lockfile
ignore. Future dependency notices live with their corresponding source material.
