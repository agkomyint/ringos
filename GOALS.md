# Ring (r) — Goals

Ring is a compiled language with a compiler written first in Rust, then rewritten in Ring itself (self-hosting).

## Technical goals
1. **Smallest binary** — no libc, no runtime. The compiler writes the executable itself.
2. **Fastest compile time** — direct machine-code generation, no external C compiler or linker, no heavy optimizer.
3. **Fastest runtime** — static types, native code, no garbage collector, no hidden costs.
4. **Secure** — bounds-checked arrays, no null, no undefined behavior, no uninitialized memory.
5. **Least syntax** — fewest lines and tokens for the same program (also cheapest for LLMs to read and write). Every feature must earn its place on `cargo run --bin bench`.
6. **Self-hosting** — Ring's compiler is written in Ring, and a stage-1 / stage-2 build must produce identical output.

## Goal: the most beloved language of all time
Being fast is not enough. People love languages that feel good to use. Ring commits to:

7. **Memorable by design** — body-part keywords where the metaphor is real (`mouth` prints, `ear` listens, `heart` loops, `finger` points). If you can guess it, you can remember it.
8. **Joy in the first 5 minutes** — install is one tiny file, "hello world" runs instantly, and there is no setup.
9. **Errors that help** — every compiler error says what is wrong, where, and how to fix it, in plain language. Never cryptic.
10. **One obvious way** — few features, no style wars. Formatting is built in and there is one canonical style.
11. **Instant feedback** — compile times so short that you never wait. A change shows its result in under a second.
12. **Batteries included, tiny** — one tool does build, run, test, and format. No package-manager maze for basic tasks.
13. **Honest and stable** — no breaking changes without a very long notice. Code you write today still compiles in ten years.
14. **Friendly to humans and LLMs** — small grammar, predictable rules, and few surprises, so both people and AI write correct Ring on the first try.
15. **Welcoming community** — kind by default, with beginner questions treated as valuable, clear docs, and a small language that is possible to fully understand.
16. **Fun** — a sense of humor is allowed. The language that makes people smile gets remembered.

## Ambitious goals (measurable, public)
17. **A million lines per second** — single-pass, parallel-by-default compilation. "I never wait for the compiler" is literally true.
18. **Hello world under 512 bytes, aiming for 150 (the "F-150")** — a public binary-size leaderboard. Size is a bragging right, not an afterthought.
19. **Bootstrap from a tiny seed** — a few-KB hand-auditable binary can rebuild the entire compiler, with no LLVM, no linker, and no C. The whole toolchain is readable and trustworthy.
20. **Learnable in an afternoon** — the whole language, specification included, fits in one short document.

21. **Least memory possible** — Ring programs and the Ring compiler both use as little RAM as possible.
    - Programs: no garbage collector, no hidden allocations, no runtime heap at startup. Stack and arenas first, and tightly packed data (no padding waste, small integer types when a value fits).
    - Compiler: streaming, single-pass, and arena-allocated, so it can compile on a tiny machine with a few MB of RAM.
    - Measured: a public "peak RAM" number for hello world, for the compiler, and in the benchmarks, next to binary size and speed.

22. **Hello world in 12 characters of source or fewer** — e.g. `mouth"Hi"` (9). Tracked in the syntax benchmark.

23. **Safe, cheap parallelism by default** — tasks share nothing and talk by messages, so data races are impossible by construction. Tasks cost a few KB each, so millions can run. Uses all cores with no configuration.
24. **Scale by adding machines (2 to 10,000+)** — write one program, add machines, and it uses all their RAM and CPU. Big collections (`big`) are partitioned across the pool and the work moves to the data. Remote cost is always visible in the types, never hidden. Optional library: hello world never pays for it.

### Ring's answer to the weak spots of Ray-style systems
| Common problem in distributed frameworks | Ring's answer |
|---|---|
| Heavy per-node runtime and RAM overhead (interpreter, object store, daemons) | Tiny native node: almost all RAM is left for your data |
| Hard to configure: head node, ports, config files, version matching | One binary, one line: `ring run --pool a,b,c`. Nodes with a different version refuse to join, with a clear message |
| Insecure by default: open ports, unauthenticated nodes can run code | Mutual authentication and encryption by default; nodes only run code signed by the program owner |
| Head node is a single point of failure | No special head: any node can coordinate, and the pool survives its loss |
| Data silently spilled to disk or pulled over the network, causing surprise slowness | Remote and spilled access is visible in the types and in a built-in profiler |
| Serialization overhead (pickling Python objects) | Fixed binary layouts: data moves as raw bytes with no conversion |
| Hard to debug: logs scattered across machines, vague failures | Errors carry the node, task, and cause; one command shows the whole pool; every remote call returns `pain` explicitly |
| A crashed node loses its data | Replication policy is a visible, simple setting (`copies 2`), with its RAM cost shown up front |
| Slow start for small jobs | A task starts in microseconds, not seconds |

