use std::time::Instant;
use std::io::Write;

use chrono::NaiveDate;
use gtfs_structures::GtfsReader;

use raptor::network::Network;
use raptor::utils::const_unwrap;

use crate::simulation::{AgentCount, CrowdingCost, PopulationCount, SimulationParams, SimulationResult};
use crate::utils::create_pool;

mod simulation;
mod data_import;
mod data_export;
mod utils;

// Simulation notes:
// When we get the O-D data, we can run journey planning for each OD and apply the passenger counts to the relevant trips.
// once this is run once, we update the journey planning weights based on the crowding and run again.
// This is like the 'El Farol Bar' problem.
// Matsim-like replanning for a proportion of the population might also be viable.

pub struct DefaultSimulationParams {
    pub max_train_capacity: AgentCount,
}

impl DefaultSimulationParams {
    pub const fn new(max_train_capacity: AgentCount) -> Self {
        let result = Self {
            max_train_capacity,
        };

        result
    }
    fn f(x: CrowdingCost) -> CrowdingCost {
        const B: CrowdingCost = 5.;
        let bx = B * x;
        let ebx = bx.exp();
        (ebx - 1.) / (B.exp() - 1.)
    }
}

impl SimulationParams for DefaultSimulationParams {
    fn max_train_capacity(&self) -> AgentCount {
        self.max_train_capacity
    }

    fn cost_fn(&self, count: PopulationCount) -> CrowdingCost {
        debug_assert!(count >= 0, "Negative population count");
        let proportion = count as CrowdingCost / self.max_train_capacity() as CrowdingCost;
        Self::f(proportion)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let exec_start = Instant::now();

    // Set up network.
    let network = {
        let gtfs_start = Instant::now();
        // PTV GTFS:
        // 1 - Regional Train
        // 2 - Metropolitan Train
        // 3 - Metropolitan Tram
        // 4 - Metropolitan Bus
        // 5 - Regional Coach
        // 6 - Regional Bus

        //let gtfs = GtfsReader::default().read_shapes(false).read("../gtfs/2/google_transit_no_shapes.zip")?;
        let gtfs = GtfsReader::default().read_shapes(true).read("../gtfs/2/google_transit.zip")?;
        //let gtfs = GtfsReader::default().read_shapes(true).read("../gtfs/3/google_transit.zip")?;
        //let gtfs = GtfsReader::default().read_shapes(true).read("../gtfs/4/google_transit.zip")?;
        let journey_date = const_unwrap(NaiveDate::from_ymd_opt(2024, 5, 10));

        //let gtfs = GtfsReader::default().read_shapes(false).read("../gtfs_processing/TokyoGTFS/tokyo_trains.zip")?;
        //let gtfs = GtfsReader::default().read_shapes(false).read("../gtfs_processing/tube-gtfs.zip")?;
        //let journey_date = const_unwrap(NaiveDate::from_ymd_opt(2024, 6, 8));

        //let gtfs = GtfsReader::default().read_shapes(true).read("../gtfs_processing/la_gtfs.zip")?;
        //let gtfs = GtfsReader::default().read_shapes(true).read("../gtfs_processing/ny_gtfs.zip")?;
        //let gtfs = GtfsReader::default().read_shapes(true).read("../gtfs_processing/de_gtfs.zip")?;
        //let journey_date = const_unwrap(NaiveDate::from_ymd_opt(2024, 6, 24));

        //let gtfs = GtfsReader::default().read_shapes(true).read("../gtfs_processing/srl-gtfs")?;
        //let gtfs = GtfsReader::default().read_shapes(true).read("../gtfs_processing/SRL/data/srl-gtfs")?;

        println!("GTFS import: {:?}", gtfs_start.elapsed());
        gtfs.print_stats();

        let default_transfer_time = 3 * 60;
        let network_start = Instant::now();
        let mut network = Network::new(&gtfs, journey_date, default_transfer_time);
        println!("Network parse: {:?}", network_start.elapsed());

        // Set Flinders Street transfer time.
        //let flinders = network.get_stop_idx_from_name("Flinders Street").unwrap() as usize;
        //network.transfer_times[flinders] = 4 * 60;

        let connections_start = Instant::now();
        network.build_connections();
        println!("Build connections: {:?}", connections_start.elapsed());

        network.print_stats();

        network
    };

    // Set up simulation.
    let params = DefaultSimulationParams::new(
        // From VicSig: X'Trapolis 3-car has 264 seated, 133 standing. A 6-car has 794 in total.
        // Crush capacity is 1394, but that's a bit mean.
        // https://vicsig.net/suburban/train/X'Trapolis
        794,
    );

    loop {
        print!("Enter number of processors to use: ");
        std::io::stdout().flush()?;
        let mut num_procs = String::new();
        std::io::stdin().read_line(&mut num_procs)?;
        let num_processors = num_procs.trim().parse()?;
        // Set up thread pool for benchmarking.
        create_pool(num_processors)?.install(|| -> std::io::Result<()> {

            // Run prefix sum benchmark.
            if false {
                simulation::simulation_prefix_benchmark(&network, &params, "../data/benchmark.csv")?;
            }

            // Run simulation and print duration to csv.
            print!("Enter number of agents to use: ");
            std::io::stdout().flush()?;
            let mut num_agents = String::new();
            std::io::stdin().read_line(&mut num_agents)?;
            let num_agents = num_agents.trim().parse().unwrap();
            let simulation_steps = simulation::gen_simulation_steps(&network, Some(num_agents), Some(0));

            let mut simulation_result = SimulationResult { agent_journeys: Vec::new() };
            let simulation_start = Instant::now();
            let num_iterations = 1;
            for _ in 0..num_iterations {
                simulation_result = simulation::run_simulation::<_, true>(&network, &simulation_steps, &params);
            }
            let duration = simulation_start.elapsed() / (num_iterations * num_agents as u32);

            // Append to csv.
            if false {
                use std::fs::OpenOptions;
                use std::path::Path;

                let simulation_benchmark_path = "../data/simulation_scaling.csv";
                let exists = Path::new(simulation_benchmark_path).exists();
                let mut simulation_benchmark_file = OpenOptions::new().append(true).create(true).open("../data/simulation_benchmark.csv")?;
                if !exists {
                    writeln!(&mut simulation_benchmark_file, "num_processors,duration")?;
                }
                writeln!(&mut simulation_benchmark_file, "{num_processors},{}", duration.as_micros())?;

                println!("Simulation duration {} microseconds", duration.as_micros());
            }

            println!("Exporting results.");
            let export_start = Instant::now();
            data_export::export_agent_counts("../data/counts.parquet", &network, &simulation_result).unwrap();
            if network.has_shapes {
                data_export::export_shape_file("../train-vis/src/data/shapes.bin.zip", &network).unwrap();
                data_export::export_network_trips("../train-vis/src/data/trips.bin.zip", &network, &simulation_result).unwrap();
            } else {
                println!("Warning: GTFS shapes not loaded, no visualisation export.");
            }
            println!("Export duration: {:?}", export_start.elapsed());

            println!();
            println!("Total time: {:?}", exec_start.elapsed());

            Ok(())
        })?;
    }
}
