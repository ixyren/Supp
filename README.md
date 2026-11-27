# Supp

Cliente nativo de WhatsApp multisesión para Windows construido 100% en **Rust** y **egui**, sin WebView2 ni motor de navegador.

Basado en la arquitectura nativa de [ZapFast](https://github.com/crmne/zapfast) (licencia MIT) sobre [whatsapp-rust](https://github.com/oxidezap/whatsapp-rust), adaptado con la estética **Material You 3 Expressive Dark-Lime** y tipografía **Google Sans** de Supp.

## Características principales (v2.0.0)

- **100% Nativo en Rust + egui (Cero WebView2)**: Arranque instantáneo, bajo consumo de memoria y cifrado local SQLCipher.
- **Multisesión aislada**: Vincula múltiples cuentas de WhatsApp en el mismo proceso con almacenamiento, cachés y claves totalmente separadas por sesión.
- **Estética Dark-Lime Material You 3**: Paleta oscura lima (`#0d1009` / `#12150e` / `#181c13` / `#c6ff00`), tipografía **Google Sans** integrada e iconos Lucide.
- **Llamadas, voz y multimedia nativos**: Notas de voz, reproducción integrada de video/GIF/stickers y llamadas VoIP sin navegador.
- **Bandeja del sistema, enlaces externos y autoactualización**: Abre enlaces en el navegador predeterminado, permanece en bandeja y comprueba nuevas versiones desde GitHub Releases (`ixyren/Supp`).

## Licencias y avisos de terceros

Este proyecto deriva su infraestructura nativa de [ZapFast](https://github.com/crmne/zapfast) (Copyright © 2026 Carmine Paolino, licencia MIT). Consulta [`LICENSE`](LICENSE) y [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md) para ver los avisos completos.
