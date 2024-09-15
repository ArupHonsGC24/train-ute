<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { PathLayer } from "@deck.gl/layers";
  import { TripsLayer } from "@deck.gl/geo-layers";
  import { MapboxOverlay } from "@deck.gl/mapbox";
  import { Map } from "mapbox-gl";
  import "mapbox-gl/dist/mapbox-gl.css";

  let container: HTMLElement;

  // Loads positions, indices, timestamps, and colours from a buffer for use in a deck.gl TripLayer.
  function createTripData(buffer: ArrayBuffer) {
    const headerView = new Uint32Array(buffer, 0, 8);

    const positionsOffset = headerView[0];
    const positionsLength = headerView[1] / Float32Array.BYTES_PER_ELEMENT;

    const indicesOffset = headerView[2];
    const indicesLength = headerView[3] / Uint32Array.BYTES_PER_ELEMENT;

    const timestampsOffset = headerView[4];
    const timestampsLength = headerView[5] / Float32Array.BYTES_PER_ELEMENT;

    const coloursOffset = headerView[6];
    const coloursLength = headerView[7] / Uint8ClampedArray.BYTES_PER_ELEMENT;

    const positions = new Float32Array(buffer, positionsOffset, positionsLength);
    const indices = new Uint32Array(buffer, indicesOffset, indicesLength);
    const timestamps = new Float32Array(buffer, timestampsOffset, timestampsLength);
    const colours = new Uint8ClampedArray(buffer, coloursOffset, coloursLength);

    return {
      length: indices.length,
      startIndices: indices,
      attributes: {
        getPath: { value: positions, size: 3 },
        getColor: { value: colours, size: 4 },
        getTimestamps: { value: timestamps, size: 1 },
      },
    };
  }

  // Loads positions, indices, and colours from a buffer for use in a deck.gl PathLayer.
  function createPathData(buffer: ArrayBuffer) {
    // Get the locations of the data from the header.
    const headerView = new Uint32Array(buffer, 0, 6);

    const positionsOffset = headerView[0];
    const positionsLength = headerView[1] / Float32Array.BYTES_PER_ELEMENT;

    const indicesOffset = headerView[2];
    const indicesLength = headerView[3] / Uint32Array.BYTES_PER_ELEMENT;

    const coloursOffset = headerView[4];
    const coloursLength = headerView[5] / Uint8ClampedArray.BYTES_PER_ELEMENT;

    // Load data into buffer views.
    const positions = new Float32Array(buffer, positionsOffset, positionsLength);
    const indices = new Uint32Array(buffer, indicesOffset, indicesLength);
    const colours = new Uint8ClampedArray(buffer, coloursOffset, coloursLength);

    return {
      length: indices.length,
      startIndices: indices,
      attributes: {
        getPath: { value: positions, size: 3 },
        getColor: { value: colours, size: 3 },
      },
    };
  }

  let deckOverlay: MapboxOverlay;
  let map: Map;

  // TODO: type these.
  let pathData: any;
  let tripData: any;

  export async function updateData() {
    // Get data from Rust. TODO: test with promise.all.
    let pathDataBuffer = await invoke("get_path_data") as ArrayBuffer;
    pathData = createPathData(pathDataBuffer);
    let tripDataBuffer = await invoke("get_trip_data") as ArrayBuffer;
    tripData = createTripData(tripDataBuffer);
  }

  // ms
  let initialTimestamp: number | null = 0;

  function update(timestamp: number) {
    // Update the time.
    const startTime = 4 * 60 * 60; // 4am.
    const endTime = 28 * 60 * 60; // 28 hours (4am the next day, to account for post-midnight trips).
    let duration = endTime - startTime;

    let currentTime = 0;
    if (initialTimestamp) {
      currentTime = (timestamp - initialTimestamp) % duration + startTime;
    }

    // Create deck.gl layers.
    const tripsLayer = new TripsLayer({
      id: "trips-layer",
      data: tripData,
      currentTime,
      getWidth: 20,
      trailLength: 50,
      widthMinPixels: 4,
      jointRounded: true,
      capRounded: true,
      // pickable: true,
      _pathType: "open",
    });
    const trainLinesLayer = new PathLayer({
      id: "train-lines-layer",
      data: pathData,
      _pathType: "open",
      getWidth: 10,
      widthMinPixels: 2,
      jointRounded: true,
      //extensions: [new Fp64Extension({})],
    });

    // Add layers to deck.gl overlay.
    deckOverlay.setProps({
      layers: [trainLinesLayer, tripsLayer],
    });

    requestAnimationFrame(update);
  }
  requestAnimationFrame(update);

  onMount(() => {
    // Set up MapBox.
    map = new Map({
      container,
      accessToken: "pk.eyJ1IjoiYmxlbmRlcnNsZXV0aCIsImEiOiJjbHdkMGgwM3EwODdsMmpsZW14eG90MXMyIn0.GHeqFyz5Pr451kf38MJEPQ",
      style: "mapbox://styles/blendersleuth/clwwynkq601cm01q13tcn5di7",
      antialias: true,
    });

    deckOverlay = new MapboxOverlay({
      interleaved: true, // Interleaved on chromium has issues.
      layers: [],
    });

    map.once("load", () => {
      map.addControl(deckOverlay);
    });

    initialTimestamp = document.timeline.currentTime as number;

  });

  onDestroy(() => {
    // Clean up function.
    map.remove();
  });
</script>

<div bind:this={container} />

<style>
  div {
    width: 100%;
    height: 100%;
    background-color: #22333b;
    border-radius: 15px;
    border: 1px solid #5e503f;
  }
</style>
