# oki-decal 🏁

Stampare **decalcomanie (decals) con toner bianco** sulla stampante **OKI C711WT** — senza RIP commerciale, senza Digital Factory, con un binario Rust che parla direttamente con la stampante.

Nato per stampare le decals delle macchinine rc in scala (Brabham BT19, Senna helmet, ecc.) su film trasparente con sotto-bianco.

---

## Come funziona (in 30 secondi)

```
PNG (con alpha) ──► silhouette bianca (choke 3px)  ─┐
                                                     ├─► 4 piani 1bpp a 600×1200
PNG ──► ICC Cadlink (RGB→CMYK) ──► curve ──► halftone┘        [C, W, M, Y]
                                                                    │
                                                    stream PCL: righe interlacciate
                                                    (4 blocchi/riga: V,V,V,W, packbits)
                                                                    │
                                                        RAW ──► OKI C711WT (USB)
```

- La C711WT ha 4 tamburi C/M/Y/**W** (niente nero) con **cartucce invertite fisicamente** (W,Y,M,C) per avere il bianco depositato per primo (underprint).
- Il firmware mappa i 4 slot di ogni riga ai toner per colore: `[Cyan, White, Magenta, Yellow]` — verificato empiricamente con test a bande.
- Ogni riga raster viene trasmessa come **4 blocchi compressi packbits** (`ESC*b<NNNNNN>V` ×3 + `ESC*b<NNNNNN>W` per chiudere la riga). Nessun selettore di piano: la posizione È il piano.
- L'header PJL viene **omesso** (`--no-pjl` di default): il language monitor dello spooler Windows lo corrompe e la stampante lo stampa come testo.

## Setup

1. **Rust** (stable, toolchain MSVC)
2. **Profili ICC Cadlink** in `oki-decal/profiles/` (NON inclusi nel repo — copyright CADlink):
   - `CADlink Unified RGB.icm` (profilo RGB input)
   - `CadlinkWideCMYK.icm` (profilo CMYK)
   - `WTC_WRGB_Underprint.icm` (output, opzionale)

   Si copiano dall'installazione di Digital Factory (`Rip/system/clinks/`) o dal proprio pacchetto `.pkg64`.

3. **Stampante Windows**: qualsiasi coda collegata alla porta USB della OKI (es. "OKI C711WT"). L'invio usa datatype RAW — il driver non processa nulla, è solo un ponte verso la porta.

```bash
cd oki-decal
cargo build --release
```

## Uso

```bash
# decal con sotto-bianco su tutta la silhouette (default, come il profilo Underprint di DF)
oki-decal decal decals.png

# decal "bianco dove l'ho dipinto" (workflow Illustrator: dipingi bianco su sfondo trasparente)
oki-decal decal numeri.png --white=explicit

# specchiata (per film stampati a rovescio)
oki-decal decal livrea.png --mirror

# supporto diverso (fusione/velocità): plain|labels|transparency|cardstock|glossy|bond...
oki-decal decal etichette.png --media=labels

# test: 4 bande per verificare la mappa piani→toner (C,W,M,Y)
oki-decal bars

# stampare il file generato
powershell -File test/Send-RAW.ps1 -PrinterName "OKI C711WT" -File oki-decal/decal.prn
```

### Ricetta di stampa (dal profilo live DF10 dell'utente)

| Parametro | Valore |
|---|---|
| Risoluzione | 600×1200 dpi, 1 bit/piano |
| Halftone | Floyd-Steinberg (DF usava Variable 260lpi) |
| Bianco | choke 3px, da alpha (o esplicito con knockout CMY) |
| Curve | linearizzazione C/M/Y del profilo, K piegato in CMY (non c'è toner nero) |
| Media | dGlossy, vassoio Multi Purpose |
| Pagina | A4, centrata |

## Struttura

```
oki-decal/src/main.rs   # tutto il pipeline (≈500 righe, zero dipendenze pesanti)
oki-decal/profiles/     # ICC Cadlink (da fornire) + underprint_live.json (ricetta)
test/                   # Send-RAW (spooling), capture_tcp (cattura stream), generatori test
profiles/               # documentazione della ricetta estratta dal setup DF originale
```

## Come è stato ricostruito il protocollo

1. Dal pacchetto `.pkg64` di CADlink (uno ZIP): PDX/PPD/PIE/psqx — tutta la configurazione in XML/PostScript leggibile
2. Cattura dello stream reale di Digital Factory 12 (trial) su una porta TCP loopback
3. Analisi byte-per-byte → righe interlacciate, comandi V/W, terminatori
4. Verifica empirica sulla stampante (test a bande) per la mappa piani→toner

Testato e funzionante su OKI C711WT (cartucce swap W,Y,M,C) con Windows 11.

## Limiti noti

- Single-pass white (il double-white del pacchetto CADlink non è attivabile su C711WT: testato, vedi test `barsdw1/2`)
- Il bianco "esplicito" usa pixel quasi-bianchi (RGB>240) + alpha
- L'header PJL va tenuto disattivato via spooler Windows (`--pjl` per forzare, es. per invio diretto TCP/USB)

## Licenze

Codice dell'app: dell'autore. Profili ICC e file CADlink citati: proprietà dei rispettivi proprietari — il repo non li include.
