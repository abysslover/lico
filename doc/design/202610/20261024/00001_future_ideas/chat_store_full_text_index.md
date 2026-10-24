# Future idea: a full-text index for the chat store

- Status: idea recorded for a later milestone; not planned, not approved, nothing built or measured
- Date: 2026-10-24
- Source: the owner's idea, from experience with genome-scale sequence indexes. The statements about
  speed and memory below are the owner's report and are **unverified** here.

## The idea

Keep every session's conversation in one searchable store whose full-text index is a Burrows-Wheeler
transform with an FM-index, made run-length compressed (an RLBWT or r-index style structure) so that
the index can also be **updated** as conversations grow:

- each session is terminated by a separator symbol (`$`), so a session can be told apart, replaced
  or removed without rebuilding the whole index;
- short hashed keys (minimizers) can find short strings quickly; this worked well on genomes, where
  the alphabet has few symbols;
- a CUDA kernel can make search fast, but the owner expects it to be hard to make memory-efficient;
- for one person's chat history this combination might be the best fit: compact, updatable and
  searchable per session.

## Questions a plan would have to answer by measurement

- Alphabet: text is Unicode (UTF-8 bytes or code points), not four symbols; the cost of the rank
  structure over such an alphabet (a wavelet tree or matrix) and of run-length compression on chat
  text is unknown. Conversation text repeats less than genomes do.
- Updates: how appends and replacements are handled (dynamic structure, or small segments merged in
  the background), and what a purge of one session costs.
- Memory and latency against the product budgets, on a real history of the owner's size, compared
  with a plain alternative (for example an inverted or trigram index in an embedded database) so
  that the extra complexity is justified by a number.
- Hardware: whether a GPU is required or only optional; the product runs on machines without one.
- Privacy: the index of a conversation is as sensitive as the conversation. It belongs under the
  same owner-only modes, retention limit and purge as the session log, and a canary must not
  survive a purge in any index segment.
- Where it sits: a search service next to the session host, never inside the GUI process whose
  memory budget is fixed.

## Not decided

Nothing here is a requirement. A milestone for chat search would be drafted from the owner's
direction, with its own plan and approval.
