# M01-P04 S01 — credential store: interface, client options and measured cost

Status: recorded during S01 development. The figures are from one development run on a dirty tree,
not the phase measurement (S05 repeats them from a clean commit). Everything ran against a
throwaway private Secret Service (a private session bus and `gnome-keyring-daemon` 46.1 in a
private home, canary values); no real keyring, account or provider was involved.

## Interface

`lico-cred --data-dir DIR` with `status`, `add`, `list`, `remove` and `helper`:

- a secret is read from standard input only; a `--secret` argument is refused before anything runs,
  and a masked label that contains the secret is refused;
- `helper --ref R` writes the secret to standard output for the program that asked for that
  reference (the mode a provider's key helper setting would call); it refuses a terminal;
- keyring entries carry the attributes `application=lico`, `provider`, `account`, `kind` and an
  opaque `ref`; the metadata file (`credentials.json`, mode 0600, replaced atomically) holds the
  same fields, a masked label and a creation time, never a secret;
- when the session bus, the Secret Service or the default keyring is missing, or the keyring is
  locked, every command exits 3 with the action the person can take, nothing is stored anywhere
  else, and no metadata is written. A locked keyring is reported, never unlocked by Lico.

## Client options

| Option | State | Notes |
|---|---|---|
| Hand-written D-Bus client in Rust (about 650 lines, standard library plus the `serde_json` already used by the host) | **built and measured** | no new dependency |
| A Rust crate with a D-Bus library | not built, not measured | would add several pinned dependencies, each announced and approved before download; size and memory unknown |
| libsecret through C | not built | its development files are not installed on this host |

Measured with the hand-written client (indicative): the binary is 407 KB (the host binary is
660 KB); one `helper` process peaks at 1.9 MiB resident (10 runs, GNU time); a read takes 1.6 ms
median over 100 runs including process start; an `add` takes about 60 ms median over 20 runs (the
keyring does the work); failing closed takes 1.4 ms with a locked keyring and 0.8 ms with no bus.

Limits of the hand-written client, all unverified beyond what is stated: it was tested against
gnome-keyring 46.1 only (KeePassXC and other services are unverified); it uses the `plain`
transfer algorithm, so a secret crosses the owner-only session bus unencrypted (the encrypted
algorithm is not implemented; a process with the same user's access can already do what this client
does); it speaks little-endian messages and EXTERNAL authentication only and handles no prompt.

## Where the loopback control token stands

The host writes the loopback listener's token to a 0600 file when it starts and creates it new
each run. AUTH-01 says application-owned secrets live only in the credential store. A headless host
may have no Secret Service at start, so moving the token would make the optional listener depend on
one. Not moved in this phase; the deviation from the letter of AUTH-01 is reported to the owner.
