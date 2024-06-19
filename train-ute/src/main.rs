use std::time::Instant;

use chrono::NaiveDate;
use gtfs_structures::GtfsReader;

use raptor::network::Network;

use crate::simulation::{AgentCount, CrowdingCost, PopulationCount, SimulationParams};

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
    cost_lookup_table: [CrowdingCost; Self::SAMPLES + 1],
}

impl DefaultSimulationParams {
    const SAMPLES: usize = 1000;
    pub fn new(max_train_capacity: AgentCount) -> Self {
        let mut result = Self {
            max_train_capacity,
            cost_lookup_table: [0.; Self::SAMPLES + 1],
        };

        for i in 0..=Self::SAMPLES {
            result.cost_lookup_table[i] = Self::f((i as CrowdingCost) / Self::SAMPLES as CrowdingCost);
        }

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
        let gtfs = GtfsReader::default().read_shapes(true).read("../gtfs/2/google_transit.zip")?;
        //let gtfs = GtfsReader::default().read_shapes(true).read("../gtfs_processing/TokyoGTFS/tokyo_trains.zip")?;
        //let gtfs = GtfsReader::default().read_shapes(true).read("../gtfs_processing/tube-gtfs.zip")?;
        //let gtfs = GtfsReader::default().read_shapes(true).read("../gtfs_processing/srl-gtfs")?;
        //let gtfs = GtfsReader::default().read_shapes(true).read("../gtfs_processing/SRL/data/srl-gtfs")?;

        println!("GTFS import: {:?}", gtfs_start.elapsed());

        let journey_date = NaiveDate::from_ymd_opt(2024, 6, 3).unwrap();
        let default_transfer_time = 3 * 60;
        let network_start = Instant::now();
        let mut network = Network::new(&gtfs, journey_date, default_transfer_time);
        println!("Network parse: {:?}", network_start.elapsed());
        
        // Set Flinders Street transfer time.
        let flinders = network.get_stop_idx_from_name("Flinders Street").unwrap() as usize;
        network.transfer_times[flinders] = 4 * 60;

        let connections_start = Instant::now();
        //network.build_connections();
        println!("Build connections: {:?}", connections_start.elapsed());
        
        network
    };

    // Set up simulation
    let params = DefaultSimulationParams::new(
        // From VicSig: X'Trapolis 3-car has 264 seated, 133 standing. A 6-car has 794 in total.
        // Crush capacity is 1394, but that's a bit mean.
        // https://vicsig.net/suburban/train/X'Trapolis
        794,
    );

    let simulation_steps = simulation::gen_simulation_steps(&network, Some(0));

    let simulation_start = Instant::now();
    let simulation_result = simulation::run_simulation(&network, &simulation_steps, &params);
    println!("Simulation duration: {:?} to run {} steps", simulation_start.elapsed(), simulation_steps.len());

    println!("Exporting results.");
    let export_start = Instant::now();

    data_export::export_shape_file("../train-vis/src/data/shapes.bin.zip", &network)?;
    data_export::export_network_trips("../train-vis/src/data/trips.bin.zip", &network, &simulation_result)?;

    println!("Export duration: {:?}", export_start.elapsed());

    println!();
    println!("Total time: {:?}", exec_start.elapsed());

    Ok(())
}
