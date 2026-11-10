/** TURN and SCTP recovery can take longer than five seconds on a rate-limited route. */
export function waitForSendBuffer(channel: RTCDataChannel, signal: AbortSignal): Promise<void> {
  if (signal.aborted || channel.readyState !== 'open') return Promise.reject(new Error('Channel unavailable'));
  if (channel.bufferedAmount <= 256 * 1024) return Promise.resolve();
  return new Promise<void>((resolve, reject) => {
    const done = () => { cleanup(); resolve(); };
    const failed = () => { cleanup(); reject(new Error('Channel unavailable')); };
    const timer = setTimeout(failed, 30_000);
    const cleanup = () => {
      clearTimeout(timer);
      channel.removeEventListener('bufferedamountlow', done);
      channel.removeEventListener('close', failed);
      signal.removeEventListener('abort', failed);
    };
    channel.addEventListener('bufferedamountlow', done, { once: true });
    channel.addEventListener('close', failed, { once: true });
    signal.addEventListener('abort', failed, { once: true });
    if (signal.aborted || channel.readyState !== 'open') failed();
    else if (channel.bufferedAmount <= channel.bufferedAmountLowThreshold) done();
  });
}
