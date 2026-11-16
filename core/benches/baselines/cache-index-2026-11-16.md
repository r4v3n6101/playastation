# Block-index comparison — 2026-11-16

Baseline: benchmark commit `8f9b472`, after policy commit `8f557a3`.
The third commit selects the custom page table after comparing HashMap/FNV,
blart 0.6.0 and the custom table. CPU engine, programs, cycle budgets and
validation are identical across the three runs.

Environment: Apple M4 (10 cores, 16 GiB), macOS 27.2.1; Rust
`1.99.0-nightly (c4af71034 2026-07-06)`, LLVM 22.1.8; Criterion 0.8.2.
Bench/release defaults, 50 samples, 1 s warm-up, 3 s measurement.
Runs were sequential, without CPU pinning. Time below is the Criterion slope
estimate in microseconds per complete guest program pass; lower is better.

## Warm cached execution

| Workload | HashMap | blart | Custom table | Custom vs HashMap |
|---|---:|---:|---:|---:|
| alu | 126.120 | 93.183 | 97.064 | -23.0% |
| branches | 258.362 | 189.508 | 173.761 | -32.7% |
| ram_copy | 116.869 | 91.239 | 83.954 | -28.2% |
| dispatch_64 | 1.771 | 1.524 | 1.303 | -26.4% |
| dispatch_4096 | 125.389 | 111.234 | 107.949 | -13.9% |
| invalidation | 43.824 | 58.367 | 40.695 | -7.1% |

## Cold first pass

| Workload | HashMap | blart | Custom table | Custom vs HashMap |
|---|---:|---:|---:|---:|
| alu | 126.896 | 91.526 | 98.781 | -22.2% |
| branches | 259.628 | 182.774 | 175.510 | -32.4% |
| ram_copy | 117.420 | 85.224 | 84.189 | -28.3% |
| dispatch_64 | 8.123 | 10.103 | 6.779 | -16.5% |
| dispatch_4096 | 486.198 | 631.634 | 424.869 | -12.6% |
| invalidation | 44.475 | 58.327 | 40.628 | -8.6% |

Cold timing includes decoding and insertion, but excludes engine construction
and destruction. A first pass can already revisit cached blocks in loop workloads.

## Allocations

Requested live heap bytes after a pass, including decoded blocks, the index
and page tracking; this is neither process RSS nor peak allocation size.
Bus/RAM image allocations are excluded. The allocation counter runs in a
separate executable and does not affect the timing measurements.

| Workload | HashMap bytes | blart bytes | Custom bytes |
|---|---:|---:|---:|
| alu | 12780 | 12824 | 19960 |
| branches | 13104 | 13280 | 20280 |
| ram_copy | 12684 | 12728 | 19864 |
| dispatch_64 | 26136 | 27584 | 34304 |
| dispatch_4096 | 888984 | 946576 | 955112 |
| invalidation | 12636 | 12680 | 19816 |

| State / operation | HashMap | blart | Custom |
|---|---:|---:|---:|
| Empty engine heap, bytes | 12288 | 12288 | 17408 |
| After clearing 4096 blocks, bytes | 167960 | 28688 | 33808 |
| Allocations to fill 4096 blocks | 12623 | 17750 | 12997 |
| Allocations in invalidation workload | 1284 | 2052 | 1285 |

## Why the custom table

It reduces time in all measured cached workloads relative to HashMap, and
avoids the cold-fill and invalidation regressions seen with blart. blart wins
the ALU loop. The custom table uses 955112 live heap bytes in the 4096-block
case: about 7.4% more than HashMap and 0.9% more than blart. This is a tradeoff
for simpler indexed lookup and fewer allocation operations than blart, not a
claim that the custom structure uses the least memory.

## Structure

Like [DuckStation's lookup](https://github.com/stenzek/duckstation/blob/82a11824976bb9afe1d5d433025aa6ccf99e89be/src/core/cpu_code_cache.cpp#L414),
the index splits PC into a table number and an instruction slot. This implementation
uses normalized physical addresses and a 640-entry root for 2 MiB RAM plus
512 KiB BIOS. The root consumes 5120 bytes on a 64-bit host; lower tables are lazy.
Each lower table covers 4 KiB of guest memory and has 1024 u16 slots. A slot
contains zero or a one-based index into a dense vector of `(word offset, block)`
pairs. Lookup therefore includes an extra vector access compared with a direct
pointer table. `swap_remove` repairs the moved block's slot in constant time.

Empty lower tables are retained during ordinary invalidation, then freed by
`invalidate_all`. Existing overlap-page tracking, RAM mirrors, Rc handles and
early exit are preserved. Unaligned PCs and physical regions outside RAM/BIOS
use an alloc-only BTreeMap so they retain exact-address semantics.

The u16 slot arrays for all RAM pages would total 1 MiB, plus 256 KiB for BIOS;
they are not allocated up front. These bounds exclude block vectors, decoded
instructions and other metadata. This is an independently written compact index,
not a port of DuckStation's pointer LUT implementation.

## Reproducing the comparison

Use the same toolchain and an explicit shared CARGO_TARGET_DIR. At the baseline
commit, run `cargo bench -p playastation --bench cpu --locked -- --save-baseline hashmap`
and `cargo bench -p playastation --bench cpu_memory --locked`.
The [blart patch](../variants/blart.patch) applies to commit `8f9b472`; it changes
only the index and its dependencies. For that variant use `--save-baseline blart`.
For the final custom implementation use `--save-baseline lut`.

All timing estimates and 95% confidence intervals, including the plain-interpreter
controls, are stored in `hashmap-2026-11-16.csv`, `blart-2026-11-16.csv` and
`lut-2026-11-16.csv`. The corresponding `*-memory-2026-11-16.csv` files hold
the allocation results. Full Criterion samples from this run remain under
`/private/tmp/playastation-policy-bench-target/criterion`.

## Validation and limits

The isolated policy commit passed 19 core tests. The final table passed 22 core
tests, all 24 benchmark smoke cases, the allocation/deallocation checks, and
the no_std wasm32 library check. New tests exercise a full 1024-slot page,
replacement, removal and reinsertion, unaligned PCs, region separation and clearing.

These are synthetic CPU/cache workloads; they do not predict game FPS or BIOS
boot time. The comparison is one sequential run per variant. For the plain
interpreter controls, time changes relative to HashMap were:
- blart: -1.2% to +5.2%.
- lut: -1.5% to +5.6%.

Small cross-variant differences can include run-to-run noise or code-layout effects.
- `cpu.rs` SHA-256: `e2a03760a8616a4e5b2124a95783c9014fd73c22954eb471a90b466b65b153fc`.
- `support/mod.rs` SHA-256: `4588ec018e5be3f56ea4a9bc06f6549542d878773be33961e85c51f0ee145e79`.
- `cpu_memory.rs` SHA-256: `6c0ab3f57f3273554aaf0a17a3a23b2b3367adb3815eab9432103ecb0bc1c402`.
