<script lang="ts">
  import { version as appVersion } from '../package.json';
  import { onMount } from 'svelte';
  import { invoke, isTauri } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { confirm } from '@tauri-apps/plugin-dialog';
  import {
    Frame,
    Plus,
    ListVideo,
    Settings2,
    ArrowDownToLine,
    ArrowUpRight,
    FolderOpen,
    Link,
    Clipboard,
    Play,
    Pause,
    X,
    Check,
    RefreshCw,
    Search,
    Sun,
    Moon,
    ChevronUp,
    ChevronDown,
    Film,
    LoaderCircle,
    CircleCheck,
    ShieldCheck,
    HardDrive,
  } from '@lucide/svelte';
  import Options from './Options.svelte';
  import Thumb from './Thumb.svelte';
  import {
    defaultOptions,
    stateNames,
    active,
    bytes,
    duration,
    percent,
  } from './types';
  import type {
    DownloadOptions,
    Row,
    Snapshot,
    Settings,
    Job,
    Media,
  } from './types';
  import './style.css';
  let view: 'add' | 'queue' | 'settings' = 'add';
  let snapshot: Snapshot = {
    jobs: [],
    settings: {
      defaults: { ...defaultOptions },
      concurrent_jobs: 3,
      theme: 'dark',
    },
    update: { phase: 'idle' },
    history: [],
  };
  let options = { ...defaultOptions };
  let settings: Settings = structuredClone(snapshot.settings);
  let overrides: Record<string, DownloadOptions> = {};
  let suggestedOutput = '';
  let refreshTicket = 0;
  let previewTimer: ReturnType<typeof setTimeout> | undefined;
  let perItem = false,
    initialized = false,
    input = '',
    rows: Row[] = [],
    focused = '',
    busy = false,
    inspecting = false,
    enqueueing = false,
    session = '',
    message = '',
    toast = '',
    search = '',
    filter = 'all',
    selectedJobs: string[] = [],
    online = true,
    native = false;
  $: focusedRow = rows.find((r) => r.key === focused);
  $: selected = rows.filter((r) => r.selected && r.available !== false);
  $: editingOptions =
    (perItem && focusedRow && overrides[focusedRow.key]) || options;
  $: activeJobs = snapshot.jobs.filter(active);
  $: finished = snapshot.jobs.filter((j) => j.state === 'completed');
  $: speed = activeJobs.reduce((sum, j) => sum + (j.progress.speed || 0), 0);
  $: transferred = snapshot.jobs.reduce(
    (sum, j) => sum + j.progress.downloaded,
    0,
  );
  $: visibleJobs = snapshot.jobs.filter(
    (j) =>
      (filter === 'all' ||
        (filter === 'active'
          ? active(j) || j.state === 'queued'
          : j.state === filter)) &&
      `${j.title} ${j.channel}`
        .toLocaleLowerCase()
        .includes(search.toLocaleLowerCase()),
  );
  $: aggregateTotal =
    snapshot.jobs.length && snapshot.jobs.every((j) => j.progress.total != null)
      ? snapshot.jobs.reduce((sum, j) => sum + (j.progress.total || 0), 0)
      : undefined;
  $: aggregateEta =
    activeJobs.length &&
    activeJobs.every((j) => j.progress.total != null) &&
    speed > 0
      ? activeJobs.reduce(
          (sum, j) =>
            sum + Math.max(0, (j.progress.total || 0) - j.progress.downloaded),
          0,
        ) / speed
      : undefined;
  $: theme =
    settings.theme === 'system'
      ? window.matchMedia('(prefers-color-scheme: light)').matches
        ? 'light'
        : 'dark'
      : settings.theme;
  $: document.documentElement.dataset.theme = theme;
  function error(e: unknown) {
    message = String(e);
  }
  const call = <T,>(command: string, args: Record<string, unknown> = {}) =>
    invoke<T>(command, args);
  function applySnapshot(value: Snapshot) {
    snapshot = value;
    if (!initialized) {
      settings = structuredClone(value.settings);
      options = { ...value.settings.defaults };
      initialized = true;
    }
  }
  onMount(() => {
    native = isTauri();
    online = navigator.onLine;
    const updateOnline = () => (online = navigator.onLine);
    window.addEventListener('online', updateOnline);
    window.addEventListener('offline', updateOnline);
    const cleanup: (() => void)[] = [];
    let disposed = false;
    const register = async <T,>(
      name: string,
      handler: (payload: T) => void,
    ) => {
      const stop = await listen<T>(name, (e) => handler(e.payload));
      if (disposed) stop();
      else cleanup.push(stop);
    };
    if (native)
      void (async () => {
        await register<Snapshot>('snapshot', applySnapshot);
        await register<{ session: string; item?: Media; done?: boolean }>(
          'discovery',
          (event) => {
            if (event.session !== session) return;
            if (event.done) {
              busy = false;
              return;
            }
            if (event.item) {
              const item = {
                ...event.item,
                key: crypto.randomUUID(),
                selected:
                  event.item.available !== false && !event.item.duplicate,
              };
              rows = [...rows, item];
              if (!focused && item.available !== false) {
                focused = item.key;
                if (!item.formats) void refresh(item);
                else void outputPreview(item);
              }
            }
          },
        );
        await register<string>('app-error', error);
        await register('close-request', async () => {
          if (
            await confirm(
              'I download verranno interrotti e potranno essere ripresi al prossimo avvio. Chiudere YT Saver?',
              {
                title: 'Download in corso',
                kind: 'warning',
                okLabel: 'Annulla e chiudi',
                cancelLabel: 'Resta aperto',
              },
            )
          )
            await call('shutdown');
        });
        applySnapshot(await call<Snapshot>('snapshot'));
      })().catch(error);
    return () => {
      clearTimeout(previewTimer);
      disposed = true;
      cleanup.forEach((stop) => stop());
      window.removeEventListener('online', updateOnline);
      window.removeEventListener('offline', updateOnline);
    };
  });
  async function analyze() {
    if (!input.trim() || busy) return;
    if (!native) {
      error('Avvia l’app desktop YT Saver per analizzare e scaricare i video.');
      return;
    }
    message = '';
    rows = [];
    focused = '';
    overrides = {};
    session = crypto.randomUUID();
    busy = true;
    try {
      await call('discover', { id: session, input, options });
    } catch (e) {
      busy = false;
      error(e);
    }
  }
  async function paste() {
    try {
      input = await navigator.clipboard.readText();
    } catch {
      error('Incolla il link con Ctrl+V nel campo URL.');
    }
  }
  async function focus(row: Row) {
    focused = row.key;
    perItem = !!overrides[row.key];
    if (row.available !== false && !row.formats) await refresh(row);
  }
  async function refresh(row = focusedRow) {
    if (!row) return;
    const ticket = ++refreshTicket;
    inspecting = true;
    message = '';
    try {
      const detail = await call<Media>('inspect_media', {
        url: row.url,
        options: overrides[row.key] || options,
      });
      if (ticket === refreshTicket) {
        rows = rows.map((r) =>
          r.key === row.key
            ? { ...r, ...detail, selected: r.selected, key: r.key }
            : r,
        );
        await outputPreview({ ...row, ...detail });
      }
    } catch (e) {
      error(e);
    } finally {
      if (ticket === refreshTicket) inspecting = false;
    }
  }
  async function outputPreview(row = focusedRow) {
    if (!row || !native) return;
    try {
      const result = await call<{ filename: string }>('preview_output', {
        destination: (overrides[row.key] || options).destination,
        filename: row.filename || row.title,
        extension: row.selection?.container || 'mkv',
      });
      if (focused === row.key) suggestedOutput = result.filename;
    } catch {
      suggestedOutput = 'Verifica la cartella di destinazione';
    }
  }
  function updateOptions(value: DownloadOptions) {
    const old = editingOptions;
    const mediaChanged = [
      'audio_only',
      'video_quality',
      'container',
      'audio_format',
      'audio_bitrate',
    ].some(
      (key) =>
        old[key as keyof DownloadOptions] !==
        value[key as keyof DownloadOptions],
    );
    if (perItem && focusedRow)
      overrides = { ...overrides, [focusedRow.key]: value };
    else options = value;
    clearTimeout(previewTimer);
    previewTimer = setTimeout(() => {
      if (mediaChanged) void refresh();
      else void outputPreview();
    }, 300);
  }
  function toggleRow(key: string, checked: boolean) {
    rows = rows.map((r) => (r.key === key ? { ...r, selected: checked } : r));
  }
  async function addSelected() {
    enqueueing = true;
    message = '';
    try {
      await call('enqueue', {
        items: selected.map((row) => ({
          media: { ...row, filename: row.filename || row.title },
          options: overrides[row.key] || options,
        })),
      });
      rows = rows.map((r) => (r.selected ? { ...r, selected: false } : r));
      toast = 'Aggiunto alla coda';
      view = 'queue';
    } catch (e) {
      error(e);
    } finally {
      enqueueing = false;
    }
  }
  async function action(job: Job, action: string) {
    try {
      await call('control_job', { id: job.id, action });
    } catch (e) {
      error(e);
    }
  }
  async function batch(actionName: string) {
    toast = 'Azione inviata';
    for (const job of snapshot.jobs.filter((j) =>
      selectedJobs.includes(j.id),
    )) {
      if (
        (actionName === 'pause' && (active(job) || job.state === 'queued')) ||
        (actionName === 'resume' && job.state === 'paused') ||
        (actionName === 'cancel' &&
          !['completed', 'cancelled', 'failed'].includes(job.state))
      )
        await action(job, actionName);
    }
  }
  async function reorder(job: Job, delta: number) {
    const waiting = snapshot.jobs.filter((j) => j.state === 'queued');
    const index = waiting.findIndex((j) => j.id === job.id),
      next = index + delta;
    if (next < 0 || next >= waiting.length) return;
    [waiting[index], waiting[next]] = [waiting[next], waiting[index]];
    try {
      await call('reorder_jobs', { ids: waiting.map((j) => j.id) });
    } catch (e) {
      error(e);
    }
  }
  async function save() {
    message = '';
    try {
      await call('save_settings', { settings });
      options = { ...settings.defaults };
      toast = 'Impostazioni salvate';
    } catch (e) {
      error(e);
    }
  }
  async function checkUpdate() {
    message = '';
    try {
      await call('check_update');
    } catch (e) {
      error(e);
    }
  }
  async function openOutput(job: Job, folder: boolean) {
    try {
      await call('open_output', { id: job.id, folder });
    } catch (e) {
      error(e);
    }
  }
  async function preview() {
    if (focusedRow)
      try {
        await call('preview', { id: focusedRow.id });
      } catch (e) {
        error(e);
      }
  }
  async function youtube() {
    if (focusedRow)
      try {
        await call('open_youtube', { url: focusedRow.url });
      } catch (e) {
        error(e);
      }
  }
  const updateLabels: Record<string, string> = {
    idle: 'Pronto',
    checking: 'Ricerca aggiornamenti',
    current: 'Motore aggiornato',
    installing: 'Installazione separata in corso',
    testing: 'Download di prova in corso',
    activated: 'Aggiornamento verificato',
    inconclusive: 'Test da riprovare',
    rejected: 'Versione precedente conservata',
    quarantined: 'Versione non attivata',
    rollback: 'Versione precedente ripristinata',
    error: 'Aggiornamento non riuscito',
  };
