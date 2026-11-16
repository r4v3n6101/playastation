# CPU benchmarks

Run from the repository root, using the project's Rust toolchain:

```sh
cargo test -p playastation --lib --locked
cargo bench -p playastation --bench cpu --locked -- --test
cargo bench -p playastation --bench cpu --locked -- --save-baseline hashmap
cargo bench -p playastation --bench cpu_memory --locked
```

After changing the cache implementation, use the same toolchain, build options,
machine and target directory:

```sh
cargo bench -p playastation --bench cpu --locked -- --baseline hashmap
```

Criterion keeps the full samples and estimates under
`$CARGO_TARGET_DIR/criterion` (or `target/criterion` by default). Committed
baseline summaries are in `baselines/`.

The [three-way cache comparison](baselines/cache-index-2026-11-16.md) records
HashMap, blart and the custom page table, including allocation measurements.

## What is measured

`cpu.rs` runs identical MIPS programs through `CpuEngine`. The only difference
between the two modes is the policy: `ExecutionMode::Interpreter` always
executes a single instruction; `ExecutionMode::CachedInterpreter` always
executes a decoded block. Neither fixed policy needs a statistics lookup.

The default console policy is separate: it selects single instructions when
the remaining budget is below eight cycles. These benchmarks deliberately
measure the individual modes, rather than that mixture.

The MIPS programs and reference results live in `support/mod.rs` and are shared
by the timing and memory executables.

Every program finishes at a `BREAK` instruction. Before measuring, both modes
are checked against independently computed register and RAM results, expected
PC, pending load/branch state, and exact guest cycles. Validation runs with a
fresh cache, the same cache again, and after `invalidate_all`. The budget is
the known total cycle count, including `BREAK`; both modes must reach it with
the same stop reason. A cached block is never cut short to match a budget.

| Workload | Work per iteration | Guest instructions | Guest cycles |
|---|---|---:|---:|
| `alu` | 4096 xorshift32 and wrapping-sum iterations | 40961 | 40961 |
| `branches` | 4096 data-dependent branch iterations | 40961 | 40961 |
| `ram_copy` | Copy and sum 4096 words (16 KiB), respecting load delay | 32769 | 53249 |
| `dispatch_64` | Visit 64 distinct blocks in a fixed permutation | 513 | 513 |
| `dispatch_4096` | Visit 4096 distinct blocks in a fixed permutation | 32769 | 32769 |
| `invalidation` | 256 stores into the currently executing code page | 1281 | 1793 |

Instruction counts include branch-delay NOPs and the final `BREAK`. Criterion's
elements/second is **guest instructions/second**, not guest cycles/second.
The dispatch workloads execute eight instructions per block and put block
starts 64 bytes apart. They cover both a small and a larger cache working set.
The invalidation workload writes the same instruction value back: it measures
page invalidation, early exit and decoding again, without changing the workload
between repeats. A separate engine regression test changes the instruction and
checks that the new value executes.

## Timing boundaries

- **Warm:** execute one complete pass before timing, retain the engine/cache,
  and reset CPU registers outside each timed iteration. No cache clear occurs
  between iterations. Invalidation still happens naturally in the store test.
- **Cold:** create a new engine with an empty code cache outside every timed
  iteration. Decoding, insertion and execution are timed; engine construction
  and destruction are excluded. Within that first pass, repeated visits can
  already hit the cache. Thus `cold` means *first program pass*, not a cache miss
  on every instruction or block.
- Bus creation, RAM loading, reference calculations and validation are outside
  the timer in both modes. Input/code are fixed and outputs are overwritten on
  each pass, so the bus can be reused. Host CPU caches are not flushed.

The harness uses Criterion's `iter_batched_ref` with `PerIteration` to keep
setup/destruction outside the timer without retaining many large engine
instances. The measured routine is a complete program pass, not a single
lookup. See [Criterion's timing-loop documentation](https://bheisler.github.io/criterion.rs/book/user_guide/timing_loops.html).

The defaults are 50 samples, one second of warm-up and three seconds of
measurement per case. Both the program and the resulting state are observable
through `std::hint::black_box`. No tracing subscriber is installed.

## Memory measurements

`cpu_memory` is a separate executable with a counting allocator. It does not
measure time, and the Criterion executable does not use that allocator.
It reports bytes requested from the heap while the CPU engine is empty, after
one program pass, and after `invalidate_all`, plus the number of successful
allocations/reallocations during that pass. CPU/Bus construction, RAM images and
reference results are outside the measured allocation interval. Dropping the
engine must return the live allocation count to its starting value.

The heap totals include the index, decoded blocks and page tracking, not just
the index. `engine_size` is the separate inline size of `CpuEngine`. The numbers
exclude allocator bookkeeping, size-class rounding and resident-memory effects;
they are not process RSS. The invalidation case reports the surviving cache
after repeated stores, and its allocation count captures the churn.

## Scope

These are synthetic CPU/cache benchmarks, not BIOS boot, game frame times or
whole-console benchmarks. GPU, audio, DMA scheduling, GTE-heavy programs,
and indirect jumps are not measured. A change should
be evaluated across the cases, including cold execution and invalidation,
rather than by the fastest loop alone.

A future JIT policy can use the same programs, reference results and metrics.
Its warm setup must explicitly run enough passes to compile every intended hot
block before timing. The cold measurement should keep compilation inside the
timer. Separate compiler-only measurements can then explain first-pass cost.
