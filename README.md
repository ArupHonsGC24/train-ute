# The "Who's on Board?" Rail Service Demand model.

This is the main repository for the "Who's on Board?" model, a method of assigning passenger trips from origin-destination trip data derived from smart card systems.
This software was created for a Bachelor of Science Advanced: Global Challenges honours project.

The design decisions have been informed by using this model for Melbourne's metropolitan train network, but in principle the model should be generally applicable given sufficient data.

## Building

This project uses Rust for the backend, and Svelte for the frontend (enabled by the Tauri framework). To set up the project, you will need Rust and Node.js/whatever you like that can read package.json files.
The Raptor submodule is a self-contained crate that contains the pathfinding logic, so you'll need to clone with submodules:
```bash
git clone -recurse-submodules https://github.com/ArupHonsGC24/train-ute.git
```

From there, you'll need to install the v2.0 version of the Tauri CLI tool:
```bash
cargo install tauri-cli --locked
```

Then from the project directory run
```bash
cargo tauri build
```
to build the model as an executable.

## Data inputs

The model takes various data inputs:
- A static GTFS feed (such as from [DataVIC](https://discover.data.vic.gov.au/dataset/timetable-and-geographic-information-gtfs).
- Patronage data, as a list of trips to make on the network.
- Optionally, some per-trip train capacities to allow varying rollingstock across the network.

## Patronage data:

Currently, the patronage data needs to be in a specific format. Due to its size it's read in as a parquet, and contains the following columns:

| Origin_Station (VARCHAR) | Destination_Station (VARCHAR) | Departure_Time (Time64MicrosecondType) | Agent Count (Int32) |
|--------------------------|-------------------------------|----------------------------------------|---------------------|
| ...                      | ...                           | ...                                    | ...                 |
| Glenbervie               | Melbourne Central             | 08:15:00                               | 4                   |
| Ferntree Gully           | Southern Cross                | 08:15:00                               | 3                   |
| ...                      | ...                           | ...                                    | ...                 |

There is also a option to use randomised patronage data (a generated list of randomly selected pairs of points across the network), which lets you get a feel for how the model works.
