<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { callBackend, callBackendWithWaitCursor, runWithWaitCursor } from "./utilities";
  import Button from "./Button.svelte";

  let dispatch = createEventDispatcher();

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

  let modelDate = "2024-05-10";
  // TODO: invalidate on different date.
  let dataImportDisabled = true;
  let runSimulationDisabled = true;
  let exportResultsDisabled = true;

  async function generateNetwork() {
    await callBackendWithWaitCursor("gen_network", { modelDate });
    dataImportDisabled = false;
  }

  async function patronageDataImport() {
    await callBackendWithWaitCursor("patronage_data_import");
    runSimulationDisabled = false;
  }

  async function runSimulation() {
    await callBackendWithWaitCursor("run_simulation");
    exportResultsDisabled = false;
    dispatch("simulation-finished");
  }

</script>

<div id="cfg-panel">
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
    <!--TODO: Handle case of date being changed when network is already generated. Should invalidate on different date-->
    <input
      type="date"
      id="model-date"
      class="cfg-style cfg-input"
      disabled={allowedDateRange === undefined}
      bind:this={dateInput}
      bind:value={modelDate}
    />
  </div>

  <!--
    <div class="network-buttons">
      <Button text="Save Network to Disk" command="save_network" class="cfg-style" />
      <Button text="Load Network from Disk" command="load_network" class="cfg-style" />
    </div>
  -->

  <Button
    text="Generate Network"
    class="cfg-style"
    disabled={allowedDateRange == null}
    disabledTooltip="Load GTFS and select date first."
    on:click={generateNetwork}
  />

  <Button
    text="Patronage Data Import"
    class="cfg-style"
    disabled={dataImportDisabled}
    on:click={patronageDataImport}
  />

  <Button
    text="Run Simulation"
    class="cfg-style"
    disabled={runSimulationDisabled}
    disabledTooltip="Network must be generated."
    on:click={runSimulation}
  />

  <Button
    text="Export Results"
    class="cfg-style"
    disabled={exportResultsDisabled}
    disabledTooltip="Run simulation first."
    on:click={() => callBackendWithWaitCursor("export_results")}
  />
</div>

<style>
  #cfg-panel {
    display: flex;
    flex-direction: column;
    justify-content: start;
    align-items: flex-start;
    gap: 20px;
  }

  #cfg-panel :global(.cfg-style) {
    width: 100%;
    padding: 10px;
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

  input {
    background-color: #5e503f;
    color: white;
  }

  input[type="file"] {
    color-scheme: light;
  }

  input:disabled {
    background-color: #423525;
    color: #a28a6f;
  }

  /*
  .network-buttons {
    display: flex;
    width: 100%;
    gap: 10px;
  }
  */
</style>
