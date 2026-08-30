/// <reference lib="webworker" />

import { decodeSampleEnvelope } from './sampleProtocol';

interface DecodeRequest {
  key: string;
  buffer: ArrayBuffer;
}

self.onmessage = (event: MessageEvent<DecodeRequest>) => {
  try {
    const batch = decodeSampleEnvelope(event.data.buffer);
    self.postMessage({ key: event.data.key, batch });
  } catch (error) {
    self.postMessage({
      key: event.data.key,
      error: error instanceof Error ? error.message : String(error),
    });
  }
};

