# Proposal

## Why

Serve un'app desktop locale che permetta di scaricare contenuti YouTube con una UI moderna, senza installare manualmente Python o strumenti da terminale. Il motore deve restare aggiornabile quando YouTube cambia, proteggendo gli utenti dalle regressioni mediante download di test e rollback automatico.

## What Changes

- Introdurre una UI desktop Windows con inserimento URL, anteprima di thumbnail, titolo, canale, durata e nome di destinazione; riproduzione di anteprima opzionale con fallback al browser.
- Supportare video con audio muxato, solo audio, scelta delle qualità disponibili, contenitore e cartella di destinazione, con impostazioni globali e override per elemento.
- Gestire URL multipli e playlist con selezione degli elementi, coda persistente, parallelismo limitato e configurabile, progresso, velocità, ETA, annullamento, ripresa e retry.
- Aggiornare automaticamente a ogni nuova versione stabile di yt-dlp disponibile, senza blocco major/minor o per anno: installazione pip privata, download reale di prova, verifica multimediale, attivazione atomica e rollback. Le note di rilascio hanno funzione informativa.
- Distribuire un installer Windows x64 `.exe` già compilato da GitHub Actions, con runtime e strumenti inclusi, artefatti CI e pubblicazione su GitHub Releases per tag di versione.
- Mantenere elaborazione, coda, impostazioni e file sul computer dell'utente, senza backend cloud applicativo.

## Capabilities

### New Capabilities

- `media-discovery`: validazione URL YouTube, metadati, anteprime ed espansione bulk/playlist.
- `download-configuration`: modalità video/audio, qualità, contenitori e destinazioni dei file.
- `download-queue`: scheduling persistente, concorrenza, progressi e recupero degli errori.
- `engine-updates`: aggiornamenti automatici isolati, download di test, promozione e rollback del motore.
- `desktop-experience`: navigazione, accessibilità, stati operativi e integrazione locale.
- `windows-distribution`: installer autonomo e pipeline GitHub di build, verifica e distribuzione.

### Modified Capabilities

Nessuna: non esistono specifiche o implementazioni precedenti.

## Impact

Il repository attualmente contiene soltanto la struttura OpenSpec; non ci sono codice, test o workflow da migrare. La proposta introduce Svelte/TypeScript con Vite, Tauri 2/Rust, un worker Python privato con yt-dlp, FFmpeg/ffprobe e runtime JavaScript compatibile con il motore. CI richiederà runner Windows, toolchain bloccate, inventario delle dipendenze e licenze dei binari redistribuiti.

Assunzioni: prima release per Windows 10/11 x64; `.exe` indica un installer completo, non un singolo binario portatile. Interfaccia iniziale in italiano. Fuori ambito: altri siti, autenticazione/cookie, live stream attivi, DRM, build macOS/Linux, auto-update dell'intera app e garanzie di velocità indipendenti da rete/YouTube. L'embed è best effort e non condiziona i download.
