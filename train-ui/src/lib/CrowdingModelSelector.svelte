<script context="module" lang="ts">
  // TS equivalent of the Rust types.
  export type CrowdingFunc =
    { func: "linear" } |
    { func: "quadratic" } |
    { func: "oneStep", params: { a0: number, a: number, b: number } } |
    { func: "twoStep", params: { a0: number, a1: number, a: number, b: number, c: number } };
  export type CrowdingFuncType = CrowdingFunc["func"];

  export type TripCapacity = {
    seated: number;
    standing: number;
  };
</script>

<script lang="ts">
  import Button from "$lib/Button.svelte";
  import { callBackend, callBackendWithWaitCursor } from "$lib/utilities";

  let crowdingFuncType: CrowdingFuncType = "twoStep";
  export let defaultTripCapacity: TripCapacity = {
    seated: 528,
    standing: 266,
  };
  let a0 = 0.25;
  let a1 = 0.5;
  let a = 5;
  let b = 0.5;
  let c = 0.02;
  export let crowdingFunc: CrowdingFunc = { func: crowdingFuncType, params: { a0, a1, a, b, c } };
  export let costUtility = 0.5;

  $: {
    a = crowdingFuncType === "oneStep" ? Math.max(a, 5) : a;
  }
  $: a_min = crowdingFuncType === "oneStep" ? 5 : 0;

  $: {
    let coeff_min = 0.0001;

    function f(x: number) {
      return Math.max(coeff_min, x);
    }

    if (crowdingFuncType === "oneStep") {
      crowdingFunc = { func: "oneStep", params: { a0: f(a0), a: f(a), b } };
    } else if (crowdingFuncType === "twoStep") {
      crowdingFunc = { func: "twoStep", params: { a0, a1, a, b, c } };
    } else {
      crowdingFunc = { func: crowdingFuncType };
    }
  }

  async function exportModelCSV() {
    await callBackend("export_model_csv", { crowdingFunc, defaultTripCapacity });
  }

  let tripCapacitiesValid = false;

  async function importTripCapacities() {
    await callBackendWithWaitCursor("import_trip_capacities");
    tripCapacitiesValid = true;
  }

</script>

<div class="container">
  <label for="crowding-model" class="title">Crowding Model:</label>
  <div class="params">
    <div class="param">
      <label for="crowding-func" class="cfg-label">Function:</label>
      <select
        id="crowding-func"
        bind:value={crowdingFuncType}
      >
        <option value="linear">Linear</option>
        <option value="quadratic">Quadratic</option>
        <option value="oneStep">One-step</option>
        <option value="twoStep">Two-step</option>
      </select>
    </div>
    <div class="cap-params">
      <div class="cap-params-vert">
        <div class="param">
          <label for="S" class="cfg-label">Default Seated</label>
          <input type="number" id="S" min="1" step="1" bind:value={defaultTripCapacity.seated}>
        </div>
        <div class="param">
          <label for="T" class="cfg-label">Default Standing</label>
          <input type="number" id="T" min="1" step="1" bind:value={defaultTripCapacity.standing}
                 disabled={crowdingFuncType === "oneStep"}>
        </div>
        <Button
          text="Import trip capacities"
          class="cfg-style"
          defaultTooltip="Import override trip capacities"
          processIndicator={true}
          processComplete={tripCapacitiesValid}
          on:click={importTripCapacities}
        />
        <div class="param">
          <label for="costUtility" class="cfg-label">Cost Utility</label>
          <input type="number" id="costUtility" min="0" step="0.1"
                 title="Used by journey utility calculation: journey_time + cost_utility * crowding cost"
                 bind:value={costUtility}>
        </div>
      </div>
      <div class="cap-params-vert">
        <div class="param">
          <label for="a0" class="cfg-label">a0:</label>
          <input type="number" id="a0" min="0" step="any" bind:value={a0} title="Seated cost"
                 disabled={crowdingFuncType !== "oneStep" && crowdingFuncType !== "twoStep"}>
        </div>
        <div class="param">
          <label for="a1" class="cfg-label">a1:</label>
          <input type="number" id="a1" min="0" step="any" bind:value={a1} title="Standing cost"
                 disabled={crowdingFuncType !== "twoStep"}>
        </div>
      </div>
      <div class="cap-params-vert">
        <div class="param">
          <label for="a" class="cfg-label">a:</label>
          <input type="number" id="a" min={a_min} step="0.1" bind:value={a}
                 disabled={crowdingFuncType !== "oneStep" && crowdingFuncType !== "twoStep"}>
        </div>
        <div class="param">
          <label for="b" class="cfg-label">b:</label>
          <input type="number" id="b" min="0" step="0.1" bind:value={b}
                 disabled={crowdingFuncType !== "oneStep" && crowdingFuncType !== "twoStep"}>
        </div>
        <div class="param">
          <label for="c" class="cfg-label">c:</label>
          <input type="number" id="c" min="0" step="0.001" bind:value={c} disabled={crowdingFuncType !== "twoStep"}>
        </div>
      </div>
    </div>
    <Button
      text="Export function to CSV"
      class="cfg-style"
      defaultTooltip="Export the crowding function to a CSV file for reference."
      on:click={exportModelCSV}
    />
  </div>
</div>

<style>
  .container {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .title {
    font-size: 1.2rem;
    color: white;
    flex: 1;
  }

  .params {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .cap-params {
    display: flex;
    gap: 10px;
  }

  .cap-params-vert {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .param {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  select {
    flex: 2;
  }

  .params :global(.cfg-style) {
    /* TODO: This is a hack until I can sort out the styles properly. */
    padding: 4px !important;
  }

  .cfg-label {
    font-size: 0.8rem;
    flex: 1;
  }

  input {
    flex: 1;
    width: 20%;
  }

</style>