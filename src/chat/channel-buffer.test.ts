import { afterEach, describe, expect, it, vi } from 'vitest';
import { waitForSendBuffer } from './channel-buffer';

class BufferedChannel extends EventTarget {
  readyState: RTCDataChannelState = 'open';
  bufferedAmount = 300 * 1024;
  bufferedAmountLowThreshold = 64 * 1024;
  get channel() { return this as unknown as RTCDataChannel; }
  drain() { this.bufferedAmount = 0; this.dispatchEvent(new Event('bufferedamountlow')); }
}
afterEach(() => { vi.useRealTimers(); });
describe('bounded DataChannel backpressure', () => {
  it('keeps a legitimate fourteen-second drain alive', async () => {
    vi.useFakeTimers();
    const channel = new BufferedChannel(), controller = new AbortController();
    let settled = false;
    const result = waitForSendBuffer(channel.channel, controller.signal).then(() => { settled = true; return 'drained'; }, () => { settled = true; return 'failed'; });
    await vi.advanceTimersByTimeAsync(14_000);
    expect(settled).toBe(false);
    channel.drain();
    expect(await result).toBe('drained');
    expect(vi.getTimerCount()).toBe(0);
  });
  it('rejects an undrained channel at thirty seconds', async () => {
    vi.useFakeTimers();
    const channel = new BufferedChannel(), controller = new AbortController();
    let settled = false;
    const result = waitForSendBuffer(channel.channel, controller.signal).then(() => 'drained', () => { settled = true; return 'failed'; });
    await vi.advanceTimersByTimeAsync(29_999);
    expect(settled).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(await result).toBe('failed');
    expect(vi.getTimerCount()).toBe(0);
  });
  it.each(['abort', 'close'] as const)('cancels immediately on %s and removes all listeners', async action => {
    vi.useFakeTimers();
    const channel = new BufferedChannel(), controller = new AbortController();
    const removeChannel = vi.spyOn(channel, 'removeEventListener');
    const removeSignal = vi.spyOn(controller.signal, 'removeEventListener');
    const result = waitForSendBuffer(channel.channel, controller.signal).then(() => 'drained', () => 'failed');
    await vi.advanceTimersByTimeAsync(100);
    if (action === 'abort') controller.abort();
    else { channel.readyState = 'closed'; channel.dispatchEvent(new Event('close')); }
    expect(await result).toBe('failed');
    channel.drain();
    expect(vi.getTimerCount()).toBe(0);
    expect(removeChannel).toHaveBeenCalledTimes(2);
    expect(removeSignal).toHaveBeenCalledTimes(1);
  });
  it('does not wait or schedule timers for an already drained channel', async () => {
    vi.useFakeTimers();
    const channel = new BufferedChannel(); channel.bufferedAmount = 0;
    await expect(waitForSendBuffer(channel.channel, new AbortController().signal)).resolves.toBeUndefined();
    expect(vi.getTimerCount()).toBe(0);
  });
});
