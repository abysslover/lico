# Architecture candidates (M01-P01)

Two minimal clients for the synthetic host in `src/lico_host`. Both take
commands on stdin (`show ID [CURSOR]`, `hide`, `status`, `hoststatus`, `quit`)
and print one JSON object per line. They exist to be measured, not shipped.

| Directory | Candidate | Build |
|---|---|---|
| `a_native` | Rust with winit 0.30.13, softbuffer 0.4.8, fontdue 0.9.5 (versions pinned in `Cargo.lock`) | `cargo build --release --locked` |
| `b_gtk` | C with GTK 3 (`gtk+-3.0`, LGPL-2.1-or-later) | `gcc -O2 main.c $(pkg-config --cflags --libs gtk+-3.0)` |
| `c_qt` | C++ with Qt 5 Widgets (LGPL-3.0 or GPL, as installed) | `g++ -O2 -fPIC main.cpp $(pkg-config --cflags --libs Qt5Widgets)` |
| `d_webkit` | C with GTK 3 and WebKitGTK 4.1 (`webkit2gtk-4.1`, LGPL-2.1-or-later) | `gcc -O2 main.c $(pkg-config --cflags --libs gtk+-3.0 webkit2gtk-4.1)` |

Variants measured from the same binaries: `b2` (GTK with GDK GL disabled and a
GLX vendor override), and `b3`, `c3`, `d3` (experiment only: an empty `libGL.so.1`
first on the library path so the GL driver stack is never mapped; not a
supported deployment).

Candidate A resolves 81 crates for the Linux target. Every one declares a
permissive license (MIT, Apache-2.0, Zlib, ISC, BSD-2-Clause, Unlicense or an
OR/AND combination of these). Generated notices are produced before any release.
