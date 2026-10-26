# Design

## Context

Vedere `proposal.md` per motivazione e ambito. L'ispezione del repository ha trovato solo configurazione OpenSpec e directory di skill: nessun sorgente, lockfile, test, workflow o specifica preesistente. Non ci sono convenzioni applicative da conservare o dati da migrare.

Il target iniziale è Windows 10/11 x64. L'utente ha scelto esplicitamente di tentare ogni aggiornamento stabile del motore, verificarlo con un download reale e fare rollback automatico se necessario. Non si usano major/minor, anno o regex delle release note come criteri di ammissione. Questo non elimina i requisiti minimi del runtime: se una versione non può essere installata, resta attiva la precedente e viene spiegata la necessità di una nuova build dell'app.

## Goals / Non-Goals

**Goals:**
- Mantenere il thread UI libero da estrazione, I/O, installazione pip e post-processing.
- Rendere persistenti e verificabili le transizioni di coda e aggiornamento.
- Consentire installazione e utilizzo senza strumenti di sviluppo installati dall'utente.
- Limitare la superficie privilegiata esposta a contenuti remoti.

**Non-Goals:**
- Un framework per plugin arbitrari: il primo adapter è solo yt-dlp.
- Sostituzione automatica di Python, FFmpeg o runtime JavaScript nativi: questi seguono le release dell'app.
- Velocità garantita, transcodifica video implicita, oppure compatibilità universale di un singolo test con ogni video YouTube.

## Decisions

### 1. Tauri 2, Svelte/TypeScript e Rust

Frontend Svelte con Vite e CSS a token; Tauri 2 come shell locale; Rust con runtime asincrono come supervisore. La proposta usa una UI web per flessibilità visiva e una shell desktop che non distribuisce un browser Chromium completo. WebView2 è un prerequisito gestito dall'installer.

Alternative considerate: Electron semplifica un ambiente JavaScript uniforme ma distribuisce un browser aggiuntivo; Qt/Python avvicina UI e motore ma complica l'isolamento degli aggiornamenti Python; .NET/WinUI è adatto a Windows ma rende meno riutilizzabile la UI. Tauri richiede tre toolchain in sviluppo; la CI ne nasconde la complessità all'utente finale.

Struttura proposta, da creare nell'apply:

```text
src/                         UI, componenti, stato tipizzato, temi
src-tauri/src/               comandi, scheduler, persistenza, processi, updater
worker/                      adapter Python e protocollo JSON
resources/                   manifest runtime, canary e licenze
scripts/                     provisioning runtime, packaging e smoke test
tests/                       fixture, integrazione, UI e controlli pacchetto
.github/workflows/           verify/build e release Windows
```

Il nome pubblico del prodotto e l'icona possono essere scelti durante l'implementazione senza cambiare contratti o architettura; usare il nome del repository come identificatore provvisorio.

### 2. Worker Python privato e protocollo ristretto

Includere una distribuzione CPython standalone `install_only` Windows x64 con pip verificato in CI, versione supportata dalla baseline yt-dlp, più Deno e FFmpeg/ffprobe bloccati per versione e digest. Usare un Python completo redistribuibile, evitando di trattare l'embeddable package minimale di CPython come un ambiente pip ordinario.

Un adapter Python versionato con l'app usa l'API yt-dlp e produce JSON Lines su stdout: handshake con versione protocollo e motore, metadati normalizzati, progresso, esito e codici errore. Stderr contiene diagnostica limitata e redatta. Comandi: inspect, enumerate, download, healthcheck. Ogni worker gestisce un job; Rust supervisiona la vita dei processi e i relativi figli FFmpeg/Deno con Windows Job Objects. Linee JSON e output diagnostico hanno limiti di dimensione; gli eventi includono job e attempt ID per ignorare eventi obsoleti.

Rust invoca solo percorsi assoluti verificati delle risorse, senza shell e senza ricerca sul PATH. Disabilitare configurazioni yt-dlp dell'utente, plugin esterni, user-site Python e configurazioni pip esterne. Il frontend invia oggetti tipizzati, non argomenti shell o opzioni yt-dlp arbitrarie. Gli ambienti includono `yt-dlp[default]` e dipendenze solver compatibili; i componenti remoti non vengono scaricati ad hoc fuori dal percorso di aggiornamento controllato.

