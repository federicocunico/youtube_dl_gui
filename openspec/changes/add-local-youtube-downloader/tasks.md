# Tasks

## 1. Fondazione desktop e runtime

- [x] 1.1 Creare scaffold Svelte/TypeScript/Vite e Tauri 2/Rust nelle directory del design, con toolchain e lockfile; verificare avvio desktop e build locale Windows.
- [x] 1.2 Aggiungere provisioning ripetibile di CPython standalone con pip, FFmpeg/ffprobe e Deno con versioni e SHA-256 bloccati; verificare ogni binario da percorso assoluto e produrre inventario e notice delle licenze.
- [x] 1.3 Definire protocollo JSON Lines, handshake, errori tipizzati e ID job/tentativo nel worker Python; verificare test di round-trip, versione incompatibile, JSON malformato, output troppo grande e crash del worker.
- [x] 1.4 Implementare supervisione Rust con Windows Job Objects, timeout, output limitato e runtime lookup privato; verificare che stop/uscita terminino anche processi figli e che PATH non fornisca strumenti mancanti.
- [x] 1.5 Documentare prerequisiti di sviluppo, comandi di verifica/build e separazione fra runtime dell'app e Python di sistema; eseguire i comandi documentati.

## 2. Persistenza e lifecycle della coda

- [x] 2.1 Creare schema SQLite versione 1 per settings, job, tentativi, ambienti e update history e lock di singola istanza; verificare transazioni, riapertura del database e rifiuto di un secondo scheduler.
- [x] 2.2 Implementare stati e scheduler con job 1-8/default 3, frammenti 1-8/default 4, due slot metadata e un mux; verificare limiti, ordine e riduzione concorrenza con worker controllati.
- [x] 2.3 Implementare snapshot delle opzioni, enqueue bulk e riordino; verificare che cambiare default non muti job esistenti e che l'ordine sopravviva al riavvio.
- [x] 2.4 Implementare pausa, ripresa, cancellazione e recupero post-crash; verificare rilascio slot, conservazione dei parziali, riavvio paused e assenza di duplicati dopo crash fra rename e commit completed.
- [x] 2.5 Implementare classificazione errori, tre retry con backoff/Retry-After e progresso aggregato; verificare errori permanenti, rate limit e isolamento dei fallimenti.
- [x] 2.6 Documentare semantica pausa/ripresa e limiti del parallelismo; verificare che i messaggi UI distinguano restart di una sorgente non resumable e ripresa effettiva.

## 3. Discovery e selezione dei media

- [x] 3.1 Implementare validazione e normalizzazione URL YouTube watch/Shorts/youtu.be/playlist e deduplica per video ID; verificare hostname ingannevoli, homepage, URL malformati e duplicati.
- [x] 3.2 Implementare inspect yt-dlp con metadati e formati normalizzati, senza download completo; verificare fixture valide, thumbnail assente, video privato/rimosso/login e live attivo.
- [x] 3.3 Implementare enumerazione incrementale e cancellabile delle playlist con selezione e dettaglio lazy; verificare risultati parziali, elementi inaccessibili e cancellazione su playlist estesa.
- [x] 3.4 Collegare la UI Aggiungi con input multilinea, risultati selezionabili e preview titolo/canale/thumbnail/durata; verificare il percorso bulk misto fino all'enqueue con test di interazione.
- [ ] 3.5 Implementare playback su richiesta in webview senza privilegi e fallback browser; verificare embed negato, mancato autoplay e rifiuto di accesso IPC/file da contenuto remoto.
- [x] 3.6 Documentare URL supportati, limiti discovery e fallback preview nella guida utente; verificare corrispondenza dei casi documentati con fixture e messaggi UI.

## 4. Output video/audio e destinazioni

- [x] 4.1 Implementare scelta best e risoluzione massima, compatibilità contenitore/codec, modalità solo audio originale o MP3/AAC con preset; verificare selezione senza upscale, no video in audio-only e incompatibilità esplicita del contenitore.
- [x] 4.2 Implementare cartella nativa, default persistenti, override e preview nome file; verificare selezione tastiera, snapshot e nomi riservati/caratteri invalidi/percorso esterno alla destinazione.
- [x] 4.3 Implementare download a stream separati, gestione slot mux, FFmpeg e ffprobe sui percorsi inclusi; verificare con fixture locali tracce attese, durata positiva, audio-only senza video e conversioni dichiarate.
- [x] 4.4 Implementare directory temporanee per job/tentativo e pubblicazione atomica senza overwrite; verificare collisioni concorrenti, file esistente, disco pieno, cartella rimossa e cancel durante mux.
- [x] 4.5 Collegare progressi, ETA, velocità, totali ignoti e azioni apri file/cartella; verificare che completed segua ffprobe e che file esterni rimossi producano un errore comprensibile.
- [x] 4.6 Documentare formati supportati, differenza fra mux e conversione audio, stime e collisioni; verificare un esempio video e uno audio seguendo la guida.

## 5. Aggiornamenti isolati e validazione reale

