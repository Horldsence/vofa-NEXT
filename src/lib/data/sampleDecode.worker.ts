/// <reference lib="webworker" />

import { decodeSampleEnvelope } from './sampleProtocol';

interface DecodeRequest {
  key: string;
  generation: number;
  receivedAt: number;
  buffer: ArrayBuffer;
}

self.onmessage = (event: MessageEvent<DecodeRequest>) => {
  try {
    const batch = decodeSampleEnvelope(event.data.buffer);
    self.postMessage({
      key: event.data.key,
      generation: event.data.generation,
      receivedAt: event.data.receivedAt,
      batch,
    });
  } catch (error) {
    self.postMessage({
      key: event.data.key,
      generation: event.data.generation,
      receivedAt: event.data.receivedAt,
      error: error instanceof Error ? error.message : String(error),
    });
  }
};
