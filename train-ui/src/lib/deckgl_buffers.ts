export type TripData = {
  length: number;
  startIndices: Uint32Array;
  attributes: {
    getPath: { value: Float32Array; size: number };
    getColor: { value: Uint8ClampedArray; size: number };
    getTimestamps: { value: Float32Array; size: number };
  };
};

// Loads positions, indices, timestamps, and colours from a buffer for use in a deck.gl TripLayer.
export function createTripData(buffer: ArrayBuffer): TripData {
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
  const timestamps = new Float32Array(
    buffer,
    timestampsOffset,
    timestampsLength,
  );
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

export type PathData = {
  length: number;
  startIndices: Uint32Array;
  attributes: {
    getPath: { value: Float32Array; size: number };
    getColor: { value: Uint8ClampedArray; size: number };
  };
};

// Loads positions, indices, and colours from a buffer for use in a deck.gl PathLayer.
export function createPathData(buffer: ArrayBuffer): PathData {
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
