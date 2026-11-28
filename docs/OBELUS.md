## Errors and obelus

Errors shown to users end with an **obelus**: a marker unique to the line
of code that raised the error, so a user's screenshot or bug report leads
straight to it. Users see a sentence followed by the obelus in brackets,
`... for solutions. (dapper mackerel)`; developers grep for the obelus.

In sv2 an obelus is written as `Obelus("dapper mackerel")`, a type in
`sv2/crates/common/src/lib.rs`, at the place the error is raised. sv2 has no
numeric error codes.

An obelus is:

- **Two words**, lowercase ASCII, separated by one space
- **Distinct and unusual**: unlikely to turn up on Google until we create it,
  so searching for it finds only us. Avoid common phrases ("gone fishing",
  "empty mailbox") and anything already in the codebase.
- **Unobjectionable**: nothing rude, crude, or unkind; users will read it
  while already annoyed
- **Funny**, ideally, to take the edge off an error

Examples: `snarly foo`, `woebegone elephant`, `lofty undertow`.

Write a new obelus for every new call site; never reuse one, and do not
repeat one in a test. A unit test in `sv2/crates/common/src/lib.rs`
(`every_obelus_is_unique_and_two_lowercase_words`, run by
`scripts/tests-sv2`) scans the Rust sources and fails on a duplicate, a
malformed obelus, or one that is not a literal.

"Obelus" is both singular and plural (one obelus, many obelus), so a single
grep for `obelus` finds every mention.

Not every message needs one. Usage errors ("unknown option") and messages
that already name the fix ("an admin must run: sv2 install") explain
themselves; an obelus is for the error whose origin a reader could not
otherwise find.
