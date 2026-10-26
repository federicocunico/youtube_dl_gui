import { test, expect, type Page, type Locator } from '@playwright/test';

async function mock(page: Page, count = 0) {
  await page.addInitScript(
    ({ count }) => {
      const w = window as any;
      w.isTauri = true;
      const defaults = {
        destination: 'C:\\Downloads',
        audio_only: false,
        video_quality: 0,
        container: 'auto',
        audio_format: 'original',
        audio_bitrate: 192,
        fragments: 4,
      };
      let state = {
        settings: { defaults, concurrent_jobs: 3, theme: 'dark' },
        jobs: Array.from({ length: count }, (_, i) => ({
          id: `job-${i}`,
          title: `Fixture ${i}`,
          channel: 'Fixture channel',
          filename: `Fixture ${i}`,
          url: 'https://youtu.be/BaW_jenozKc',
          state: i < 3 ? 'downloading' : 'queued',
          position: i,
          options: { ...defaults },
          progress: {
            downloaded: 1024,
            total: 10240,
            speed: i < 3 ? 500 : 0,
            eta: 20,
            detail: 'Scaricamento',
          },
        })),
        history: [],
        update: { phase: 'current', installed_version: '2026.10.21' },
      };
      const callbacks: Record<number, (e: any) => void> = {};
      const listeners: Record<string, number> = {};
      let next = 1;
      const emit = (event: string, payload: unknown) => {
        const fn = callbacks[listeners[event]];
        if (fn) fn({ event, id: 1, payload: structuredClone(payload) });
      };
      w.__send = (event: string, payload: any) => {
        if (event === 'snapshot') state = structuredClone(payload);
        emit(event, payload);
      };
      w.__snapshot = () => structuredClone(state);
      if (count)
        setInterval(() => {
          state.jobs
            .filter((j) => j.state === 'downloading')
            .forEach((j) => (j.progress.downloaded += 10));
          emit('snapshot', state);
        }, 200);
      w.__commands = [];
      w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
      w.__TAURI_INTERNALS__ = {
        transformCallback: (fn: (e: any) => void) => {
          const id = next++;
          callbacks[id] = fn;
          return id;
        },
        unregisterCallback: (id: number) => delete callbacks[id],
        invoke: async (command: string, args: any) => {
          w.__commands.push({ command, args, time: performance.now() });
          if (command === 'plugin:event|listen') {
            listeners[args.event] = args.handler;
            return next++;
          }
          if (command === 'snapshot') return structuredClone(state);
          if (command === 'plugin:dialog|open') return 'C:\\Chosen';
          if (command === 'plugin:dialog|confirm')
            return w.__confirmResult !== false;
          if (command === 'plugin:dialog|message')
            return w.__confirmResult === false
              ? 'Resta aperto'
              : 'Annulla e chiudi';
          if (command === 'open_output')
            throw new Error('Il file è stato spostato o eliminato.');
          if (command === 'preview' && w.__previewFails)
            throw new Error('Anteprima non disponibile. Usa Apri su YouTube.');
          if (command === 'discover') {
            const media = {
              id: 'BaW_jenozKc',
              url: 'https://www.youtube.com/watch?v=BaW_jenozKc',
              title: 'A little test video',
              channel: 'Test channel',
              duration: 10,
              filename: 'A little test video',
              available: true,
              formats: [{ format_id: 'v', height: 1080 }],
              selection: { height: 1080, container: 'mp4', acodec: 'mp4a' },
            };
            setTimeout(() => {
              emit('discovery', { session: args.id, item: media });
              emit('discovery', {
                session: args.id,
                item: { ...media, duplicate: true },
              });
              emit('discovery', {
                session: args.id,
                item: {
                  id: 'bad',
                  source: 'invalid',
                  error: 'Inserisci un URL YouTube valido.',
                  available: false,
                },
              });
              emit('discovery', { session: args.id, done: true });
            }, 0);
            return;
          }
          if (command === 'enqueue') {
            for (const item of args.items)
              state.jobs.push({
                ...item.media,
                id: crypto.randomUUID(),
                options: item.options,
                state: 'queued',
                position: state.jobs.length,
                progress: { downloaded: 0, detail: '' },
              });
            emit('snapshot', state);
            return ['new'];
          }
          if (command === 'control_job') {
            const job = state.jobs.find((j) => j.id === args.id);
            if (job)
              job.state =
                args.action === 'pause'
                  ? 'paused'
                  : args.action === 'cancel'
                    ? 'cancelled'
                    : 'queued';
            w.__ack = performance.now();
            emit('snapshot', state);
            return;
          }
          if (command === 'save_settings') {
            state.settings = args.settings;
            emit('snapshot', state);
            return;
          }
          if (command === 'thumbnail') throw new Error('No thumbnail fixture');
          if (command === 'check_update') return state.update;
          if (command === 'plugin:event|unlisten') return;
          return {};
        },
      };
    },
    { count },
  );
}

