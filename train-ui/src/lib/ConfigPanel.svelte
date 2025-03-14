<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Channel } from "@tauri-apps/api/core";
  import { callBackend, callBackendWithWaitCursor, runWithWaitCursor } from "$lib/utilities";
  import Button from "$lib/Button.svelte";
  import CrowdingModelSelector, { type CrowdingFunc, type TripCapacity } from "$lib/CrowdingModelSelector.svelte";

  let dispatch = createEventDispatcher<{
    "simulation-finished": void;
  }>();

  // Binding for the file input property.
  let inputFiles: FileList | null = null;

  // Binding for the date input element.
  let dateInput: HTMLInputElement;

  // Matches the rust struct.
  type DateRange = {
    min: string,
    max: string,
  };

  let allowedDateRange: DateRange | undefined = undefined;

  // A bit of fancy async/await to handle the file upload.
  async function loadGTFS(event: Event) {
    if (inputFiles && inputFiles.length > 0) {
      let file = inputFiles[0];
      if (file.name.endsWith(".zip")) {
        allowedDateRange = undefined;

        allowedDateRange = await runWithWaitCursor(async () => {
          let loadedGtfsZip = await file.arrayBuffer();
          return await callBackend("load_gtfs", loadedGtfsZip);
        });

        if (allowedDateRange) {
          console.log("Allowed date range:", allowedDateRange);
          dateInput.min = allowedDateRange.min;
          dateInput.max = allowedDateRange.max;
          invalidateSimulation();
        } else {
          alert("Failed to load GTFS. Please try again.");
          allowedDateRange = undefined;
          (event.currentTarget as HTMLInputElement).value = "";
        }
      } else {
        alert("Invalid file type. Please load a .zip file.");
        inputFiles = null;
        allowedDateRange = undefined;
        (event.currentTarget as HTMLInputElement).value = "";
      }
    }
  }

  let modelDate = "2024-05-20";

  let numRounds = 3;
  let bagSize = 3;
  let crowdingFunc: CrowdingFunc;
  let defaultTripCapacity: TripCapacity;
  let costUtility: number;

  let networkValid = false;
  let randomPatronageData = false;
  let patronageDataValid = false;
  let simulationResultsValid = false;

  async function generateNetwork() {
    await callBackendWithWaitCursor("gen_network", {
      modelDate,
   //   modeFilter: "rail"
    });
    networkValid = true;
  }

  async function patronageDataImport() {
    await callBackendWithWaitCursor("patronage_data_import");
    patronageDataValid = true;
  }

  type SimulationEvent =
    | { event: "Started"; data: { numRounds: number, numSteps: number }; }
    | { event: "StepCompleted"; };

  const onSimulationEvent = new Channel<SimulationEvent>();

  onSimulationEvent.onmessage = (event) => {
    switch (event.event) {
      case "Started":
        console.log("Simulation started: %d rounds each with %d steps.", event.data.numRounds, event.data.numSteps);
        break;
      case "StepCompleted":
        //console.log("Simulation progress 1.");
        break;
    }
  };

  let simulationRunning = false;
  $: simulationDisabledTooltip = simulationRunning ? "Simulation currently running." : "Generate network and import patronage data first.";
  $: canRunSimulation = networkValid && (patronageDataValid || randomPatronageData) && !simulationRunning;
  async function runSimulation() {
    if (simulationRunning) {
      return;
    }
    simulationRunning = true;
    try {
      await callBackendWithWaitCursor("run_simulation", {
        numRounds,
        bagSize,
        costUtility,
        crowdingFunc,
        defaultTripCapacity,
        shouldReportProgress: false,
        genRandomSteps: randomPatronageData,
        onSimulationEvent,
      });
    } finally {
      simulationRunning = false;
    }
    simulationResultsValid = true;
    dispatch("simulation-finished");
  }

  function invalidateSimulation() {
    // Invalidate the network and patronage data when the date changes.
    networkValid = false;
    patronageDataValid = false;
    simulationResultsValid = false;
  }

</script>