</script>

<svelte:window
  onkeydown={(e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && view === 'add') {
      e.preventDefault();
      void analyze();
    }
  }}
/>
<div class="app-shell">
  <aside class="sidebar">
    <a
      href="#add"
      class="brand"
      onclick={() => (view = 'add')}
      aria-label="YT Saver, nuovo download"
      ><Frame size={29} strokeWidth={1.8} /><span
        >YT Saver<span class="brand-dot">.</span></span
      ></a
    >
    <div class="nav-caption">IL TUO SPAZIO</div>
    <nav aria-label="Navigazione principale">
      <button
        class:selected={view === 'add'}
        onclick={() => {
          view = 'add';
          message = '';
        }}><Plus size={19} /><span>Nuovo download</span></button
      ><button
        class:selected={view === 'queue'}
        onclick={() => {
          view = 'queue';
          message = '';
        }}
        ><ListVideo size={19} /><span>Coda download</span
        >{#if snapshot.jobs.length}<span class="nav-badge"
            >{snapshot.jobs.length}</span
          >{/if}</button
      ><button
        class:selected={view === 'settings'}
        onclick={() => {
          view = 'settings';
          message = '';
        }}><Settings2 size={19} /><span>Impostazioni</span></button
      >
    </nav>
    <div class="sidebar-note">
      <HardDrive size={21} /><strong>Direttamente sul tuo PC</strong>
      <p>Video, audio e cronologia.<br />Il tuo archivio resta qui.</p>
    </div>
    <div class="sidebar-bottom">
      <div>
        <span class="status-dot"></span>Motore locale<span class="version"
          >v{appVersion}</span
        >
      </div>
      <button
        class="theme-button"
        onclick={() =>
          (settings = {
            ...settings,
            theme: theme === 'dark' ? 'light' : 'dark',
          })}
        aria-label={theme === 'dark'
          ? 'Attiva tema chiaro'
          : 'Attiva tema scuro'}
        >{#if theme === 'dark'}<Sun size={16} />{:else}<Moon
            size={16}
          />{/if}<span>{theme === 'dark' ? 'Tema chiaro' : 'Tema scuro'}</span
        ></button
      >
    </div>
  </aside>
  <div class="workspace">
    <header class="topbar">
      <div class="breadcrumb">
        WORKSPACE<span>/</span><strong
          >{view === 'add'
            ? 'NUOVO DOWNLOAD'
            : view === 'queue'
              ? 'CODA DOWNLOAD'
              : 'IMPOSTAZIONI'}</strong
        >
      </div>
      <div class="connection">
        <span class="status-dot" class:offline={!online}></span>{online
          ? 'Connessione disponibile'
          : 'Offline'}
      </div>
    </header>
    <main>
      {#if !native}<div class="notice">
          Anteprima dell’interfaccia. Per scaricare i media, avvia l’app desktop
          YT Saver.
        </div>{/if}
      {#if message}<div class="alert" role="alert">
          <span>{message}</span><button
            class="icon-button"
            aria-label="Chiudi messaggio"
            onclick={() => (message = '')}><X size={17} /></button
          >
        </div>{/if}
      {#if toast}<div class="toast" role="status">
          <Check size={16} />{toast}<button
            class="icon-button"
            aria-label="Chiudi notifica"
            onclick={() => (toast = '')}><X size={15} /></button
          >
        </div>{/if}
      {#if view === 'add'}
        <section class="page-heading">
          <div class="eyebrow">UN LINK. TUTTO QUELLO CHE TI SERVE.</div>
          <h1>Da YouTube.<br /><span>Al tuo spazio.</span></h1>
          <p>
            Il video che vuoi tenere, la musica che vuoi portare con te.<br
            />Incolla un link e scegli come salvarlo.
          </p>
        </section>
        <div class="add-layout">
          <div class="media-column">
            <form
              class="panel input-panel"
              onsubmit={(e) => {
                e.preventDefault();
                void analyze();
              }}
            >
              <div class="panel-title">
                <span class="step">01</span>
                <h2>Aggiungi i tuoi link</h2>
                <span class="subtle">VIDEO · SHORTS · PLAYLIST</span>
              </div>
              <label for="urls" class="sr-only">URL YouTube, uno per riga</label
              >
              <div class="url-box">
                <Link size={19} /><textarea
                  id="urls"
                  bind:value={input}
                  placeholder="Incolla un URL YouTube…&#10;Puoi aggiungerne più di uno, su righe separate."
                  rows={3}></textarea><button
                  type="button"
                  class="icon-button"
                  aria-label="Incolla dagli appunti"
                  onclick={paste}><Clipboard size={18} /></button
                >
              </div>
              <div class="input-actions">
                <span class="field-help"
                  >Un video o un’intera selezione. Decidi tu.</span
                >{#if busy}<button
                    type="button"
                    class="button secondary"
                    onclick={() =>
                      call('cancel_discovery', { id: session }).catch(error)}
                    ><X size={16} />Ferma analisi</button
                  >{:else}<button
                    class="button primary"
                    type="submit"
                    disabled={!input.trim()}
                    ><Search size={17} />Analizza link<span
                      class="keyboard-hint">Ctrl ↵</span
                    ></button
                  >{/if}
              </div>
            </form>
            {#if rows.length || busy}<section class="panel results-panel">
                <div class="panel-title">
                  <span class="step">02</span>
                  <h2>La tua selezione</h2>
                  <span class="count">{rows.length}</span
                  >{#if busy}<LoaderCircle size={16} class="spin" />{/if}
                </div>
                <div class="selection-bar">
                  <label
                    ><input
                      type="checkbox"
                      checked={rows.some((r) => r.available !== false) &&
                        rows
                          .filter((r) => r.available !== false)
                          .every((r) => r.selected)}
                      onchange={(e) => {
                        const checked = e.currentTarget.checked;
                        rows = rows.map((r) => ({
                          ...r,
                          selected: r.available !== false && checked,
                        }));
                      }}
                    />Seleziona tutti</label
                  ><span>{selected.length} selezionati</span>
                </div>
                <div class="result-list">
                  {#each rows as row (row.key)}<div
                      class="result-row"
                      class:focused={focused === row.key}
                    >
                      <input
                        type="checkbox"
                        aria-label={`Seleziona ${row.title || row.source || 'elemento'}`}
                        disabled={row.available === false}
                        checked={row.selected}
                        onchange={(e) =>
                          toggleRow(row.key, e.currentTarget.checked)}
                      /><button
                        class="result-content"
                        onclick={() => focus(row)}
                        disabled={row.available === false}
                        ><div class="result-thumb">
                          <Thumb url={row.thumbnail} />
                        </div>
                        <span
                          ><strong
                            >{row.title ||
                              row.source ||
                              'Elemento non disponibile'}</strong
                          ><small
                            >{row.error ||
                              row.channel ||
                              'Seleziona per i dettagli'}</small
                          >{#if row.duplicate}<em
                              >Duplicato · seleziona per aggiungerlo di nuovo</em
                            >{/if}</span
                        ><span class="duration">{duration(row.duration)}</span
                        ></button
                      >
                    </div>{/each}
                </div>
              </section>
              {#if focusedRow}<section class="panel preview-panel">
                  <div class="panel-title">
                    <Film size={17} />
                    <h2>Anteprima</h2>
                    <button class="text-button" onclick={youtube}
                      >Apri su YouTube<ArrowUpRight size={15} /></button
                    >
                  </div>
                  <Thumb url={focusedRow.thumbnail} large
                    ><button
                      class="play-button"
                      onclick={preview}
                      aria-label="Riproduci anteprima"
                      ><Play size={22} fill="currentColor" /></button
                    ></Thumb
                  >
                  <h3>{focusedRow.title}</h3>
                  <p class="muted">
                    {focusedRow.channel} · {duration(focusedRow.duration)}
                  </p>
                  <p class="field-help">
                    Se il player non è disponibile, usa “Apri su YouTube”.
                  </p>
                </section>{/if}
            {:else}<div class="empty-preview">
                <div class="empty-icon">
                  <Film size={31} strokeWidth={1.3} />
                </div>
                <h3>Il prossimo download parte da qui.</h3>
                <p>
                  Analizza un link per vedere titolo, canale e anteprima,<br
                  />prima di aggiungerlo alla coda.
                </p>
                <div class="feature-line">
                  <span><Check size={13} />Video + audio</span><span
                    ><Check size={13} />Download in parallelo</span
                  ><span><Check size={13} />Nessun account</span>
                </div>
              </div>{/if}
          </div>
          <aside class="panel output-panel">
            <div class="panel-title">
              <ArrowDownToLine size={19} />
              <h2>Come lo vuoi salvare?</h2>
            </div>
            <div class="output-intro">Le tue preferenze, il tuo file.</div>
            <div class="output-scroll">
              {#if focusedRow}<label class="scope-toggle"
                  ><input
                    type="checkbox"
                    bind:checked={perItem}
                    onchange={(e) => {
                      if (!e.currentTarget.checked && focusedRow) {
                        const next = { ...overrides };
                        delete next[focusedRow.key];
                        overrides = next;
                      }
                    }}
                  />Personalizza questo elemento</label
                >{/if}<Options
                value={editingOptions}
                onchange={updateOptions}
                onerror={error}
                id="download"
              />
              {#if focusedRow}<label for="filename"
                  >Nome del file<input
                    id="filename"
                    onblur={() => outputPreview()}
                    value={focusedRow.filename}
                    oninput={(e) => {
                      const name = e.currentTarget.value;
                      rows = rows.map((r) =>
                        r.key === focused ? { ...r, filename: name } : r,
                      );
                    }}
                  /></label
                >
                <p class="field-help">
                  Nome finale suggerito: {suggestedOutput ||
                    focusedRow.filename}
                </p>
                <div class="format-preview">
                  <span>Formato rilevato</span><strong
                    >{focusedRow.selection
                      ? `${focusedRow.selection.height ? focusedRow.selection.height + 'p · ' : ''}${focusedRow.selection.container?.toUpperCase()} · ${focusedRow.selection.acodec || 'audio'}`
                      : 'Da analizzare'}</strong
                  ><button
                    class="text-button"
                    disabled={inspecting}
                    onclick={() => refresh()}
                    ><RefreshCw
                      size={13}
                      class={inspecting ? 'spin' : ''}
                    />Aggiorna formati</button
                  >
                </div>{/if}
            </div>
            <button
              class="button primary download-button"
              disabled={!selected.length || enqueueing}
              onclick={addSelected}
              ><ArrowDownToLine size={18} />Aggiungi alla coda{#if selected.length}<span
                  >{selected.length}</span
                >{/if}</button
            >
            <p class="footnote">
              <ShieldCheck size={13} />L’originale resta sempre su YouTube.
            </p>
          </aside>
        </div>
      {:else if view === 'queue'}
        <section class="page-heading compact">
          <div class="eyebrow">TUTTO SOTTO CONTROLLO</div>
          <h1>La tua coda<span>.</span></h1>
          <p>Più download insieme. Un posto per gestirli tutti.</p>
        </section>
        <div class="stats-grid">
          <div class="stat panel">
            <span>IN CORSO</span><strong
              >{activeJobs.length}<small
                >/ {snapshot.settings.concurrent_jobs}</small
              ></strong
            >
          </div>
          <div class="stat panel">
            <span>VELOCITÀ TOTALE</span><strong
              >{speed ? bytes(speed) : '—'}<small>{speed ? '/s' : ''}</small
              ></strong
            >
            <small class="field-help"
              >Tempo stimato: {duration(aggregateEta)}</small
            >
          </div>
          <div class="stat panel">
            <span>DATI SCARICATI</span><strong>{bytes(transferred)}</strong
            ><small class="field-help"
              >Totale: {aggregateTotal == null
                ? '—'
                : `~${bytes(aggregateTotal)}`}</small
            >
          </div>
          <div class="stat panel">
            <span>COMPLETATI</span><strong
              >{finished.length}<CircleCheck size={19} /></strong
            >
          </div>
        </div>
        <section class="panel queue-panel">
          <div class="queue-toolbar">
            <div class="tabs" role="group" aria-label="Filtra download">
              {#each [['all', 'Tutti'], ['active', 'In corso'], ['completed', 'Completati'], ['failed', 'Non riusciti']] as [value, label]}<button
                  class:selected={filter === value}
                  onclick={() => (filter = value)}>{label}</button
                >{/each}
            </div>
            <label class="search-input"
              ><Search size={16} /><input
                placeholder="Cerca nella coda"
                aria-label="Cerca nella coda"
                bind:value={search}
              /></label
            >
          </div>
          {#if selectedJobs.length}<div class="batch-bar">
              <span>{selectedJobs.length} selezionati</span><button
                class="text-button"
                onclick={() => batch('pause')}><Pause size={14} />Pausa</button
              ><button class="text-button" onclick={() => batch('resume')}
                ><Play size={14} />Riprendi</button
              ><button class="text-button" onclick={() => batch('cancel')}
                ><X size={14} />Annulla</button
              ><button class="text-button" onclick={() => (selectedJobs = [])}
                >Deseleziona</button
              >
            </div>{/if}
          {#if !visibleJobs.length}<div class="empty-queue">
              <ListVideo size={38} strokeWidth={1.2} />
              <h3>
                {snapshot.jobs.length
                  ? 'Nessun risultato'
                  : 'La coda è pronta. Manca solo un link.'}
              </h3>
              <p>
                {snapshot.jobs.length
                  ? 'Prova un’altra ricerca o un altro filtro.'
                  : 'Aggiungi un video, scegli la qualità e al resto pensa YT Saver.'}
              </p>
              {#if !snapshot.jobs.length}<button
                  class="button primary"
                  onclick={() => (view = 'add')}
                  ><Plus size={17} />Nuovo download</button
                >{/if}
            </div>{/if}
          {#each visibleJobs as job (job.id)}<article class="job-row">
              <input
                type="checkbox"
                aria-label={`Seleziona download ${job.title}`}
                checked={selectedJobs.includes(job.id)}
                onchange={(e) =>
                  (selectedJobs = e.currentTarget.checked
                    ? [...selectedJobs, job.id]
                    : selectedJobs.filter((id) => id !== job.id))}
              />
              <div class="job-thumb"><Thumb url={job.thumbnail} /></div>
              <div class="job-body">
                <div class="job-top">
                  <h3 title={job.title}>{job.title}</h3>
                  <span class={`state-badge ${job.state}`}
                    >{#if job.state === 'completed'}<Check
                        size={12}
                      />{:else if active(job)}<LoaderCircle
                        size={12}
                        class="spin"
                      />{/if}{stateNames[job.state] || job.state}</span
                  >
                </div>
                <div class="job-meta">
                  <span>{job.channel}</span><span>·</span><span
                    >{job.options.audio_only
                      ? 'Solo audio'
                      : job.selection?.height
                        ? `${job.selection.height}p · ${job.selection.container.toUpperCase()}`
                        : job.options.video_quality
                          ? `Fino a ${job.options.video_quality}p`
                          : 'Migliore qualità'}</span
                  >
                </div>
                <div
                  class="progress-track"
                  class:indeterminate={active(job) && !job.progress.total}
                >
                  <div style={`width:${percent(job)}%`}></div>
                </div>
                <div class="job-progress">
                  <span
                    >{job.error?.message ||
                      `${bytes(job.progress.downloaded)} / ${job.progress.total == null ? '—' : '~' + bytes(job.progress.total)}${job.progress.detail ? ' · ' + job.progress.detail : ''}`}</span
                  ><span
                    >{active(job) && job.progress.speed
                      ? `${bytes(job.progress.speed)}/s · ${duration(job.progress.eta)} rimasti`
                      : job.state === 'completed'
                        ? bytes(job.progress.downloaded)
                        : ''}</span
                  >
                </div>
              </div>
              <div class="job-actions">
                {#if active(job) || job.state === 'queued'}<button
                    class="icon-button"
                    aria-label={`Pausa ${job.title}`}
                    onclick={() => action(job, 'pause')}
                    ><Pause size={17} /></button
                  ><button
                    class="icon-button"
                    aria-label={`Annulla ${job.title}`}
                    onclick={() => action(job, 'cancel')}
                    ><X size={17} /></button
                  >{:else if job.state === 'paused'}<button
                    class="icon-button"
                    aria-label={`Riprendi ${job.title}`}
                    onclick={() => action(job, 'resume')}
                    ><Play size={17} /></button
                  ><button
                    class="icon-button"
                    aria-label={`Annulla ${job.title}`}
                    onclick={() => action(job, 'cancel')}
                    ><X size={17} /></button
                  >{:else if job.state === 'completed'}<button
                    class="icon-button"
                    aria-label={`Apri file ${job.title}`}
                    onclick={() => openOutput(job, false)}
                    ><Play size={17} /></button
                  ><button
                    class="icon-button"
                    aria-label={`Apri cartella ${job.title}`}
                    onclick={() => openOutput(job, true)}
                    ><FolderOpen size={17} /></button
                  ><button
                    class="icon-button"
                    aria-label={`Rimuovi ${job.title}`}
                    onclick={() => action(job, 'remove')}
                    ><X size={16} /></button
                  >{:else}<button
                    class="icon-button"
                    aria-label={`Riprova ${job.title}`}
                    onclick={() => action(job, 'retry')}
                    ><RefreshCw size={17} /></button
                  ><button
                    class="icon-button"
                    aria-label={`Rimuovi ${job.title}`}
                    onclick={() => action(job, 'remove')}
                    ><X size={17} /></button
                  >{/if}{#if job.state === 'queued'}<div
                    class="reorder-controls"
                  >
                    <button
                      class="icon-button"
                      aria-label={`Sposta su ${job.title}`}
                      onclick={() => reorder(job, -1)}
                      ><ChevronUp size={15} /></button
                    ><button
                      class="icon-button"
                      aria-label={`Sposta giù ${job.title}`}
                      onclick={() => reorder(job, 1)}
                      ><ChevronDown size={15} /></button
                    >
                  </div>{/if}
              </div>
            </article>{/each}
        </section>
      {:else}
        <section class="page-heading compact">
          <div class="eyebrow">A MODO TUO</div>
          <h1>Le preferenze<span>.</span></h1>
          <p>
            Imposta i tuoi default. Cambiali quando vuoi, per ogni download.
          </p>
        </section>
        <div class="settings-layout">
          <section class="panel settings-panel">
            <div class="panel-title">
              <Settings2 size={19} />
              <h2>Download e aspetto</h2>
            </div>
            <Options
              value={settings.defaults}
              onchange={(v) => (settings = { ...settings, defaults: v })}
              onerror={error}
              id="defaults"
            />
            <div class="field-grid">
              <label for="concurrency"
                >Download simultanei<input
                  id="concurrency"
                  type="number"
                  min="1"
                  max="8"
                  bind:value={settings.concurrent_jobs}
                /></label
              ><label for="fragments"
                >Frammenti per download<input
                  id="fragments"
                  type="number"
                  min="1"
                  max="8"
                  value={settings.defaults.fragments}
                  onchange={(e) =>
                    (settings = {
                      ...settings,
                      defaults: {
                        ...settings.defaults,
                        fragments: Number(e.currentTarget.value),
                      },
                    })}
                /></label
              >
            </div>
            <p class="field-help">
              Più parallelismo può aiutare, se la sorgente lo supporta. La
              velocità dipende anche dalla tua connessione.
            </p>
            <label for="theme"
              >Aspetto<select id="theme" bind:value={settings.theme}
                ><option value="dark">Scuro</option><option value="light"
                  >Chiaro</option
                ><option value="system">Come il sistema</option></select
              ></label
            ><button class="button primary" onclick={save}
              ><Check size={17} />Salva impostazioni</button
            >
          </section>
          <section class="panel settings-panel">
            <div class="panel-title">
              <RefreshCw size={19} />
              <h2>Motore di download</h2>
            </div>
            <div class="engine-state">
              <span class="status-dot"></span><strong
                >{updateLabels[snapshot.update.phase] ||
                  snapshot.update.phase ||
                  'Pronto'}</strong
              >
            </div>
            <dl class="engine-details">
              <div>
                <dt>Versione installata</dt>
                <dd>
                  {snapshot.update.installed_version || 'Inclusa nell’app'}
                </dd>
              </div>
              {#if snapshot.update.candidate}<div>
                  <dt>Nuova versione</dt>
                  <dd>{snapshot.update.candidate}</dd>
                </div>{/if}
              <div>
                <dt>Canale</dt>
                <dd>Stabile</dd>
              </div>
              {#if snapshot.update.last_success}
                <div>
                  <dt>Ultimo test riuscito</dt>
                  <dd>
                    {new Date(
                      snapshot.update.last_success * 1000,
                    ).toLocaleString('it-IT')}
                  </dd>
                </div>
              {/if}
              <div>
                <dt>Aggiornamenti</dt>
                <dd>Automatici</dd>
              </div>
            </dl>
            <p class="field-help">
              Ogni aggiornamento viene provato con un download reale. Se il test
              fallisce, resta attiva la versione precedente. Le regressioni
              confermate attivano il ripristino automatico.
            </p>
            {#if snapshot.update.message}<div class="notice">
                {snapshot.update.message}
              </div>{/if}<button
              class="button secondary"
              disabled={['checking', 'installing', 'testing'].includes(
                snapshot.update.phase,
              )}
              onclick={checkUpdate}
              ><RefreshCw size={16} />Controlla e riprova ora</button
            >
            <div class="history">
              <h3>Attività recenti</h3>
              {#if !snapshot.history.length}<p class="field-help">
                  Gli aggiornamenti e i ripristini appariranno qui.
                </p>{/if}{#each snapshot.history as item}<div
                  class="history-row"
                >
                  <span
                    >{item.state === 'activated'
                      ? 'Aggiornato'
                      : item.state === 'rollback'
                        ? 'Ripristino'
                        : item.state === 'rejected'
                          ? 'Versione rifiutata'
                          : 'Controllo'}</span
                  ><span
                    >{item.version || item.from || item.message || '—'}</span
                  >
                </div>{/each}
            </div>
          </section>
        </div>
      {/if}
      <footer class="page-footer">
        <span>YT SAVER · VIDEO E AUDIO, SUL TUO COMPUTER.</span><span
          ><ShieldCheck size={13} />Elaborazione locale</span
        >
      </footer>
    </main>
  </div>
</div>