async function tabTo(page: Page, target: Locator) {
  for (let i = 0; i < 60; i++) {
    if (await target.evaluate((el) => el === document.activeElement)) return;
    await page.keyboard.press('Tab');
  }
  throw new Error('Control not reachable by keyboard');
}

test('bulk input, duplicates and selected items reach native enqueue', async ({
  page,
}) => {
  await mock(page);
  await page.goto('/');
  await expect(page).toHaveTitle(/YT Saver/);
  await expect(
    page.getByRole('link', { name: 'YT Saver, nuovo download' }),
  ).toBeVisible();
  await page
    .getByRole('textbox', { name: 'URL YouTube, uno per riga' })
    .fill('https://youtu.be/BaW_jenozKc\ninvalid');
  await page.getByRole('button', { name: 'Analizza link' }).click();
  await expect(
    page.getByText('Duplicato · seleziona per aggiungerlo di nuovo'),
  ).toBeVisible();
  await expect(
    page.getByText('Inserisci un URL YouTube valido.'),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Aggiungi alla coda' }).click();
  await expect(
    page.getByRole('heading', { name: 'La tua coda.' }),
  ).toBeVisible();
  const sent = await page.evaluate(() =>
    (window as any).__commands.find((v: any) => v.command === 'enqueue'),
  );
  expect(sent.args.items).toHaveLength(1);
  expect(sent.args.items[0].media.title).toBe('A little test video');
});

test('audio-only path is keyboard operable including folder selection', async ({
  page,
}) => {
  await mock(page);
  await page.goto('/');
  await tabTo(
    page,
    page.getByRole('textbox', { name: 'URL YouTube, uno per riga' }),
  );
  await page.keyboard.type('https://youtu.be/BaW_jenozKc');
  await page.keyboard.press('Control+Enter');
  await expect(page.getByText('1 selezionati')).toBeVisible();
  const audio = page.getByRole('switch', { name: /Solo audio/ });
  await tabTo(page, audio);
  await page.keyboard.press('Space');
  await expect(page.getByLabel('Qualità video')).toBeDisabled();
  await tabTo(page, page.getByRole('button', { name: 'Scegli cartella' }));
  await page.keyboard.press('Enter');
  await expect(page.getByLabel('Cartella di download')).toHaveValue(
    'C:\\Chosen',
  );
  await tabTo(page, page.getByRole('button', { name: 'Aggiungi alla coda' }));
  await page.keyboard.press('Enter');
  await expect(
    page.getByRole('heading', { name: 'La tua coda.' }),
  ).toBeVisible();
  const sent = await page.evaluate(() =>
    (window as any).__commands.find((v: any) => v.command === 'enqueue'),
  );
  expect(sent.args.items[0].options.audio_only).toBe(true);
});

test('100-job queue acknowledges pause and cancel within 500ms', async ({
  page,
}) => {
  await mock(page, 100);
  await page.goto('/');
  await page.getByRole('button', { name: 'Coda download' }).click();
  for (const [label, state] of [
    ['Pausa Fixture 0', 'In pausa'],
    ['Annulla Fixture 1', 'Annullato'],
  ]) {
    const elapsed = await page
      .getByRole('button', { name: label, exact: true })
      .evaluate((el) => {
        const start = performance.now();
        (el as HTMLButtonElement).click();
        return new Promise<number>((resolve) => {
          const wait = () => {
            if ((window as any).__ack >= start)
              resolve((window as any).__ack - start);
            else requestAnimationFrame(wait);
          };
          wait();
        });
      });
    expect(elapsed).toBeLessThan(500);
    test.info().annotations.push({
      type: 'ack-latency-ms',
      description: `${label}: ${elapsed.toFixed(2)}`,
    });
    await expect(page.getByText(state, { exact: true })).toBeVisible();
  }
  await page.screenshot({
    path: 'test-results/queue-100.png',
    fullPage: false,
  });
});

test('missing files and update errors remain actionable', async ({ page }) => {
  await mock(page, 1);
  await page.goto('/');
  await page.getByRole('button', { name: 'Coda download' }).click();
  await page.evaluate(() => {
    const w = window as any;
    const state = w.__snapshot();
    state.jobs[0].state = 'completed';
    w.__send('snapshot', state);
  });
  await page
    .getByRole('button', { name: 'Apri file Fixture 0', exact: true })
    .click();
  await expect(page.getByRole('alert')).toContainText('spostato o eliminato');
  await page.getByRole('button', { name: 'Impostazioni', exact: true }).click();
  await page.evaluate(() => {
    const w = window as any;
    const state = w.__snapshot();
    state.update = {
      phase: 'error',
      installed_version: '2026.10.21',
      message: 'Serve aggiornare YT Saver per il nuovo runtime Python.',
    };
    w.__send('snapshot', state);
  });
  await expect(
    page.getByText('Serve aggiornare YT Saver per il nuovo runtime Python.'),
  ).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Controlla e riprova ora' }),
  ).toBeEnabled();
  for (const [phase, label, message] of [
    [
      'inconclusive',
      'Test da riprovare',
      'Rete non disponibile. Conservata la versione attiva.',
    ],
    [
      'rollback',
      'Versione precedente ripristinata',
      'Ripristinata automaticamente la versione precedente.',
    ],
  ]) {
    await page.evaluate(
      ({ phase, message }) => {
        const w = window as any;
        const state = w.__snapshot();
        state.update = {
          phase,
          message,
          installed_version: '2026.10.21',
          last_success: 1793011764,
        };
        w.__send('snapshot', state);
      },
      { phase, message },
    );
    await expect(page.getByText(label, { exact: true })).toBeVisible();
    await expect(page.getByText(message, { exact: true })).toBeVisible();
    await expect(
      page.getByText('Ultimo test riuscito', { exact: true }),
    ).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'Controlla e riprova ora' }),
    ).toBeEnabled();
  }
});