Alternativa: eseguibile yt-dlp congelato aggiornabile; è più semplice da distribuire ma non soddisfa direttamente la richiesta di pacchetti pip locali aggiornabili. Un server HTTP localhost aggiungerebbe autenticazione e gestione porte: stdin/stdout basta per questo protocollo.

### 3. Coda SQLite e budget delle risorse

SQLite, gestito da Rust, conserva settings, job, snapshot delle opzioni, ordine, tentativi, percorsi temporanei/finali, ambiente assegnato e storia aggiornamenti. Transazioni per ogni cambio di stato; progresso aggregato persistito a frequenza ridotta. Un lock di singola istanza evita due scheduler sulla stessa coda.

Lo scheduler permette 1-8 job, default 3; 1-8 frammenti per job, default 4. Due slot separati per metadati e uno per mux/conversione evitano saturazione; un job in attesa di mux mantiene lo slot job. Ridurre i limiti non interrompe lavori attivi. Il parallelismo dei frammenti è efficace soltanto nei protocolli che lo supportano: non promettere accelerazione lineare. Backoff per errori transitori, tre retry, jitter e rispetto Retry-After; gli errori permanenti richiedono un'azione utente.

Gli stati sono quelli di `download-queue`. Pause e cancel terminano l'albero dei processi e liberano gli slot; pause conserva i `.part`, cancel li rende eliminabili con una pulizia esplicita. La ripresa ri-estrae URL temporanei e riparte dai parziali quando possibile. Al riavvio, lavori non terminali diventano paused; prima di riprendere si riconcilia il file finale, per recuperare un crash fra verifica output e commit dello stato completed.

Ogni job ha una directory temporanea nella destinazione. Il nome finale è prenotato senza sovrascritture, con suffisso univoco per collisioni anche concorrenti. Dopo ffprobe e controllo dimensione/durata, rename atomico sullo stesso volume. Validare percorsi, nomi riservati e spazio; le stime da metadati possono essere ignote. I trasferimenti non espongono file finali incompleti.

### 4. Selezione formati e flusso UI

La pagina Aggiungi contiene campo URL multilinea, azione Analizza e lista di risultati selezionabili con anteprima. Per playlist, enumerazione incrementale e cancellabile; dettaglio completo dei formati solo sugli elementi selezionati, evitando un processo per ogni voce. Validare hostname esatto e identificatori; deduplicare per video ID. Le homepage chiedono un URL media.

Un pannello mostra thumbnail, titolo, canale, durata, nome file, cartella, toggle Solo audio, qualità e contenitore. Default video: miglior qualità disponibile, contenitore automatico compatibile, preferendo MP4 quando compatibile e MKV altrimenti, senza ricodifica video. Se il contenitore è esplicitamente scelto, non cambiarlo silenziosamente. Audio: originale o MP3/AAC con preset espliciti, spiegando quando avviene conversione. Mostrare il formato effettivo e le stime ignote.

La vista Coda espone stati, percentuali e byte, velocità, ETA, controlli per elemento e selezione multipla. La vista Impostazioni contiene default, concorrenza e stato aggiornamenti. Token per superfici, bordi, accenti, tipografia e spaziatura; layout adattivo con temi chiaro/scuro, focus evidente, nomi accessibili e messaggi italiani. Aggiornamenti del progresso limitati a circa 5 eventi al secondo per job; liste virtualizzate se necessario.

L'anteprima parte solo su click, in una webview remota separata senza capability Tauri, senza navigazione a contenuti locali e senza bridge IPC. Restrizioni di origine e navigazione; nessun player remoto nella webview privilegiata. Fallback Apri su YouTube nel browser quando l'embed fallisce. Le immagini passano da un fetch controllato e limitato, senza autorizzare HTML remoto nella UI. Testare esplicitamente il tentativo di chiamata ai comandi locali da remoto.

### 5. Aggiornamento pip transazionale con test reale

Layout sotto la directory dati per-user dell'app:

```text
engines/<environment-id>/     ambiente Python privato completo
staging/<attempt-id>/        installazione candidata non attiva
state.sqlite                 coda e puntatori active/previous
update-tests/<attempt-id>/   file di prova temporanei
```