## Future roadmap (parallel and distributed)
Build order. Each phase must not block the next, and none may bloat a program that doesn't use it.
1. **Compiler basics** — lexer, parser, x86-64 codegen, tiny `.exe` (current work).
2. **Local tasks** — share-nothing tasks and message passing.
3. **Multicore scheduler** — tasks spread across all cores automatically.
4. **Ring Protocol (RP)** — authenticated, encrypted messages between two machines, on a LAN or over the open internet (see below).
5. **Pools** — a named group of machines, with join, leave, and failure handling.
6. **`big` collections** — partitioned data, with work moved to the data.
7. **Placement and replication** — smart scheduling and the `copies N` policy.
8. **Tooling** — a pool profiler, one-command cluster status, and cross-machine tracing.

## Ring Protocol (RP)
Ring has its own small network protocol so combining machines and running programs on them is easy.

**Rule: invent the protocol, never the cryptography.** Encryption and authentication use proven pieces only (Noise-style handshake, Ed25519 signatures, ChaCha20-Poly1305). RP starts on top of TCP, and QUIC comes later. What is ours is the message format and rules.

**Easy to use**
- `ring pool new` prints a short join token. `ring join <token>` on any machine adds it. The token carries the pool key, so there is no certificate setup.
- On a local network, machines discover each other automatically.
- `ring run app` on any member spreads the program across the pool. A member can leave at any time and the pool rebalances.

**Works on the open internet, not only a LAN**
- Pool members can be anywhere: home machines, cloud servers, and offices mixed in one pool.
- Direct connections wherever possible. Most machines sit behind a router (NAT), so RP tries direct connection, then hole punching, then falls back to a relay node. A relay only forwards encrypted bytes and can never read them.
- A machine with an open port can act as a relay for the others, so no central service is required.
- Every connection is encrypted and mutually authenticated. Internet-facing means no plain-text mode, no anonymous nodes, and no open-by-default services.
- Built to survive the internet: slow links, lost packets, reconnects, and a changing IP address. Latency and bandwidth are visible to the scheduler, so `big` collections place work sensibly across slow links.
- Rate limits and size limits on every message, so a hostile peer cannot exhaust a node's RAM or CPU.

**Scale: from 2 machines to 10,000 or more**
- The same program and the same commands work for a pool of 2 or 10,000+. Nothing is reconfigured as it grows.
- No full mesh. At 10,000 machines, every node connecting to every other (about 50 million links) cannot work. Each node keeps connections to a small number of peers only, and messages route through them.
- Membership by gossip (SWIM-style): nodes tell a few random peers who joined, left, or died, so failure detection stays cheap and no central list exists.
- Data placement by consistent hashing: adding or removing a machine moves only a small slice of the data, not everything.
- Coordination is a tree or hierarchy that any node can take part in, so there is still no head node and no single bottleneck.
- Per-node overhead stays almost flat: a node's RAM and CPU spent on pool bookkeeping must not grow with pool size beyond a small logarithmic factor.
- Tested at scale: simulate 10,000 nodes before we claim it works. Until then the claim is a goal, not a fact.

**Protocol shape (draft)**
- A small fixed header on every message: type, length, task ID.
- Handshake: version check, mutual authentication, encrypted channel.
- Message types: `join`, `leave`, `send task`, `data chunk`, `result`, `pain` (error).
- Data is sent in Ring's in-memory binary layout, so there is no encode or decode step.

**Costs we accept**
- A published protocol must stay compatible forever (ties to goal 13).
- NAT traversal and relays are real work and come after the LAN version works.
- It stays an optional library: hello world never includes it (goals 18 and 21).

## Ideas on the shelf (not committed yet)
Zero-cost safety (prove bounds checks away) · reproducible builds · capability-based security for libraries · cross-compile anywhere with no setup · WebAssembly and microcontroller targets · hot reload · a 50-year compatibility promise · measurably the fewest LLM mistakes of any language.

## How we hold ourselves to this
- Measure, don't argue: benchmark syntax size, compile time, binary size, and runtime.
- Say no often. A new feature needs a measurable win on a benchmark.
- Never trade one goal quietly for another. If two goals conflict, write the decision down here.

## Decisions log
- No C backend (breaks fastest-compile and smallest-binary). Direct x86-64 PE output instead.
- Hot reload and full safety proofs are shelved: they cost binary size or compile time, which conflict with goals 1, 2, 17, and 18.
- Memory vs speed: when they conflict, small memory wins unless it costs more than ~2x speed. No GC in exchange (see goal 21).
- Functions have no keyword; `if`/`else` stay plain; body-part words only where the metaphor is obvious (see `SYNTAX.md`).
