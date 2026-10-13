// oki-decal CLI - vedi gui per l'interfaccia grafica
use oki_decal::{bars_stream, decal_stream, DecalOptions};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()).unwrap_or("help") {
        "bars" => {
            let no_pjl = args.iter().any(|a| a == "--no-pjl");
            let s = bars_stream(!no_pjl);
            std::fs::write("bars.prn", &s).expect("scrittura bars.prn");
            println!("bars.prn ({} byte). Invia: powershell -File test/Send-RAW.ps1 -PrinterName \"OKI C711WT\" -File bars.prn", s.len());
        }
        "decal" => {
            let input = args.get(2).expect("uso: oki-decal decal <immagine> [opzioni]");
            let mut opts = DecalOptions {
                explicit_white: args.iter().any(|a| a.contains("white=explicit")),
                mirror: args.iter().any(|a| a == "--mirror"),
                ..Default::default()
            };
            if let Some(m) = args.iter().find(|a| a.starts_with("--media=")) {
                opts.media = m[8..].to_string();
            }
            opts.with_pjl = args.iter().any(|a| a == "--pjl");
            match decal_stream(input, &opts) {
                Ok(s) => {
                    std::fs::write("decal.prn", &s).expect("scrittura decal.prn");
                    println!("decal.prn ({} byte). Invia: powershell -File test/Send-RAW.ps1 -PrinterName \"OKI C711WT\" -File decal.prn", s.len());
                }
                Err(e) => eprintln!("ERRORE: {e}"),
            }
        }
        _ => {
            println!("oki-decal - decal printer OKI C711WT");
            println!("  oki-decal bars                     -> bars.prn (test piani)");
            println!("  oki-decal decal <img> [opzioni]    -> decal.prn");
            println!("opzioni: --white=explicit --mirror --media=NOME --pjl");
            println!("GUI: oki-decal-gui");
        }
    }
}
