# Initial architecture constraints from primary sources

Checked: 2026-10-23. Scope: documentation review only, before proposing M01 phases.
No application or installed-provider experiment was executed.

| Primary source | Observed constraint | Consequence for the proposed work |
|---|---|---|
| [Linux kernel: /proc filesystem](https://www.kernel.org/doc/html/latest/filesystems/proc.html) | `smaps_rollup` aggregates mapping statistics; PSS apportions shared pages | Report process-tree RSS and PSS separately with explicit GUI/host/agent categories |
| [IETF RFC 8252](https://www.rfc-editor.org/rfc/rfc8252.html) | Native authorization uses a separate browser context; public clients use PKCE | Provider-supported OAuth requires browser/PKCE handling; this does not confer a provider integration permission |
| [freedesktop Secret Service specification](https://specifications.freedesktop.org/secret-service/latest/) | Linux defines a session credential-service interface | Verify the actual supported keyring and fail closed when unavailable rather than assume a local file is acceptable |

These sources inform the proposed phase division: resource/lifecycle measurement
first, persistent host next, then a real provider and account isolation. They do
not select a renderer, prove keyring availability or establish subscription-token
reuse rights. The product specification names additional provider references;
implementation must recheck official capabilities and exact installed versions.

This is an engineering inference from the sources and requirements. No outside
code was imported, and there are no third-party runtime dependencies yet. A
future candidate comparison records exact source revisions, license terms,
redistribution limits, supported display/input paths and measured cost before a
selection. Unknown or experimental provider operations remain visibly unverified.
