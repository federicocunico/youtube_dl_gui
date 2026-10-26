export interface DownloadOptions {
  destination: string;
  audio_only: boolean;
  video_quality: number;
  container: string;
  audio_format: string;
  audio_bitrate: number;
  fragments: number;
}
export interface Settings {
  defaults: DownloadOptions;
  concurrent_jobs: number;
  theme: string;
}
export interface Format {
  format_id: string;
  height?: number;
  vcodec?: string;
  acodec?: string;
  ext?: string;
}
export interface Media {
  id: string;
  url: string;
  title: string;
  channel: string;
  thumbnail?: string;
  duration?: number;
  filename: string;
  formats?: Format[];
  selection?: {
    height?: number;
    container: string;
    vcodec?: string;
    acodec?: string;
    abr?: number;
    estimate?: number;
  };
  error?: string;
  available?: boolean;
  duplicate?: boolean;
  source?: string;
}
export interface Row extends Media {
  key: string;
  selected: boolean;
}
export interface Progress {
  downloaded: number;
  total?: number;
  speed?: number;
  eta?: number;
  detail: string;
}
export interface Job {
  selection?: Media['selection'];
  id: string;
  title: string;
  channel: string;
  thumbnail?: string;
  url: string;
  filename: string;
  state: string;
  options: DownloadOptions;
  progress: Progress;
  output?: string;
  error?: { message: string; code: string };
  position: number;
}
export interface Snapshot {
  jobs: Job[];
  settings: Settings;
  active_engine?: string;
  previous_engine?: string;
  update: Record<string, any>;
  history: Record<string, any>[];
}
export const defaultOptions: DownloadOptions = {
  destination: '',
  audio_only: false,
  video_quality: 0,
  container: 'auto',
  audio_format: 'original',
  audio_bitrate: 192,
  fragments: 4,
};
export const stateNames: Record<string, string> = {
  queued: 'In attesa',
  resolving: 'Analisi',
  downloading: 'Scaricamento',
  paused: 'In pausa',
  muxing: 'Finalizzazione',
  completed: 'Completato',
  failed: 'Non riuscito',
  cancelled: 'Annullato',
};
export const active = (job: Job) =>
  ['resolving', 'downloading', 'muxing'].includes(job.state);
export function bytes(value?: number): string {
  if (value == null || !Number.isFinite(value)) return '—';
  if (value < 1024) return `${Math.round(value)} B`;
  const i = Math.min(3, Math.floor(Math.log(value) / Math.log(1024)));
  return `${(value / 1024 ** i).toFixed(1)} ${['B', 'KB', 'MB', 'GB'][i]}`;
}
export function duration(value?: number): string {
  if (value == null || !Number.isFinite(value)) return '—';
  const s = Math.max(0, Math.round(value));
  return s >= 3600
    ? `${Math.floor(s / 3600)}:${String(Math.floor(s / 60) % 60).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`
    : `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}
export function percent(job: Job): number {
  return job.state === 'completed'
    ? 100
    : job.progress.total
      ? Math.min(100, (100 * job.progress.downloaded) / job.progress.total)
      : 0;
}
