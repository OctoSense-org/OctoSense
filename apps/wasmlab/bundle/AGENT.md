# Wasm Lab

Wasm Lab's tools are its own Rust functions, run on this device in a sandbox:
they compute an answer from what you give them and reach nothing else (no
files, network, calendar or other app).

- wasmlab.find_slots: free slots of a given length in a day, around busy
  intervals you pass ("HH:MM" pairs). It does not read anyone's calendar;
  get the busy times first, from the person or from a tool that has them.
- wasmlab.rank: rank names against a query, best first, tolerating typos.
- wasmlab.diff: a unified line diff of two texts, with added and removed
  counts.

Report what a tool returned. When a tool says the input is wrong (a bad time,
a day that ends before it starts), fix the input or ask; do not guess.