Conservare baseline inclusa nel pacchetto e versione precedente validata. Creare ambienti con il runtime privato, senza dipendere da Python globale. Ogni ambiente è immutabile dopo validazione. Gli aggiornamenti non modificano gli ambienti già assegnati ai job.

Sequenza:
1. Check asincrono PyPI all'avvio, se scadute 24 ore, o su richiesta. Versioni confrontate con `packaging.version`, canale stable senza prerelease. Note di rilascio facoltative e solo informative.
2. Lock esclusivo updater. Risolvere candidato esatto e dipendenze per il Python incluso, scaricando solo wheel da PyPI via HTTPS con verifica SHA-256 dai metadati e nessuna build sdist. Salvare il manifest risolto; installare con pip privato da wheelhouse locale, configurazione isolata e pin completi. Fallimenti di risoluzione, integrità o installazione non toccano active.
3. Verificare handshake, import, versione, protocollo e self-check adapter. Provare metadati e download reali tramite gli stessi percorsi usati dai job.
4. Manifest canary incluso nell'app: due video YouTube pubblici brevi, con provenienza e idoneità all'uso di test documentate, tracce separate e audio, senza login. Selezionare e verificare gli URL durante l'implementazione del validatore, preferendo fixture mantenute da yt-dlp; non richiedere upload su account dell'utente. Testare il primo e usare il secondo se il primo non è disponibile. Selezionare piccoli formati fino a 360p; download completo con limite 25 MiB per suite, 120 secondi complessivi e un solo test attivo; contare i byte cumulativi ed interrompere anche i figli al superamento dei limiti. Scaricare video+audio, muxare, scaricare solo audio e verificare output con ffprobe. Non simulare questo passaggio durante gli aggiornamenti reali.
5. Se il candidato fallisce, riprovare lo stesso canary con active quando necessario per distinguere regressione da guasto esterno. Candidato fallisce / active passa: candidato rifiutato e in quarantena. Entrambi falliscono o nessun canary disponibile: test inconclusivo, active conservato, nuovo tentativo dopo backoff (1h, 6h, poi 24h). In tutti i casi pulire gli output di prova.
6. Successo: commit SQLite atomico dei puntatori previous e active e del rapporto test. Solo i nuovi worker usano il nuovo active. Un journal permette recupero da crash; staging non validati non diventano attivi. Eliminare vecchi ambienti solo quando non sono active, previous, baseline o referenziati da job in corso.

Regressioni dopo attivazione: un errore engine/protocollo o post-processing avvia un controllo differenziale serializzato sul canary, candidato contro previous. Errori di filesystem, autenticazione, video rimosso e rete non innescano direttamente rollback. Se il candidato fallisce e previous passa, cambiare active atomicamente, mettere in quarantena il candidato, fermare e rimettere in coda i job non terminali di quell'ambiente una sola volta sulla versione ripristinata. Usare directory temporanee per tentativo per non mescolare parziali incompatibili; mantenere intatti file completed. Evitare oscillazioni se entrambi falliscono. Un candidato in quarantena richiede nuova versione o retry manuale; nessun ciclo a ogni avvio.

Alternativa: aggiornamento in-place con `pip install -U`; espone lavori attivi e rollback a installazioni parziali. Alternativa: regex sulle note; non misura il funzionamento reale ed è stata esclusa dalla decisione dell'utente.

### 6. Packaging e CI

NSIS per un setup `.exe` per-user. Bundlare risorse native e runtime completi; WebView2 usa il bootstrapper gestito da Tauri e la documentazione espone il possibile accesso rete. Non promettere un eseguibile portatile singolo. Pin di toolchain, lockfile npm/Cargo/Python e digest per download binari. Includere licenze e provenienza dei build FFmpeg/Deno/Python; scegliere e documentare un build FFmpeg redistribuibile compatibile con mux/conversioni supportati.

Workflow verify/build per PR, main e workflow_dispatch; workflow release per tag `v*` con controllo versione. PR senza segreti e con permessi read-only; release con contents:write limitato al job di pubblicazione. Cache keyed sui lockfile; nessuna dipendenza da un generico latest per costruire la baseline. Gli aggiornamenti runtime invece cercano intenzionalmente la versione stable più recente.

