/* global AudioWorkletProcessor, registerProcessor */
class VoiceCaptureProcessor extends AudioWorkletProcessor {
  process(inputs, outputs) {
    const input = inputs[0]?.[0];
    if (input?.length) { const copy = input.slice(); this.port.postMessage(copy, [copy.buffer]); }
    for (const output of outputs) for (const channel of output) channel.fill(0);
    return true;
  }
}
registerProcessor('ssc-voice-capture', VoiceCaptureProcessor);
