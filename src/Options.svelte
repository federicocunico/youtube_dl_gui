<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { FolderOpen } from '@lucide/svelte';
  import type { DownloadOptions } from './types';
  export let value: DownloadOptions;
  export let onchange: (value: DownloadOptions) => void;
  export let onerror: (message: string) => void;
  export let id = 'options';
  const update = (
    key: keyof DownloadOptions,
    next: string | number | boolean,
  ) => onchange({ ...value, [key]: next });
  async function folder() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Scegli la cartella di download',
        defaultPath: value.destination || undefined,
      });
      if (typeof selected === 'string') update('destination', selected);
    } catch (e) {
      onerror(String(e));
    }
  }
</script>

<div class="options-form">
  <label class="toggle-row" for={`${id}-audio`}
    ><span
      ><strong>Solo audio</strong><small>Salva la traccia, senza video</small
      ></span
    ><input
      id={`${id}-audio`}
      type="checkbox"
      role="switch"
      checked={value.audio_only}
      onchange={(e) => update('audio_only', e.currentTarget.checked)}
    /></label
  >
  <div class="field-grid">
    <label for={`${id}-video`}
      >Qualità video<select
        id={`${id}-video`}
        disabled={value.audio_only}
        value={value.video_quality}
        onchange={(e) => update('video_quality', Number(e.currentTarget.value))}
        ><option value={0}>Migliore disponibile</option
        >{#each [144, 240, 360, 480, 720, 1080, 1440, 2160, 4320] as quality}<option
            value={quality}>Fino a {quality}p</option
          >{/each}</select
      ></label
    >
    <label for={`${id}-container`}
      >Contenitore<select
        id={`${id}-container`}
        disabled={value.audio_only}
        value={value.container}
        onchange={(e) => update('container', e.currentTarget.value)}
        ><option value="auto">Automatico</option><option value="mp4">MP4</option
        ><option value="mkv">MKV</option><option value="webm">WebM</option
        ></select
      ></label
    >
    <label for={`${id}-format`}
      >Formato audio<select
        id={`${id}-format`}
        disabled={!value.audio_only}
        value={value.audio_format}
        onchange={(e) => update('audio_format', e.currentTarget.value)}
        ><option value="original">Originale</option><option value="mp3"
          >MP3</option
        ><option value="aac">AAC</option></select
      ></label
    >
    <label for={`${id}-bitrate`}
      >Bitrate audio<select
        id={`${id}-bitrate`}
        disabled={!value.audio_only || value.audio_format === 'original'}
        value={value.audio_bitrate}
        onchange={(e) => update('audio_bitrate', Number(e.currentTarget.value))}
        >{#each [128, 192, 256, 320] as bitrate}<option value={bitrate}
            >{bitrate} kbps</option
          >{/each}</select
      ></label
    >
  </div>
  <p class="field-help">
    {value.audio_only
      ? value.audio_format === 'original'
        ? 'Audio originale, senza ricodifica.'
        : 'L’audio verrà convertito. Il bitrate non aumenta la qualità della sorgente.'
      : 'Video e audio uniti in un unico file, senza ricodifica video.'}
  </p>
  <label for={`${id}-folder`}
    >Cartella di download
    <div class="folder-input">
      <input
        id={`${id}-folder`}
        value={value.destination}
        placeholder="Scegli una cartella…"
        oninput={(e) => update('destination', e.currentTarget.value)}
      /><button
        class="icon-button"
        type="button"
        title="Scegli cartella"
        aria-label="Scegli cartella"
        onclick={folder}><FolderOpen size={18} /></button
      >
    </div></label
  >
</div>