- [x] 5.1 Implementare ricerca latest stable su PyPI con confronto packaging.version, check startup/24h e comando manuale; verificare aggiornamento oltre cambio anno/schema, esclusione prerelease, offline e note di rilascio non bloccanti.
- [x] 5.2 Implementare staging con pip privato, candidato esatto, wheelhouse a pin completi e verifica hash, indice controllato e nessun sdist; verificare dipendenza incompatibile, hash errato e assenza di mutazioni a system Python/active.
- [x] 5.3 Selezionare e registrare due brevi canary YouTube pubblici con provenienza e tracce separate; verificare realmente entrambi e includere nel manifest formati, limiti e motivazione dell'idoneità.
- [x] 5.4 Implementare healthcheck con metadata, download video+audio e mux, audio-only e ffprobe, budget di 25 MiB/120s e pulizia; verificare reale suite YouTube e test deterministici di file vuoto, traccia mancante, timeout e budget byte.
- [x] 5.5 Implementare confronto candidato/active, fallback al secondo canary e risultati pass/fail/inconclusive; verificare regressione isolata, guasto rete condiviso e sorgente rimossa senza attivazione impropria.
- [x] 5.6 Implementare promozione atomica, puntatori active/previous e journal/recupero staging; verificare crash in ogni fase e job in corso che mantengono il proprio ambiente.
- [x] 5.7 Implementare rollback differenziale post-attivazione, quarantena e un solo recovery retry per job, senza riusare parziali incompatibili; verificare candidate fail/previous pass, entrambi fail e nessun overwrite di completed.
- [x] 5.8 Implementare lock updater, retry inconclusivi a 1h/6h/24h, retry manuale e retention degli ambienti referenziati; verificare assenza di loop al riavvio e protezione degli ambienti usati da worker attivi.
- [x] 5.9 Collegare pannello versioni/progresso/storia/errori con log redatti e guida aggiornamenti; verificare messaggi per rollback, offline e requisito di nuovo runtime nativo.

## 6. Rifinitura dell'esperienza desktop

- [ ] 6.1 Completare navigazione Aggiungi/Coda/Impostazioni, token visivi, temi e stati vuoti/loading/error; verificare screenshot reali e azioni primarie a finestra 1024x700 e alle scale Windows 100/150/200%.
- [x] 6.2 Completare focus, etichette accessibili, scorciatoie e comunicazione degli stati indipendente dal colore; verificare l'intero flusso solo audio usando esclusivamente tastiera.
- [x] 6.3 Limitare frequenza eventi di progresso e costo delle liste; verificare fixture di 100 job con tre transfer simulati e ack pausa/cancel entro 500 ms, documentando macchina e misurazione.
- [x] 6.4 Implementare chiusura con scelta resta aperto/annulla e chiudi e pulizia diagnostica; verificare stop dei figli, recupero paused al riavvio e assenza di URL firmati o dati sensibili nei log.

## 7. Installer e GitHub Actions

- [ ] 7.1 Configurare NSIS per-user Windows x64 con risorse baseline e WebView2 bootstrapper; verificare installazione, avvio e disinstallazione su Windows senza toolchain con percorsi spazi/Unicode.
- [ ] 7.2 Creare harness del pacchetto installato con fixture HTTP locali e controllo origine dei runtime; verificare private pip, protocollo, download/mux/audio e integrazione rollback, facendo fallire la prova se manca un binario incluso anche quando esiste sul PATH.
- [ ] 7.3 Creare workflow PR/main/manuale con permessi minimi, installazioni da lockfile e gate locali equivalenti; verificare che PR senza segreti completino test/build e che main produca installer, checksum, inventario e notice.
- [ ] 7.4 Creare workflow tag v* con controllo versione e canary YouTube obbligatorio, pubblicazione solo dell'artefatto verificato e stato firma dichiarato; verificare che test fallito o inconclusivo impedisca la pubblicazione.
- [ ] 7.5 Documentare download degli artefatti CI e delle release, installazione, requisito eventuale WebView2, provenienza/licenze e firma; verificare guida su una macchina Windows pulita.

## 8. Accettazione integrata

- [ ] 8.1 Eseguire scenari integrati video muxato, audio-only, bulk/playlist, errore per elemento, cancel/restart e impostazioni persistenti sul pacchetto installato; conservare risultati e inventario della build verificata.
- [x] 8.2 Eseguire aggiornamento motore con download reale, regressione indotta e rollback automatico mentre esistono job attivi; verificare nuove assegnazioni, job preservati, retry unico e assenza di loop dopo riavvio.
- [ ] 8.3 Eseguire tutti i gate locali equivalenti alla CI e validazione OpenSpec strict; verificare esito positivo e registrare eventuali limiti esterni prima della consegna.
- [ ] 8.4 Se durante la consegna viene eseguito un push che attiva workflow, identificarne il run per commit, monitorarlo fino a termine, riprodurre/correggere eventuali errori e monitorare il nuovo push; riportare esito finale o blocker esplicito. In assenza di push, registrare non applicabile senza crearne uno solo per questa attività.
