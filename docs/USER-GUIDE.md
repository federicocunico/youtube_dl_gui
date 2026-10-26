# Usare YT Saver

In **Nuovo download**, incolla uno o più link YouTube, uno per riga, e premi
**Analizza link** (oppure Ctrl+Invio). Sono supportati video watch, link youtu.be,
Shorts e playlist. La homepage e le pagine canale richiedono un link a un video o
una playlist. Contenuti privati, rimossi, con login obbligatorio e dirette attive
mostrano un errore per elemento, senza bloccare gli altri risultati.

Controlla titolo, canale e thumbnail. Le playlist appaiono progressivamente;
**Ferma analisi** conserva gli elementi già trovati. I duplicati sono segnalati
e inizialmente deselezionati: puoi selezionarli se vuoi scaricarli di nuovo.
I dettagli dei formati delle playlist vengono caricati quando selezioni una
voce. L'anteprima video parte solo su richiesta. Se YouTube impedisce il player,
**Apri su YouTube** apre il browser; puoi comunque scaricare il contenuto.

## Video e audio

Il default salva video e audio in un unico file. **Migliore disponibile** sceglie
la massima qualità della sorgente; **Fino a 1080p**, per esempio, sceglie la
migliore risoluzione disponibile entro quel limite. Non vengono creati pixel
aggiuntivi e il video non viene ricodificato. Il contenitore automatico usa MP4
quando compatibile con le tracce, altrimenti MKV. Scegliendo un contenitore
esplicito, una combinazione incompatibile viene segnalata prima dell'enqueue.

Esempio video: analizza il link, imposta **Fino a 1080p**, scegli la cartella e
premi **Aggiungi alla coda**. **Aggiorna formati** mostra la selezione effettiva.

Attiva **Solo audio** per trasferire soltanto l'audio. **Originale** conserva la
traccia senza ricodifica; MP3 e AAC effettuano una conversione al bitrate scelto.
Il bitrate di conversione non aumenta la qualità della sorgente.

Esempio audio: analizza il link, attiva **Solo audio**, scegli MP3 e 192 kbps,
seleziona la cartella e aggiungi alla coda. I controlli video sono disabilitati.
Tutte queste operazioni sono accessibili da tastiera, incluso il selettore della
cartella. **Personalizza questo elemento** applica opzioni diverse a una sola
voce della selezione. Le impostazioni globali si salvano in **Impostazioni**;
non modificano i job già inseriti.

Puoi modificare il nome del file; il nome finale viene reso valido per Windows.
Un file già esistente non viene sovrascritto: YT Saver aggiunge un suffisso come
`(1)`. Il suggerimento è una previsione: eventuali collisioni sorte nel frattempo
vengono risolte anche alla pubblicazione. Spazio o dimensioni possono essere
ignoti fino al trasferimento; il completamento segue la verifica delle tracce.

## Coda, pausa e ripresa

Il default permette tre job simultanei e quattro frammenti per job. Entrambi i
limiti sono configurabili tra 1 e 8. Due operazioni di metadati e una di
mux/conversione possono lavorare contemporaneamente. Il parallelismo dei
frammenti aiuta soltanto quando il protocollo della sorgente lo supporta;
velocità e limiti dipendono anche dalla rete e da YouTube. Ridurre la concorrenza
lascia finire i lavori attivi prima di avviarne altri.

**Pausa** ferma il worker e conserva i dati temporanei. **Riprendi** aggiorna gli
URL temporanei della sorgente e continua dai parziali quando possibile. Il
messaggio distingue la verifica della ripresa, la ripresa confermata e il
riavvio imposto dalla sorgente. Non tutte le sorgenti supportano la ripresa.
**Annulla** interrompe il job; **Riprova** permette di riavviarlo. Rimuovere una
voce terminata elimina la sua cronologia e gli eventuali temporanei, mai il file
finale. **Apri file** e **Apri cartella** usano le applicazioni locali.

Gli errori transitori hanno al massimo tre tentativi automatici con attesa
crescente e rispetto dei tempi indicati dal server. Gli errori permanenti
restano visibili. Dopo un arresto imprevisto, i job interrotti tornano in pausa;
file già pubblicati e verificati vengono riconciliati senza duplicarli. La
chiusura durante il lavoro chiede se restare aperti o interrompere e chiudere.

## Aggiornamenti del motore

YT Saver controlla le nuove versioni stabili di yt-dlp all'avvio, al massimo una
volta ogni 24 ore, e con **Controlla e riprova ora**. Ogni versione stabile è
ammessa al test, indipendentemente da major/minor o anno. L'installazione pip
avviene in un ambiente privato, senza modificare Python di sistema o i job già
avviati. Le note di rilascio non decidono l'attivazione.

Il candidato deve superare metadati, download reale YouTube video+audio con mux,
download solo audio e verifica ffprobe, entro un limite di 25 MiB e 120 secondi.
Un test non conclusivo conserva la versione funzionante e viene riprovato dopo
1 ora, 6 ore e poi 24 ore. Una regressione confermata rispetto alla versione
precedente mette in quarantena il candidato; un retry manuale o una versione
nuova permettono una nuova prova. Le regressioni confermate dopo l'attivazione
causano rollback automatico e un solo tentativo di recupero per i job interessati.

Offline, errori della sorgente o problemi al disco non vengono interpretati da
soli come regressioni. Se una nuova versione richiede Python o runtime nativi
diversi, YT Saver conserva quella precedente e indica che serve aggiornare l'app.
Il pannello mostra versione, test e cronologia dei ripristini. La coda e le
preferenze restano sul computer; i collegamenti a YouTube e PyPI richiedono rete.
