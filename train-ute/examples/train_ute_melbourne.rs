use std::fs;
use std::fs::File;
use std::path::Path;
use train_ute::simulation::DefaultSimulationParams;
use train_ute::{data_export, data_import, simulation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up network.
    let gtfs = dev_utils::load_example_gtfs()?;
    gtfs.print_stats();
    let network = dev_utils::build_example_network(&gtfs);
    network.print_stats();

    // Set up simulation.
    let params: DefaultSimulationParams = DefaultSimulationParams::new(
        // From VicSig: X'Trapolis 3-car has 264 seated, 133 standing. A 6-car has 794 in total.
        // Crush capacity is 1394, but that's a bit mean.
        // https://vicsig.net/suburban/train/X'Trapolis
        794,
        None,
    );

    let simulation_steps = data_import::build_simulation_steps_from_patronage_data(dev_utils::find_example_patronage_data()?, &network)?;

    let simulation_result = simulation::run_simulation::<_, true>(&network, &simulation_steps, &params);
    simulation_result.print_stats();

    let data_export_folder = Path::new("../train_ute_export");
    println!("Exporting simulation data to {:?}", data_export_folder.canonicalize()?);
    fs::create_dir_all(data_export_folder)?;

    data_export::export_agent_counts(&data_export_folder.join("agent_counts"), &network, &simulation_result)?;
    data_export::export_agent_journeys(File::create(&data_export_folder.join("agent_journeys.parquet"))?, &network, &simulation_result)?;

    Ok(())
}
