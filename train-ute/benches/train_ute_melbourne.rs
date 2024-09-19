use criterion::{criterion_group, criterion_main, Criterion, SamplingMode};
use std::hint::black_box;

use dev_utils::{build_example_network, load_example_gtfs};
use raptor::journey::JourneyPreferences;
use train_ute::simulation::{gen_simulation_steps, run_simulation, DefaultSimulationParams};

fn train_ute_benchmark(c: &mut Criterion) {
    let gtfs = load_example_gtfs().unwrap();
    let network = build_example_network(&gtfs);

    let params: DefaultSimulationParams = DefaultSimulationParams::new(794, None,
        JourneyPreferences::default());

    let num_threads = 16;

    rayon::ThreadPoolBuilder::new().num_threads(num_threads).build_global().unwrap();

    for num_steps in 2..=6 {
        let simulation_steps = gen_simulation_steps(&network, Some(10usize.pow(num_steps)), Some(0));
        let mut group = c.benchmark_group(format!("Train Ute Simulation {} steps", 10usize.pow(num_steps)));
        group.sampling_mode(SamplingMode::Flat);
        group.sample_size(10);
        group.bench_function(
            &format!("{num_threads} threads"),
            |b| b.iter(|| run_simulation(&network, black_box(&simulation_steps), black_box(&params))),
        );
        group.finish();
    }
}

criterion_group!(benches, train_ute_benchmark);
criterion_main!(benches);
