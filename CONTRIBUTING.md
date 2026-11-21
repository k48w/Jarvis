# Contribuindo com o J.A.R.V.I.S. — The Singularity Edition

Obrigado pelo interesse em contribuir com o **J.A.R.V.I.S. V8.0**!

## Ambiente de Desenvolvimento

1. Instale **Node.js 20+**, **Rust stable (MSVC)** e **Visual Studio C++ Build Tools** no Windows 10/11 x64.
2. Clone o repositório e instale as dependências:
   ```powershell
   git clone https://github.com/k48w/Jarvis.git
   cd Jarvis
   npm ci
   ```
3. Execute os testes unitários em Rust e o typecheck do frontend antes de abrir um Pull Request:
   ```powershell
   cargo test --manifest-path src-tauri/Cargo.toml
   npm run build
   node tests/scene.mjs
   ```

## Padrões de Código

- **Zero PowerShell para automação interna de tela/áudio**: utilize diretamente as APIs nativas Win32 via crate `windows` em Rust (`DXGI`, `PrintWindow`, `waveIn`, `SendInput`, `OpenClipboard`, `IFileOpenDialog`) para máxima performance e compatibilidade com antivírus.
- **Segurança de credenciais**: nunca armazene chaves de API no código ou em arquivos versionados; utilize sempre o `keyring` (Gerenciador de Credenciais do Windows).
- **Commits semânticos**: use prefixos claros como `feat:`, `fix:`, `docs:`, `perf:` ou `chore:`.
