class Processor extends AudioWorkletProcessor {
  process(inputs: Float32Array[][]) {
    return inputs.length > 0;
  }
}

registerProcessor("processor", Processor);

function unusedWorkletHelper() {
  return 1;
}