<div class="cfg-panel">
  <div class="cfg-label">
    <label for="gtfs">Load GTFS:</label>
    <input
      type="file"
      accept=".zip"
      id="gtfs"
      class="cfg-style cfg-input"
      bind:files={inputFiles}
      on:change={loadGTFS}
    />
    <!--TODO: Add a green signal to indicate the gtfs is loaded (and each step's prerequisite).-->
  </div>

  <div class="cfg-label">
    <label for="model-date">Date to Model:</label>
    <input
      type="date"
      id="model-date"
      class="cfg-style cfg-input"
      disabled={allowedDateRange === undefined}
      bind:this={dateInput}
      bind:value={modelDate}
      on:change={invalidateSimulation}
    />
  </div>


  <Button
    text="Generate Network"
    class="cfg-style"
    disabled={allowedDateRange == null}
    disabledTooltip="Load GTFS and select date first."
    processIndicator={true}
    processComplete={networkValid}
    on:click={generateNetwork}
  />

  <div class="cfg-label">
    <Button
      text="Patronage Data Import"
      class="cfg-style"
      disabled={!networkValid || randomPatronageData}
      processIndicator={true}
      processComplete={patronageDataValid || randomPatronageData}
      on:click={patronageDataImport}
    />
    <label for="random">Random:</label>
    <input type="checkbox" id="random" bind:checked={randomPatronageData}>
  </div>

  <CrowdingModelSelector bind:crowdingFunc bind:defaultTripCapacity bind:costUtility />

  <div class="cfg-label">
    <label for="round-num">Rounds:</label>
    <input
      type="range"
      id="round-num"
      min="1"
      max="10"
      class="cfg-input"
      disabled={!canRunSimulation}
      bind:value={numRounds}
    />
    <span>{numRounds}</span>
  </div>

  <div class="cfg-label">
    <label for="bag-size">Journey Options Considered:</label>
    <input
      type="range"
      id="bag-size"
      min="2"
      max="5"
      step="1"
      class="cfg-input"
      disabled={!canRunSimulation}
      bind:value={bagSize}
    />
    <span>{bagSize}</span>
  </div>

  <Button
    text="Run Simulation"
    class="cfg-style"
    disabled={!canRunSimulation}
    disabledTooltip={simulationDisabledTooltip}
    processIndicator={true}
    processComplete={simulationResultsValid}
    on:click={runSimulation}
  />

  <div class="cfg-export">
    <Button
      text="Export Counts"
      class="cfg-style"
      disabled={!simulationResultsValid}
      disabledTooltip="Run simulation first."
      on:click={() => callBackendWithWaitCursor("export_counts")}
    />
    <Button
      text="Export Journeys"
      class="cfg-style"
      disabled={!simulationResultsValid}
      disabledTooltip="Run simulation first."
      on:click={() => callBackendWithWaitCursor("export_journeys", { legs: false })}
    />
    <Button
      text="Export Legs"
      class="cfg-style"
      disabled={!simulationResultsValid}
      disabledTooltip="Run simulation first."
      on:click={() => callBackendWithWaitCursor("export_journeys", { legs: true })}
    />
    <Button
      text="Export Transfers"
      class="cfg-style"
      disabled={!simulationResultsValid}
      disabledTooltip="Run simulation first."
      on:click={() => callBackendWithWaitCursor("export_transfers")}
    />
  </div>
</div>

<style>
  .cfg-panel {
    display: flex;
    flex-direction: column;
    justify-content: start;
    align-items: flex-start;
    gap: 15px;
  }

  .cfg-panel :global(.cfg-style) {
    width: 100%;
    padding: 4px;
    font-size: 1rem;
    border-radius: 5px;
    border: none;
  }

  .cfg-label {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  label {
    font-size: 1.2rem;
    color: white;
    flex: 1;
  }

  .cfg-input {
    flex: 2;
  }

  span {
    flex: 0.1;
    text-align: center;
  }

  input {
    background-color: #5e503f;
    color: white;
  }

  input[type="file"] {
    color-scheme: light;
  }

  input[type="range"] {
    flex: 1;
    padding: 0;
  }

  input:disabled {
    background-color: #423525;
    color: #a28a6f;
  }

  .cfg-export {
    display: flex;
    width: 100%;
    gap: 20px;
  }
</style>
