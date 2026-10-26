import { describe, it, expect } from 'vitest';
import { bytes, duration, percent, defaultOptions } from './types';
describe('progress with unknown or partial data', () => {
  it('marks unknown totals and time instead of inventing values', () => {
    expect(bytes(undefined)).toBe('—');
    expect(duration(undefined)).toBe('—');
    expect(duration(65)).toBe('1:05');
  });
  it('clamps estimated progress and preserves completed state', () => {
    const job: any = {
      state: 'downloading',
      progress: { downloaded: 20, total: 10 },
    };
    expect(percent(job)).toBe(100);
    job.progress.total = undefined;
    expect(percent(job)).toBe(0);
    job.state = 'completed';
    expect(percent(job)).toBe(100);
  });
  it('uses separate media defaults', () => {
    expect(defaultOptions.audio_only).toBe(false);
    expect(defaultOptions.fragments).toBe(4);
  });
});
