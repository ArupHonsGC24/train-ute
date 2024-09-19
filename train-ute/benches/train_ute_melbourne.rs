use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

use dev_utils::{build_example_network, load_example_gtfs};
use train_ute::simulation::{gen_simulation_steps, run_simulation, DefaultSimulationParams};

fn train_ute_benchmark(c: &mut Criterion) {
    let gtfs = load_example_gtfs().unwrap();
    let network = build_example_network(&gtfs);

    let params: DefaultSimulationParams = DefaultSimulationParams::new(794, None);

    let simulation_steps = gen_simulation_steps(&network, Some(100), Some(0));

    c.bench_function("Train Ute Simulation", |b| b.iter(|| run_simulation::<_, true>(&network, black_box(&simulation_steps), black_box(&params))));
}

criterion_group!(benches, train_ute_benchmark);
criterion_main!(benches);
