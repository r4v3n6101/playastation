use std::{hint::black_box, time::Duration};

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use playastation::run::{CpuEngine, ExecutionMode};

mod support;

fn cpu(c: &mut Criterion) {
    for w in support::workloads() {
        for mode in [ExecutionMode::Interpreter, ExecutionMode::CachedInterpreter] {
            w.validate(mode);
        }
        for cold in [false, true] {
            let phase = if cold { "cold" } else { "warm" };
            let mut group = c.benchmark_group(format!("cpu/{phase}/{}", w.name));
            group.throughput(Throughput::Elements(w.instructions));
            for (name, mode) in [
                ("interpreter", ExecutionMode::Interpreter),
                ("cached", ExecutionMode::CachedInterpreter),
            ] {
                let mut bus = w.bus();
                group.bench_function(BenchmarkId::from_parameter(name), |b| {
                    if cold {
                        b.iter_batched_ref(
                            || (CpuEngine::new(mode), w.cpu),
                            |(engine, cpu)| {
                                let result = engine.run_for(cpu, &mut bus, black_box(w.cycles));
                                black_box((&*cpu, &bus.ram, result));
                            },
                            BatchSize::PerIteration,
                        );
                    } else {
                        let mut engine = CpuEngine::new(mode);
                        let mut cpu = w.cpu;
                        let result = engine.run_for(&mut cpu, &mut bus, w.cycles);
                        w.check(&cpu, &bus, &result);
                        b.iter_batched_ref(
                            || w.cpu,
                            |cpu| {
                                let result = engine.run_for(cpu, &mut bus, black_box(w.cycles));
                                black_box((&*cpu, &bus.ram, result));
                            },
                            BatchSize::PerIteration,
                        );
                    }
                });
            }
            group.finish();
        }
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(50)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = cpu
}
criterion_main!(benches);