Gate deterministici: lint/typecheck, test di scheduler e protocollo, test updater con versioni/guasti simulati, fixture multimediali servite localmente e UI. Smoke del pacchetto installato su Windows usando esclusivamente i percorsi interni: avvio, private pip, mux/audio e rollback. Un harness esterno invoca direttamente il worker incluso con fixture HTTP locali riconoscibili dall'estrattore generico yt-dlp e invoca gli stessi servizi Rust dell'updater nei test di integrazione; non introdurre opzioni o comandi di test nella UI di produzione. L'harness verifica esplicitamente l'origine di ogni runtime e usa la stessa build distribuita per worker e strumenti. Separatamente, test reale YouTube manuale e come gate obbligatorio delle release, mai richiesto alle PR non fidate. Se inconclusivo, non pubblicare la release e consentire un rerun.

Artefatti: installer, checksum SHA-256, inventario dipendenze e notice. Release pubblicata solo dopo i gate, dallo stesso artefatto verificato. Firma Authenticode se i certificati vengono configurati; altrimenti indicare pacchetto non firmato. Dopo eventuali push durante l'apply, monitorare i workflow del commit fino all'esito finale e correggere i fallimenti come richiesto dalle istruzioni del progetto.

## Risks / Trade-offs

- [YouTube cambia o limita le richieste] → motore aggiornabile, budget di concorrenza e retry; errori espliciti, nessuna promessa di funzionamento su ogni contenuto.
- [Un canary non copre tutti i formati] → test video/audio/mux e rollback differenziale post-attivazione; mantenere sempre una baseline. Un test positivo riduce il rischio, non prova compatibilità universale.
- [Dipendenze nuove richiedono un runtime nativo più recente] → installazione fallisce in staging e conserva active; nuova release dell'app per i runtime.
- [PyPI distribuisce codice eseguibile] → indice e nomi controllati, wheel verificate, nessun sdist o plugin arbitrario; i digest verificano integrità ma non garantiscono assenza di difetti nel codice upstream.
- [Ambienti completi occupano disco] → retention basata sui riferimenti e pulizia staging; preferire affidabilità a aggiornamenti in-place.
- [Canary pubblico rimosso o raggiunto il limite rete] → due sorgenti, risultato inconclusivo, backoff; aggiornamento non blocca i download utente.
- [Installer e aggiornamenti sono complessi su Windows] → smoke installato con percorsi contenenti spazi/Unicode e test crash/lock/permessi.
- [Dipendenza da tre toolchain] → versioni bloccate e un comando documentato per verifica/build locale equivalente alla CI.

## Migration Plan

Non esistono dati precedenti. L'apply crea scaffold, schema SQLite versione 1 e baseline; prima esecuzione verifica le risorse e inizializza l'ambiente per-user. Le future migrazioni SQLite saranno transazionali e versionate; il rollback del motore non modifica lo schema della coda. Gli aggiornamenti dell'app dovranno conservare impostazioni e coda.

Prima distribuzione: eseguire gate deterministici, installare il setup su Windows pulito, completare canary reale e prova di rollback, quindi pubblicare per tag. Rollback di release: ritirare l'asset difettoso e distribuire una versione correttiva; il rollback automatico qui descritto riguarda il motore Python.

## References

Fonti primarie consultate per le scelte di distribuzione; le politiche e i limiti sopra sono decisioni di questo progetto:
- [Tauri: Windows installer](https://v2.tauri.app/distribute/windows-installer/) — setup NSIS e gestione WebView2.
- [Tauri: sidecar](https://v2.tauri.app/develop/sidecar/) — inclusione di eseguibili esterni.
- [yt-dlp README](https://github.com/yt-dlp/yt-dlp/blob/master/README.md) — motore, dipendenze multimediali e aggiornamento pip.
- [yt-dlp EJS](https://github.com/yt-dlp/yt-dlp/wiki/ejs) — solver JavaScript e runtime per YouTube.
- [Python Standalone Builds](https://docs.astral.sh/python-build-standalone/distributions/) — archivi di runtime redistribuibili.
- [Python su Windows](https://docs.python.org/3/using/windows.html#the-embeddable-package) — differenza fra distribuzione completa e pacchetto embeddable.
- [Version specifiers](https://packaging.python.org/en/latest/specifications/version-specifiers/) — confronto versioni e release a calendario.
