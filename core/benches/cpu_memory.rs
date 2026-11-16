use std::{
    alloc::{GlobalAlloc, Layout, System},
    mem::size_of,
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
};

use playastation::run::{CpuEngine, ExecutionMode};

mod support;

struct CountingAllocator;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// A separate executable: allocation accounting never runs in the timed benchmarks.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            LIVE_BYTES.fetch_add(layout.size(), Relaxed);
            ALLOCATIONS.fetch_add(1, Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE_BYTES.fetch_sub(layout.size(), Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !ptr.is_null() {
            LIVE_BYTES.fetch_sub(layout.size(), Relaxed);
            LIVE_BYTES.fetch_add(new_size, Relaxed);
            ALLOCATIONS.fetch_add(1, Relaxed);
        }
        ptr
    }
}

fn main() {
    println!(
        "workload,instructions,engine_size,empty_heap,filled_heap,cleared_heap,fill_allocations"
    );
    for w in support::workloads() {
        w.validate(ExecutionMode::CachedInterpreter);
        let mut bus = w.bus();
        let before = LIVE_BYTES.load(Relaxed);
        let mut engine = CpuEngine::new(ExecutionMode::CachedInterpreter);
        let empty = LIVE_BYTES.load(Relaxed) - before;
        let allocations = ALLOCATIONS.load(Relaxed);
        let mut cpu = w.cpu;
        let result = engine.run_for(&mut cpu, &mut bus, w.cycles);
        let filled = LIVE_BYTES.load(Relaxed) - before;
        let allocations = ALLOCATIONS.load(Relaxed) - allocations;
        w.check(&cpu, &bus, &result);
        engine.cache_invalidate_all();
        let cleared = LIVE_BYTES.load(Relaxed) - before;
        drop(engine);
        assert_eq!(LIVE_BYTES.load(Relaxed), before, "cache allocations leaked");
        println!(
            "{},{},{},{empty},{filled},{cleared},{allocations}",
            w.name,
            w.instructions,
            size_of::<CpuEngine<ExecutionMode>>()
        );
    }
}