test('unavailable embedded preview offers the browser fallback', async ({
  page,
}) => {
  await mock(page);
  await page.goto('/');
  await page.getByLabel('URL YouTube').fill('https://youtu.be/BaW_jenozKc');
  await page.getByRole('button', { name: 'Analizza link' }).click();
  await page.getByRole('button', { name: 'Riproduci anteprima' }).waitFor();
  await page.evaluate(() => {
    (window as any).__previewFails = true;
  });
  await page.getByRole('button', { name: 'Riproduci anteprima' }).click();
  await expect(page.getByRole('alert')).toContainText('Usa Apri su YouTube');
  await page.getByRole('button', { name: 'Apri su YouTube' }).click();
  const commands = await page.evaluate(() => (window as any).__commands);
  expect(commands.some((entry: any) => entry.command === 'open_youtube')).toBe(
    true,
  );
});

test('close confirmation preserves the app until accepted', async ({
  page,
}) => {
  await mock(page, 3);
  await page.goto('/');
  await expect(
    page.getByRole('button', { name: 'Coda download' }),
  ).toBeVisible();
  await page.evaluate(() => {
    const w = window as any;
    w.__confirmResult = false;
    w.__send('close-request', null);
  });
  await page.waitForTimeout(100);
  expect(
    await page.evaluate(() =>
      (window as any).__commands.some((c: any) => c.command === 'shutdown'),
    ),
  ).toBe(false);
  await page.evaluate(() => {
    const w = window as any;
    w.__confirmResult = true;
    w.__send('close-request', null);
  });
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__commands.some((c: any) => c.command === 'shutdown'),
      ),
    )
    .toBe(true);
});

for (const scale of [1, 1.5, 2])
  test(`layout at 1024x700, scale ${scale}`, async ({ browser }) => {
    const context = await browser.newContext({
      viewport: { width: 1024, height: 700 },
      deviceScaleFactor: scale,
    });
    const page = await context.newPage();
    await mock(page);
    await page.goto('http://127.0.0.1:1420');
    await expect(
      page.getByRole('button', { name: 'Analizza link' }),
    ).toBeInViewport();
    await expect(
      page.getByRole('button', { name: 'Aggiungi alla coda' }),
    ).toBeInViewport();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: `test-results/add-${scale}.png`,
      fullPage: true,
    });
    await context.close();
  });
